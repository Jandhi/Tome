//! Engawa: a raised Japanese veranda for large buildings. Built on the same
//! per-floor-extent machinery as [`super::frame::apply_jetty`], but inset rather
//! than grown, and by different amounts per floor so the building tapers:
//!
//! - the cellar and ground floor inset by **2** on every open-air side,
//! - the **top** floor also insets by **2**, so the roof is pulled back in,
//! - the **middle** floors inset by **1**, so they bulge out over the veranda
//!   (the tapered, overhanging engawa silhouette),
//! - the whole building is raised one block onto a wooden platform,
//! - a **2-deep wooden deck** wraps the entire building perimeter — computed as a
//!   dilation band around the merged ground-living shape, so it runs continuously
//!   around corners and bumps around junctions where two rects meet, and
//! - an **engawa roof** caps the veranda all the way around at the ground-floor
//!   ceiling: slabs on the inner ring (against the wall), stairs on the outer
//!   ring (the dripping eave edge), with the diagonal corners filled.
//!
//! Planning ([`plan_engawa`]) is pure geometry and gates on the deeply-inset
//! ground rects staying usable; [`apply_overhang`] rebuilds the frame with the
//! per-floor extents; [`place_engawa`] lays the deck and roof after the roof.

use std::collections::{HashMap, HashSet};

use crate::editor::Editor;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Placer};
use crate::geometry::{Cardinal, Point2D, Point3D, Rect2D};
use crate::minecraft::{Block, BlockForm};

use super::footprint::Footprint;
use super::footprint::merge::outline_from_rects;
use super::frame::Frame;
use super::pipeline::BuildCtx;

/// Inset (cells) for the cellar, ground floor, and top floor — the deep inset.
const DEEP_INSET: i32 = 2;
/// Inset (cells) for the middle floors — one less, so they overhang the ground.
const MID_INSET: i32 = 1;
/// Depth of the veranda deck, in cells. Matches `DEEP_INSET` so the deck fills
/// out to the nominal footprint edge on cleanly-inset sides.
const DECK_DEPTH: i32 = DEEP_INSET;

/// Smallest side a ground rect may have *after* the deep inset. Below this the
/// room partitioner has nothing usable, so the building falls back to plain
/// walls. With `DEEP_INSET = 2` this needs a nominal side of at least 8 — i.e.
/// engawa only takes on the larger footprints, as intended.
const MIN_INSET_SIDE: i32 = 4;

/// The four cardinals, for ring iteration.
const CARDINALS: [Cardinal; 4] = [
    Cardinal::North,
    Cardinal::East,
    Cardinal::South,
    Cardinal::West,
];

/// The four diagonal offsets, for corner filling: (dx, dz).
const DIAGONALS: [(i32, i32); 4] = [(-1, -1), (-1, 1), (1, -1), (1, 1)];

/// Output of [`plan_engawa`]. `building_footprint` (the deep-inset rects) is what
/// the frame, walls, rooms, and cellar are built from; `mid_rects` are the
/// per-rect middle-floor extents grafted on by [`apply_overhang`]; and
/// `deck_cells` is the continuous veranda ring.
pub struct EngawaPlan {
    /// Deep-inset footprint — cellar, ground floor, and top floor use this.
    pub building_footprint: Footprint,
    /// Middle-floor extents (inset by [`MID_INSET`]), parallel to
    /// `building_footprint.rects()`. Applied to floors `1..count-1`.
    mid_rects: Vec<Rect2D>,
    /// Veranda deck cells: a [`DECK_DEPTH`]-wide band wrapping the whole ground
    /// living shape, continuous around corners and junctions.
    pub deck_cells: Vec<Point2D>,
}

/// Which of a rect's four sides face open air (and so may inset). A side that
/// abuts another rect (a shared seam) stays flush so the inset doesn't open a
/// gap in an interior wall. Mirrors `frame::jetty`'s adjacency rule: even a
/// partial overlap blocks the whole side, keeping each inset extent a single
/// rectangle. (The veranda deck still wraps these sides — it's computed from the
/// merged shape, not per rect — so a partial seam never leaves a deck gap.)
struct OpenAir {
    west: bool,  // -x
    east: bool,  // +x
    north: bool, // -y
    south: bool, // +y
}

