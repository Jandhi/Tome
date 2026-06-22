//! Hull geometry. Pure model first (`build_model` → [`HullModel`]), then
//! placement (`plank`, `deck`). No block writes happen here.
//!
//! The model is built by filling a conceptual *solid* hull volume from the ribs,
//! then taking its **boundary** as the shell. That makes the hull watertight by
//! construction — every interior (hold) cell is fully enclosed regardless of how
//! the beam tapers fore-and-aft — and hands the later interior system a clean,
//! sealed `hold_volume` to subdivide.

pub mod rib;
pub mod plank;
pub mod deck;

use std::collections::HashSet;

use crate::geometry::{Point2D, Point3D};
use crate::minecraft::BlockForm;

use super::dimensions::ShipDimensions;
use super::{HullShape, ShipDir};
use rib::{Rib, build_ribs};

/// One placed hull-shell block. Most are full `Block`s, but outer convex corners
/// (the turn of the bilge underneath, and any sheer/flare edge) become `Stairs`
/// so the hull curves instead of stepping. The inboard faces stay sealed, so
/// using a stair on an outward-facing corner doesn't break watertightness.
#[derive(Debug, Clone, Copy)]
pub struct HullPlank {
    pub local: Point3D,
    pub form: BlockForm,
    /// For `Stairs`: the outward direction the bevel faces, and whether it's the
    /// top half (an upside-down stair under an overhang/bilge). The stair's solid
    /// side faces inboard; `plank` resolves the world facing from the heading.
    pub cut: Option<(ShipDir, bool)>,
}

/// Everything downstream needs about the hull, without re-deriving geometry.
pub struct HullModel {
    pub dims: ShipDimensions,
    /// Local y of the deck surface.
    pub deck_y: i32,
    /// Local y of the waterline (for diagnostics).
    pub waterline_y: i32,
    pub ribs: Vec<Rib>,
    /// Shell cells below the deck (hull planking) with their block form, sorted.
    pub hull_cells: Vec<HullPlank>,
    /// Deck-plane cells at `deck_y`, local `(x, z)`, sorted.
    pub deck_cells: Vec<Point2D>,
    /// Bulwark/rail rim one above the deck edge, local `(x, z)`, sorted.
    pub gunwale: Vec<Point2D>,
    /// Enclosed below-deck interior cells (left as air in Phase 1), sorted. The
    /// input the future interior system subdivides into compartments.
    pub hold_volume: Vec<Point3D>,
    /// Internal rib/stanchion posts (logs) hugging the hull inside, placed every
    /// few stations. Local coords, below the deck. The reference ships' signature
    /// detail; see the guides' "rib supports every 3–5 blocks".
    pub frame_posts: Vec<Point3D>,
    /// Keel slabs: a centerline backbone just under the hull bottom, local coords.
    pub keel_slabs: Vec<Point3D>,
    /// Deck hatch cell, if any. `None` in Phase 1 (sealed, empty hold).
    pub hatch: Option<Point2D>,
}

impl HullModel {
    /// Lowest hold (interior) cell directly under deck column `(x, z)`, if any.
    pub fn hold_floor(&self, x: i32, z: i32) -> Option<i32> {
        self.hold_volume
            .iter()
            .filter(|c| c.x == x && c.z == z)
            .map(|c| c.y)
            .min()
    }

    /// A deck column with the deepest hold beneath it, preferring stations near
    /// `toward_x`. Used to site a hatch where the ladder has the most depth.
    pub fn deepest_hold_column(&self, toward_x: i32) -> Option<Point2D> {
        let deck: std::collections::HashSet<Point2D> =
            self.deck_cells.iter().copied().collect();
        deck.iter()
            .filter_map(|&p| self.hold_floor(p.x, p.y).map(|floor| (p, floor)))
            .max_by_key(|(p, floor)| {
                // Deepest hold first, then closest to `toward_x`, then centerline.
                (self.deck_y - floor, -(p.x - toward_x).abs(), -p.y.abs())
            })
            .map(|(p, _)| p)
    }
}

