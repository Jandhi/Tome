//! Stage 1 · **Hull** — the shell built upon the keel, layer by layer.
//! See `docs/plans/ship-builder.md` (Stage 1 → Hull). Technique from the
//! piratemc 30-gun frigate tutorial.
//!
//! Each Y layer (keel → waterline) is a **stretched-teardrop outline** in the X–Z
//! plane: fine point at the **stern** (x=0), round/full **bow** (+x), widest beam
//! ~⅓ from the bow. Only the **perimeter** of the teardrop is placed (interior left
//! as air → a hollow shell). Beam grows from ~0 at the keel to the max at the
//! waterline (rounded-bilge flare).
//!
//! **Blocks only for now** — slab/stair smoothing of the shell comes later.

use std::collections::{HashMap, HashSet};

use crate::generator::materials::{MaterialPlacer, Placer};
use crate::geometry::Point3D;
use crate::minecraft::{Block, BlockForm};

use super::palette::{ShipPalette, ShipPart};
use super::tuning::{BOW_TAPER, HULL_BEVEL_FACE_OUTBOARD, HULL_BILGE_POW, STERN_TAPER};
use super::{Placement, ShipDir, ShipCtx};

/// Plan-view shape of the hull (the X–Z outline family).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HullShape {
    /// Asymmetric teardrop: fuller round bow, finer stern, widest forward.
    Teardrop,
    /// Symmetric ellipse: both ends rounded, widest amidships.
    Oval,
}

/// Normalized teardrop half-beam in `[0, 1]` for `s` in `[0, 1]` (0 = stern, 1 =
/// bow). Normalized so the peak is 1.
fn teardrop_half(s: f32) -> f32 {
    if s <= 0.0 || s >= 1.0 {
        return 0.0;
    }
    let s_peak = STERN_TAPER / (STERN_TAPER + BOW_TAPER);
    let norm = s_peak.powf(STERN_TAPER) * (1.0 - s_peak).powf(BOW_TAPER);
    (s.powf(STERN_TAPER) * (1.0 - s).powf(BOW_TAPER)) / norm
}

/// Normalized oval (ellipse) half-beam: symmetric, widest amidships, rounded ends.
fn oval_half(s: f32) -> f32 {
    if s <= 0.0 || s >= 1.0 {
        return 0.0;
    }
    let u = 2.0 * s - 1.0; // -1..1
    (1.0 - u * u).max(0.0).sqrt()
}

/// Plan-view half-beam for the chosen [`HullShape`].
fn plan_half(s: f32, shape: HullShape) -> f32 {
    match shape {
        HullShape::Teardrop => teardrop_half(s),
        HullShape::Oval => oval_half(s),
    }
}

/// Pure-geometry hull shell: the perimeter cells (local frame) plus key dims.
#[derive(Debug, Clone)]
pub struct HullModel {
    pub length: i32,
    pub depth: i32,
    pub shape: HullShape,
    /// Max beam (full width) at the waterline.
    pub max_beam: i32,
    /// Half-beam of the hull outline at the waterline per station x (`length`
    /// entries). The base the above-water topsides sit on (and tumble in from).
    pub top_half: Vec<i32>,
    /// Perimeter shell cells placed as full blocks.
    pub cells: Vec<Point3D>,
    /// Flare-bevel shell cells — where the bilge widens going up, an upside-down stair
    /// (facing outboard, `dir`) smooths the outer curve instead of a blocky step. The
    /// cell just inboard of each is kept a solid block so nothing shows from inside.
    pub bevel: Vec<HullBevel>,
    /// Hollow interior cells (inside the shell) — cleared to air so the hull stays
    /// dry on water.
    pub interior: Vec<Point3D>,
}

/// A bilge-flare bevel cell: an upside-down stair facing outboard (`dir`).
#[derive(Debug, Clone, Copy)]
pub struct HullBevel {
    pub local: Point3D,
    pub dir: ShipDir,
}

/// Build the hull shell for a keel of `length` × `depth`. `beam_ratio` is the
/// length:beam ratio — max beam = `round(length / beam_ratio)` (e.g. 2.7 ≈ the
/// tutorial's stout hull; higher = sleeker).
///
/// The shell is the **boundary of the 3D hull volume**: a cell inside the volume is
/// kept if any of its sides/underside is exposed (so the flare ledges are sealed —
/// no underside holes), while the **top is left open** (hollow, deck added later).
/// Max hull beam for a keel length: `round(length / beam_ratio)`, floored at 3. The single
/// source of this rule — the placement fit-solver ([`fleet`](super::fleet)) calls it too, so
/// the reserved footprint can't drift from the hull the builder actually lays.
pub fn max_beam(length: i32, beam_ratio: f32) -> i32 {
    ((length as f32) / beam_ratio).round().max(3.0) as i32
}