fn open_air_sides(rects: &[Rect2D]) -> Vec<OpenAir> {
    let mut sides: Vec<OpenAir> = rects
        .iter()
        .map(|_| OpenAir { west: true, east: true, north: true, south: true })
        .collect();

    for i in 0..rects.len() {
        for j in 0..rects.len() {
            if i == j { continue; }
            let a = &rects[i];
            let b = &rects[j];
            let z_overlap = a.min().y.max(b.min().y) <= a.max().y.min(b.max().y);
            let x_overlap = a.min().x.max(b.min().x) <= a.max().x.min(b.max().x);

            if a.max().x + 1 == b.min().x && z_overlap { sides[i].east = false; }
            if b.max().x + 1 == a.min().x && z_overlap { sides[i].west = false; }
            if a.max().y + 1 == b.min().y && x_overlap { sides[i].south = false; }
            if b.max().y + 1 == a.min().y && x_overlap { sides[i].north = false; }
        }
    }
    sides
}

/// Inset `rect` by `amount` on its open-air sides (shared seams stay flush).
fn inset_one(rect: &Rect2D, s: &OpenAir, amount: i32) -> Rect2D {
    Rect2D::from_points(
        Point2D::new(
            rect.min().x + if s.west { amount } else { 0 },
            rect.min().y + if s.north { amount } else { 0 },
        ),
        Point2D::new(
            rect.max().x - if s.east { amount } else { 0 },
            rect.max().y - if s.south { amount } else { 0 },
        ),
    )
}

/// Largest inset (≤ `cap`) that keeps `rect` at ≥ [`MIN_INSET_SIDE`] on both
/// axes after insetting its open-air sides. Lets a small wing inset by less (or
/// not at all) instead of forcing the whole building to bail — the relaxed gate.
fn affordable_inset(rect: &Rect2D, s: &OpenAir, cap: i32) -> i32 {
    let open_x = s.west as i32 + s.east as i32;
    let open_z = s.north as i32 + s.south as i32;
    let wx = rect.max().x - rect.min().x + 1;
    let wz = rect.max().y - rect.min().y + 1;
    let ax = if open_x == 0 { cap } else { ((wx - MIN_INSET_SIDE) / open_x).max(0) };
    let az = if open_z == 0 { cap } else { ((wz - MIN_INSET_SIDE) / open_z).max(0) };
    cap.min(ax).min(az)
}

/// Try to plan an engawa for `footprint`. Returns `None` (build plain) only when
/// the **core** rect can't afford the full deep inset — i.e. the main mass is too
/// small for a real veranda. Wings inset by as much as they can fit (down to 0),
/// so a building with one big core and small wings still gets an engawa; the deck
/// (a dilation band, below) wraps around the under-inset wings regardless.
pub fn plan_engawa(footprint: &Footprint) -> Option<EngawaPlan> {
    let rects = footprint.rects();
    let sides = open_air_sides(rects);

    // Per-rect inset amounts, clamped to what each rect can afford.
    let deep_amounts: Vec<i32> = rects.iter().zip(&sides)
        .map(|(r, s)| affordable_inset(r, s, DEEP_INSET))
        .collect();
    // The core (rect 0) must afford the full deep inset for a proper veranda.
    if deep_amounts[0] < DEEP_INSET {
        return None;
    }
    // Middle floors inset one less than the ground (but never more), so they
    // bulge out wherever the ground actually inset.
    let mid_amounts: Vec<i32> = deep_amounts.iter().map(|&d| MID_INSET.min(d)).collect();

    let deep: Vec<Rect2D> = rects.iter().zip(&sides).zip(&deep_amounts)
        .map(|((r, s), &a)| inset_one(r, s, a))
        .collect();
    let mid: Vec<Rect2D> = rects.iter().zip(&sides).zip(&mid_amounts)
        .map(|((r, s), &a)| inset_one(r, s, a))
        .collect();

    // Veranda deck: a DECK_DEPTH-wide band around the merged ground-living shape.
    // Computed by dilation (Chebyshev) so it runs continuously around corners and
    // around junctions where a partial seam stopped a rect from insetting — the
    // band simply bumps outward there to keep the ring unbroken.
    let ground_cells: HashSet<Point2D> = deep.iter().flat_map(|r| r.iter()).collect();
    let mut deck: HashSet<Point2D> = HashSet::new();
    for cell in &ground_cells {
        for dx in -DECK_DEPTH..=DECK_DEPTH {
            for dz in -DECK_DEPTH..=DECK_DEPTH {
                let p = Point2D::new(cell.x + dx, cell.y + dz);
                if !ground_cells.contains(&p) {
                    deck.insert(p);
                }
            }
        }
    }
    if deck.is_empty() {
        return None;
    }
    let deck_cells: Vec<Point2D> = deck.into_iter().collect();

    let building_footprint = Footprint::new(outline_from_rects(&deep), deep);
    Some(EngawaPlan { building_footprint, mid_rects: mid, deck_cells })
}