/// Build the hull model for a shape + dimensions. Dispatches the rib profile by
/// [`HullShape`]; the solid→shell assembly is shape-independent.
pub fn build_model(shape: HullShape, dims: ShipDimensions) -> HullModel {
    let deck_y = dims.depth;
    let waterline_y = (deck_y - dims.freeboard).max(0);
    let ribs = build_ribs(shape, &dims);

    // 1. Fill the conceptual solid hull volume from the ribs. Each rib gives a
    //    half-width per height level, so cross-sections curve (rounded bilge).
    let mut solid: HashSet<Point3D> = HashSet::new();
    for r in &ribs {
        for (i, &hw) in r.half_widths.iter().enumerate() {
            if hw < 0 {
                continue;
            }
            let y = r.bottom_y + i as i32;
            for z in -hw..=hw {
                solid.insert(Point3D::new(r.x, y, z));
            }
        }
    }

    // 2. The shell is the boundary: any solid cell with a non-solid face-neighbor.
    //    Interior (fully enclosed) cells become the hollow hold. Shell cells are
    //    classified into full blocks vs. stair bevels for a curved hull.
    let mut hull_cells: Vec<HullPlank> = Vec::new();
    let mut deck_cells: Vec<Point2D> = Vec::new();
    let mut hold_volume: Vec<Point3D> = Vec::new();

    for &c in &solid {
        let is_shell = Point3D::NEIGHBOURS_1_AWAY
            .iter()
            .map(|n| Point3D::new(c.x + n.x, c.y + n.y, c.z + n.z))
            .chain([Point3D::new(c.x, c.y + 1, c.z), Point3D::new(c.x, c.y - 1, c.z)])
            .any(|n| !solid.contains(&n));

        if c.y == deck_y {
            // Top face is the deck — planked separately so it can use deck material.
            deck_cells.push(Point2D::new(c.x, c.z));
        } else if is_shell {
            hull_cells.push(classify_shell(c, &solid));
        } else {
            hold_volume.push(c);
        }
    }

    // 3. Gunwale = deck-edge rim (a deck cell missing an in-plane deck neighbor).
    let deck_set: HashSet<Point2D> = deck_cells.iter().copied().collect();
    let mut gunwale: Vec<Point2D> = deck_cells
        .iter()
        .copied()
        .filter(|&p| {
            [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .any(|(dx, dz)| !deck_set.contains(&Point2D::new(p.x + dx, p.y + dz)))
        })
        .collect();

    // 4. Internal rib posts (logs hugging the hull inside) every few stations,
    //    and a slab keel running the flat of the bottom.
    let hold_set: HashSet<Point3D> = hold_volume.iter().copied().collect();
    let spacing = (dims.length / 6).clamp(3, 5);
    let mut frame_posts: Vec<Point3D> = Vec::new();
    let mut keel_slabs: Vec<Point3D> = Vec::new();
    for r in &ribs {
        // Keel slab under the flat central run of the bottom.
        if r.bottom_y == 0 {
            keel_slabs.push(Point3D::new(r.x, -1, 0));
        }
        // Rib posts: skip the ends, place on multiples of the spacing.
        if r.x == 0 || r.x == dims.length - 1 || r.x % spacing != 0 {
            continue;
        }
        for (i, &hw) in r.half_widths.iter().enumerate() {
            let y = r.bottom_y + i as i32;
            if y >= deck_y || hw < 2 {
                continue;
            }
            for z in [-(hw - 1), hw - 1] {
                let p = Point3D::new(r.x, y, z);
                // Only inside the hold (not the shell, not the ladder column).
                if hold_set.contains(&p) {
                    frame_posts.push(p);
                }
            }
        }
    }

    hull_cells.sort_by_key(|p| (p.local.x, p.local.y, p.local.z));
    deck_cells.sort_by_key(|p| (p.x, p.y));
    gunwale.sort_by_key(|p| (p.x, p.y));
    hold_volume.sort_by_key(|p| (p.x, p.y, p.z));
    frame_posts.sort_by_key(|p| (p.x, p.y, p.z));
    keel_slabs.sort_by_key(|p| (p.x, p.z));

    HullModel {
        dims,
        deck_y,
        waterline_y,
        ribs,
        hull_cells,
        deck_cells,
        gunwale,
        hold_volume,
        frame_posts,
        keel_slabs,
        hatch: None,
    }
}

/// Classify a shell cell into a full block or a stair bevel. A cell exposed on
/// exactly one horizontal side plus its underside becomes an upside-down stair
/// (rounds the bilge / flare overhang); exposed on one side plus its top becomes
/// a normal stair (rounds a sheer edge). Everything else stays a full block, so
/// the near-vertical topsides remain solid and corners stay watertight.
fn classify_shell(c: Point3D, solid: &HashSet<Point3D>) -> HullPlank {
    let occ = |dx, dy, dz| solid.contains(&Point3D::new(c.x + dx, c.y + dy, c.z + dz));
    let up = occ(0, 1, 0);
    let down = occ(0, -1, 0);

    let mut sides: Vec<ShipDir> = Vec::new();
    if !occ(1, 0, 0) { sides.push(ShipDir::Bow); }
    if !occ(-1, 0, 0) { sides.push(ShipDir::Stern); }
    if !occ(0, 0, 1) { sides.push(ShipDir::Starboard); }
    if !occ(0, 0, -1) { sides.push(ShipDir::Port); }

    let (form, cut) = if sides.len() == 1 {
        let out = sides[0];
        if up && !down {
            (BlockForm::Stairs, Some((out, true))) // bilge / overhang underside
        } else if !up && down {
            (BlockForm::Stairs, Some((out, false))) // sheer edge
        } else {
            (BlockForm::Block, None)
        }
    } else {
        (BlockForm::Block, None)
    };

    HullPlank { local: c, form, cut }
}

/// Structural checks, run inside `build_ship`. Returns an `Err` describing the
/// first violation. The ship analogue of `check_building_invariants`.
pub fn check_ship_invariants(model: &HullModel) -> Result<(), String> {
    let hull: HashSet<Point3D> = model.hull_cells.iter().map(|p| p.local).collect();
    let deck: HashSet<Point2D> = model.deck_cells.iter().copied().collect();

    // (a) Port/starboard symmetry: negating z maps the hull onto itself.
    for c in &hull {
        let mirror = Point3D::new(c.x, c.y, -c.z);
        if !hull.contains(&mirror) {
            return Err(format!("hull not symmetric about centerline at {c:?}"));
        }
    }

    // (b) Every hold cell is capped by deck above it — the hold is sealed.
    for h in &model.hold_volume {
        if !deck.contains(&Point2D::new(h.x, h.z)) {
            return Err(format!("hold cell {h:?} is not covered by deck"));
        }
    }

    // (c) Deck is gap-free across the beam: each station's deck z-range is
    //     contiguous (no holes a player could fall through).
    let mut by_station: std::collections::HashMap<i32, Vec<i32>> = std::collections::HashMap::new();
    for p in &model.deck_cells {
        by_station.entry(p.x).or_default().push(p.y);
    }
    for (x, mut zs) in by_station {
        zs.sort_unstable();
        if zs.windows(2).any(|w| w[1] - w[0] != 1) {
            return Err(format!("deck has a gap at station x={x}"));
        }
    }

    // (d) Gunwale forms a non-empty, symmetric rim.
    if model.gunwale.is_empty() {
        return Err("gunwale rim is empty".to_string());
    }
    let gset: HashSet<Point2D> = model.gunwale.iter().copied().collect();
    for g in &model.gunwale {
        if !gset.contains(&Point2D::new(g.x, -g.y)) {
            return Err(format!("gunwale not symmetric at {g:?}"));
        }
    }

    // (e) If a hatch is cut, it sits on a deck cell over a hold column.
    if let Some(h) = model.hatch {
        if !deck.contains(&h) {
            return Err(format!("hatch {h:?} is not on a deck cell"));
        }
        if model.hold_floor(h.x, h.y).is_none() {
            return Err(format!("hatch {h:?} has no hold beneath it"));
        }
    }

    // (f) Bounding box stays within declared dimensions.
    let half = model.dims.beam / 2;
    let cells = model.hull_cells.iter().map(|p| p.local).chain(model.hold_volume.iter().copied());
    for c in cells {
        if c.x < 0 || c.x >= model.dims.length || c.z.abs() > half || c.y < 0 || c.y > model.deck_y + 1 {
            return Err(format!("cell {c:?} outside declared dimensions"));
        }
    }

    Ok(())
}