pub fn build_hull_model(
    length: i32,
    depth: i32,
    beam_ratio: f32,
    shape: HullShape,
    keel_top: &[i32],
) -> HullModel {
    let max_beam = max_beam(length, beam_ratio);
    let max_hw = max_beam / 2; // half-beam

    // Keel's crest Y at a station (`i32::MIN` = no keel there → no constraint).
    let top_at = |x: i32| -> i32 {
        if x < 0 {
            i32::MIN
        } else {
            keel_top.get(x as usize).copied().unwrap_or(i32::MIN)
        }
    };

    // Half-beam of the solid hull volume at a station/layer (`< 1` = no volume).
    let half_at = |x: i32, y: i32| -> i32 {
        if x < 0 || x >= length || y < 1 || y > depth {
            return 0;
        }
        let s = if length > 1 { x as f32 / (length - 1) as f32 } else { 0.0 };
        let vf = ((y as f32) / (depth as f32)).powf(HULL_BILGE_POW);
        ((max_hw as f32) * plan_half(s, shape) * vf).round() as i32
    };
    // The hull volume stays strictly above the keel's crest so the keel protrudes
    // below and touches water (respects the bow rocker and stern step-up).
    let in_vol = |x: i32, y: i32, z: i32| -> bool {
        let kt = top_at(x);
        if kt != i32::MIN && y <= kt {
            return false;
        }
        let h = half_at(x, y);
        h >= 1 && z.abs() <= h && y >= 1 && y <= depth
    };

    // Boundary = exposed on a side/cap (x,z) or the underside (y-1). The +y face is
    // ignored so the top stays open (hollow deck).
    let exposed = |x: i32, y: i32, z: i32| -> bool {
        !in_vol(x - 1, y, z)
            || !in_vol(x + 1, y, z)
            || !in_vol(x, y, z - 1)
            || !in_vol(x, y, z + 1)
            || !in_vol(x, y - 1, z)
    };
    // A bilge-flare cell: an outer side cell below the waterline whose row is wider than
    // the one beneath it (the bilge widening going up). The waterline row (y == depth) is
    // left a full block — the gunwale the deck sits on.
    let flares = |x: i32, y: i32, z: i32, h: i32| -> bool {
        y < depth && z != 0 && z.abs() == h && h > half_at(x, y - 1)
    };

    // First pass: collect the flare-bevel cells and, for each, the cell just **inboard**
    // of it that must stay a solid full block. That backing keeps the interior a clean
    // wall (no stair backs showing inside) and seals the flare ledge so you can't see
    // through it (the bug the bare stairs left).
    let mut bevel_set: HashSet<(i32, i32, i32)> = HashSet::new();
    let mut backing: HashSet<(i32, i32, i32)> = HashSet::new();
    for y in 1..=depth {
        for x in 0..length {
            let h = half_at(x, y);
            if h < 1 {
                continue;
            }
            for z in -h..=h {
                if in_vol(x, y, z) && exposed(x, y, z) && flares(x, y, z, h) {
                    bevel_set.insert((x, y, z));
                    backing.insert((x, y, z - z.signum())); // one cell toward the centre
                }
            }
        }
    }

    let mut cells = Vec::new();
    let mut bevel = Vec::new();
    let mut interior = Vec::new();
    for y in 1..=depth {
        for x in 0..length {
            let h = half_at(x, y);
            if h < 1 {
                continue;
            }
            for z in -h..=h {
                if !in_vol(x, y, z) {
                    continue; // below the keel rocker → leave for the keel/water
                }
                if bevel_set.contains(&(x, y, z)) {
                    // Upside-down stair facing outboard (curve on the underside/outside
                    // of the bilge); the forced-solid backing inboard means nothing of
                    // the stair shows from inside. Facing is a screenshot flip candidate.
                    let outboard = if z > 0 { ShipDir::Starboard } else { ShipDir::Port };
                    let dir = if HULL_BEVEL_FACE_OUTBOARD { outboard } else { outboard.opposite() };
                    bevel.push(HullBevel { local: Point3D::new(x, y, z), dir });
                } else if exposed(x, y, z) || backing.contains(&(x, y, z)) {
                    cells.push(Point3D::new(x, y, z));
                } else {
                    interior.push(Point3D::new(x, y, z)); // hollow → cleared to air
                }
            }
        }
    }

    // The waterline outline (half-beam per station) for the above-water topsides.
    let top_half: Vec<i32> = (0..length).map(|x| half_at(x, depth).max(0)).collect();

    HullModel { length, depth, shape, max_beam, top_half, cells, bevel, interior }
}

/// Place the hull shell cells as full blocks (from the `Hull` palette role) and,
/// when on water, clear the hollow interior to air so the hull stays dry.
pub async fn place_hull(
    ctx: &mut ShipCtx<'_>,
    model: &HullModel,
    placement: &Placement,
    ship_palette: &ShipPalette,
    on_water: bool,
) {
    let role = ship_palette.role(ShipPart::Hull);
    let material = ctx
        .palette
        .get_material(role)
        .unwrap_or_else(|| panic!("ship palette role {role:?} missing from base palette"))
        .clone();

    let mut placer_rng = ctx.rng.derive();
    let mut placer = MaterialPlacer::new(Placer::new(&ctx.data.materials, &mut placer_rng), material);

    for &cell in &model.cells {
        placer
            .place_block(ctx.editor, placement.to_world(cell), BlockForm::Block, None, None)
            .await;
    }

    // Bilge-flare bevels: upside-down stairs facing outboard, waterlogged on water.
    for b in &model.bevel {
        let mut state = HashMap::from([
            ("facing".to_string(), placement.world_cardinal(b.dir).to_string()),
            ("half".to_string(), "top".to_string()),
        ]);
        if on_water {
            state.insert("waterlogged".to_string(), "true".to_string());
        }
        placer
            .place_block(ctx.editor, placement.to_world(b.local), BlockForm::Stairs, Some(&state), None)
            .await;
    }

    // Clear the hollow interior of any water (on land it's already air).
    if on_water {
        let air = Block::from("minecraft:air");
        for &cell in &model.interior {
            ctx.editor.place_block(&air, placement.to_world(cell)).await;
        }
    }
}