/// Rebuild `frame` with the engawa's per-floor extents: the ground floor (0) and
/// the top floor keep the deep-inset rect; the middle floors use the mid-inset
/// rect (bulging out over the ground floor). `frame` must already have been
/// generated from `plan.building_footprint`.
pub fn apply_overhang(frame: Frame, plan: &EngawaPlan) -> Frame {
    let max_floors = frame.max_floors();
    let extents: Vec<Vec<Option<Rect2D>>> = (0..frame.rect_count()).map(|i| {
        let count = frame.floor_counts()[i];
        let deep = frame.rect_at(i, 0).expect("ground extent at floor 0");
        let mid = plan.mid_rects[i];
        (0..max_floors).map(|f| {
            if f >= count { None }
            else if f == 0 || f == count - 1 { Some(deep) } // ground or top → deep
            else { Some(mid) }                              // middle → bulge out
        }).collect()
    }).collect();
    Frame::with_per_floor_extents(
        frame.footprint().clone(),
        frame.base_y(),
        extents,
        frame.wall_height(),
    )
}

/// Place the veranda deck and its engawa roof. `frame` is the inset, raised frame
/// with the overhang applied (so `floor_y(0)` is the raised interior surface).
pub async fn place_engawa(ctx: &mut BuildCtx<'_>, frame: &Frame, plan: &EngawaPlan) {
    let editor: &Editor = &*ctx.editor;

    // The veranda is a timber deck, so use the frame wood (PrimaryWood) — not the
    // GroundFloor material, which is the interior flooring (e.g. bamboo mosaic).
    let deck_material = ctx.palette
        .get_material(MaterialRole::PrimaryWood)
        .expect("No primary-wood material for engawa deck")
        .clone();
    let roof_material = ctx.palette
        .get_material(MaterialRole::PrimaryRoof)
        .expect("No primary-roof material for engawa roof")
        .clone();

    let mut deck_rng = ctx.rng.derive();
    let mut deck_placer = MaterialPlacer::new(
        Placer::new(&ctx.data.materials, &mut deck_rng),
        deck_material,
    );
    let mut roof_rng = ctx.rng.derive();
    let mut roof_placer = MaterialPlacer::new(
        Placer::new(&ctx.data.materials, &mut roof_rng),
        roof_material,
    );

    // Ground living cells (the deep-inset footprint). The engawa roof's inner
    // ring is the deck band one cell out from these (against the wall); the outer
    // ring is the band two cells out (the dripping eave edge).
    let ground: HashSet<Point2D> = plan
        .building_footprint
        .filled_points()
        .into_iter()
        .collect();
    // Cells the roof rests on (ground ∪ inner ring): used to aim the outer stairs
    // inward, toward the building.
    let mut inward_targets: HashSet<Point2D> = ground.clone();

    // Deck surface sits flush with the interior floor (`floor_y(0) - 1`); the
    // engawa roof rings the wall at the ground-floor ceiling.
    let deck_y = frame.floor_y(0) - 1;
    let roof_y = frame.ceiling_y(0);

    // Classify each deck cell as inner (touches ground in its 3×3) or outer, and
    // lay the planks. Inner cells join `inward_targets` so the outer stairs can
    // find which way the building lies.
    let is_air = |b: &Block| { let s = b.id.as_str(); s == "air" || s == "minecraft:air" };
    // How far below the deck to look for ground before giving up (avoids a
    // runaway column over an unloaded chunk or a deep cliff).
    const MAX_UNDERFILL: i32 = 24;

    let mut inner: Vec<Point2D> = Vec::new();
    let mut outer: Vec<Point2D> = Vec::new();
    for &cell in &plan.deck_cells {
        deck_placer.place_block_forced(
            editor,
            Point3D::new(cell.x, deck_y, cell.y),
            BlockForm::Block,
            None,
            None,
        ).await;

        // The deck is raised; on sloped ground there's open air between the deck
        // plank and the surface. Fill it with a wooden fence skirt (the raised
        // platform's lattice stilts), from just under the deck down to the first
        // solid block.
        for y in ((deck_y - MAX_UNDERFILL)..deck_y).rev() {
            let p = Point3D::new(cell.x, y, cell.y);
            if editor.try_get_block(p).as_ref().map_or(false, is_air) {
                deck_placer.place_block_forced(editor, p, BlockForm::Fence, None, None).await;
            } else {
                break; // hit the ground (or unknown) — stop the column
            }
        }

        let touches_ground = (-1..=1).any(|dx| (-1..=1).any(|dz| {
            ground.contains(&Point2D::new(cell.x + dx, cell.y + dz))
        }));
        if touches_ground {
            inner.push(cell);
            inward_targets.insert(cell);
        } else {
            outer.push(cell);
        }
    }

    let top_slab = HashMap::from([("type".to_string(), "top".to_string())]);

    // Inner ring → top slabs (the higher part of the pent eave, against the wall).
    for &cell in &inner {
        roof_placer.place_block_forced(
            editor,
            Point3D::new(cell.x, roof_y, cell.y),
            BlockForm::Slab,
            Some(&top_slab),
            None,
        ).await;
    }

    // Outer ring → stairs facing the building (tall back inward, step descending
    // outward). Corners, where no cardinal neighbour points at the building, take
    // a stair aimed along the diagonal so the eave wraps all the way around.
    for &cell in &outer {
        let facing = CARDINALS.into_iter()
            .find(|&d| inward_targets.contains(&(cell + Point2D::from(d))))
            .or_else(|| {
                DIAGONALS.into_iter()
                    .find(|&(dx, dz)| inward_targets.contains(&Point2D::new(cell.x + dx, cell.y + dz)))
                    .map(|(_, dz)| if dz < 0 { Cardinal::North } else { Cardinal::South })
            });
        let Some(dir) = facing else { continue };
        let state = HashMap::from([("facing".to_string(), dir.to_string())]);
        roof_placer.place_block_forced(
            editor,
            Point3D::new(cell.x, roof_y, cell.y),
            BlockForm::Stairs,
            Some(&state),
            None,
        ).await;
    }

    // Fence-post supports under the roof's outer edge: one at every corner of
    // the veranda, plus evenly-spaced posts along the runs between them
    // (symmetric about the building centre). Each post is a wooden fence column
    // from the deck surface up to the eave.
    let outer_set: HashSet<Point2D> = outer.iter().copied().collect();
    let center = plan.building_footprint.bounds().midpoint();
    const POST_SPACING: i32 = 4;
    for &cell in &outer {
        let occupied = |d: Cardinal| outer_set.contains(&(cell + Point2D::from(d)));
        let horiz = occupied(Cardinal::East) || occupied(Cardinal::West);
        let vert = occupied(Cardinal::North) || occupied(Cardinal::South);
        let is_corner = horiz && vert; // the ring turns here
        // On a straight run, post at points symmetric about the centre.
        let on_symmetric = (horiz && !vert && (cell.x - center.x).rem_euclid(POST_SPACING) == 0)
            || (vert && !horiz && (cell.y - center.y).rem_euclid(POST_SPACING) == 0);
        if !(is_corner || on_symmetric) {
            continue;
        }
        for y in (deck_y + 1)..roof_y {
            deck_placer.place_block_forced(
                editor,
                Point3D::new(cell.x, y, cell.y),
                BlockForm::Fence,
                None,
                None,
            ).await;
        }
    }
}
