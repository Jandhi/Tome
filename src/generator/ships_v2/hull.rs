//! Stage 1 · **Hull** — the shell built upon the keel, layer by layer.
//! See `docs/plans/ship-builder-v2.md` (Stage 1 → Hull). Technique from the
//! piratemc 30-gun frigate tutorial.
//!
//! Each Y layer (keel → waterline) is a **stretched-teardrop outline** in the X–Z
//! plane: fine point at the **stern** (x=0), round/full **bow** (+x), widest beam
//! ~⅓ from the bow. Only the **perimeter** of the teardrop is placed (interior left
//! as air → a hollow shell). Beam grows from ~0 at the keel to the max at the
//! waterline (rounded-bilge flare).
//!
//! **Blocks only for now** — slab/stair smoothing of the shell comes later.

use crate::generator::materials::{MaterialPlacer, Placer};
use crate::geometry::Point3D;
use crate::minecraft::{Block, BlockForm};

use super::palette::{ShipPalette, ShipPart};
use super::{Placement, ShipV2Ctx};

/// Vertical flare exponent: beam = max · (y/depth)^p. `< 1` → a rounded bilge that
/// widens quickly off the keel then eases toward the waterline.
const HULL_BILGE_POW: f32 = 0.7;

/// Teardrop taper exponents for the plan shape `s^STERN · (1−s)^BOW`.
/// Lower = blunter/wider that end; higher = finer/narrower. Stern (s→0) is kept a
/// touch fuller, the bow (s→1) the finer/fuller-bulb end — but both < 1 so neither
/// tip is sharply pointed. The peak (widest beam) sits at `STERN/(STERN+BOW)`.
const STERN_TAPER: f32 = 0.85;
const BOW_TAPER: f32 = 0.65;

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
    /// Perimeter shell cells (all full blocks for now).
    pub cells: Vec<Point3D>,
    /// Hollow interior cells (inside the shell) — cleared to air so the hull stays
    /// dry on water.
    pub interior: Vec<Point3D>,
}

/// Build the hull shell for a keel of `length` × `depth`. `beam_ratio` is the
/// length:beam ratio — max beam = `round(length / beam_ratio)` (e.g. 2.7 ≈ the
/// tutorial's stout hull; higher = sleeker).
///
/// The shell is the **boundary of the 3D hull volume**: a cell inside the volume is
/// kept if any of its sides/underside is exposed (so the flare ledges are sealed —
/// no underside holes), while the **top is left open** (hollow, deck added later).
pub fn build_hull_model(
    length: i32,
    depth: i32,
    beam_ratio: f32,
    shape: HullShape,
    keel_top: &[i32],
) -> HullModel {
    let max_beam = ((length as f32) / beam_ratio).round().max(3.0) as i32;
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

    let mut cells = Vec::new();
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
                // Boundary = exposed on a side/cap (x,z) or the underside (y-1).
                // The +y face is ignored so the top stays open (hollow deck).
                let exposed = !in_vol(x - 1, y, z)
                    || !in_vol(x + 1, y, z)
                    || !in_vol(x, y, z - 1)
                    || !in_vol(x, y, z + 1)
                    || !in_vol(x, y - 1, z);
                if exposed {
                    cells.push(Point3D::new(x, y, z));
                } else {
                    interior.push(Point3D::new(x, y, z)); // hollow → cleared to air
                }
            }
        }
    }

    // The waterline outline (half-beam per station) for the above-water topsides.
    let top_half: Vec<i32> = (0..length).map(|x| half_at(x, depth).max(0)).collect();

    HullModel { length, depth, shape, max_beam, top_half, cells, interior }
}

/// Place the hull shell cells as full blocks (from the `Hull` palette role) and,
/// when on water, clear the hollow interior to air so the hull stays dry.
pub async fn place_hull(
    ctx: &mut ShipV2Ctx<'_>,
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

    // Clear the hollow interior of any water (on land it's already air).
    if on_water {
        let air = Block::from("minecraft:air");
        for &cell in &model.interior {
            ctx.editor.place_block(&air, placement.to_world(cell)).await;
        }
    }
}
