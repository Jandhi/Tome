//! Stage 1 · **Keel** — the backbone spine, and the parameter that sets the ship's
//! length. See `docs/plans/ship-builder-v2.md` (Stage 1 → Keel).
//!
//! Local frame: `x` = stern(0) → bow(+x), `z` = beam (centerline 0), `y` = up from
//! the flat keel bottom (0). The flat run sits at `y = 0`; the ends rise to ~the
//! waterline (`y = depth`). All math is local; [`Placement`] transforms to world.
//!
//! Profile, stern→bow (bottom edge is a top-slab line):
//! - **Sternpost** (stern tip, `x = 0`): a top slab at the bottom + a *straight
//!   vertical* column rising to the waterline; the rudder attaches here.
//! - **Stern steps** (a couple, size-scaled): small *solid* block steps at the post
//!   base, each filled from the bottom slab up — a solid mass exposed to water like
//!   the rest of the keel, easing the post into the flat run for hull shaping.
//! - **Flat run** (the majority): a line of top slabs along the bottom.
//! - **Bow curve** (front ~15–20%): a real **parabolic stem sweep** (`y = depth·t²`)
//!   — tangent to the flat run, steepening to the bow tip. We sample the curve per
//!   block-column and approximate it: **top slab** where it's gentle, **upside-down
//!   stair** where the slope ≈ 1, **full block** where steep (near the stem). Small
//!   size classes naturally degrade to a plain stair staircase (fine as an approx).
//!
//! Rake/step stairs are **upside-down (top-half)** so the keel's *inside* (top)
//! reads solid and the curve sits on the underside — never right-way-up stairs.
//!
//! Stage-1 rule: every stair/slab placed here is **waterlogged** (on water only).

use std::collections::HashMap;

use crate::generator::materials::{MaterialPlacer, Placer};
use crate::geometry::Point3D;
use crate::minecraft::BlockForm;

use super::palette::{ShipPalette, ShipPart};
use super::{Placement, ShipDir, ShipV2Ctx};

/// Which ship-local direction the rake stairs face (bow rake → bow, stern rake →
/// stern). With upside-down stairs the curve falls on the underside. This is the
/// classic MC stair flip point — if a screenshot shows a rake's curve on the wrong
/// face, swap the facing (and/or the `top_half`) for that rake.
// Upside-down bow stairs: the full-height side faces *down-slope* (toward the
// stern), so the solid top continues the keel line and the notch rounds the
// underside. The bow climbs toward +x, so its stairs face Stern. (Classic MC
// stair flip point — swap if a screenshot shows the notch on the wrong face.)
const BOW_RAKE_STAIR_FACE: ShipDir = ShipDir::Stern;

/// Exponent of the parabolic bow-stem curve `y = depth · t^p`. `2.0` = a classic
/// parabola (tangent to the flat run, steepening to the stem). Tune for a gentler
/// (lower) or sharper (higher) sweep.
const BOW_CURVE_POW: f32 = 2.0;

/// One placed keel block in the local frame.
#[derive(Debug, Clone)]
pub struct KeelCell {
    pub local: Point3D,
    pub form: BlockForm,
    pub part: ShipPart,
    /// For stairs: the ship-local direction the stair faces.
    pub facing: Option<ShipDir>,
    /// `true` = top half (top slab, or upside-down/top-half stair); `false` = bottom.
    pub top_half: bool,
}

/// Pure-geometry keel: the cells to place plus the dimensions downstream needs.
#[derive(Debug, Clone)]
pub struct KeelModel {
    /// Tip-to-tip length.
    pub length: i32,
    /// How far the flat bottom sits below the waterline (also the sternpost height).
    pub depth: i32,
    /// Local Y of the waterline (`== depth`).
    pub waterline_y: i32,
    /// Stations given to the bow curve.
    pub bow_rake_len: i32,
    /// Number of small steps at the stern post base.
    pub stern_steps: i32,
    /// Horizontal run (blocks) of each stern step — elongated for longer hulls.
    pub stern_step_run: i32,
    pub cells: Vec<KeelCell>,
}

impl KeelModel {
    /// The keel's crest (top edge): the **highest** keel block Y at each station x,
    /// as `length` entries. `i32::MIN` where the keel has no cell at that x. The
    /// hull stays strictly above this so it sits on the keel everywhere — the bow
    /// rocker *and* the solid stern step-up — keeping the keel the outermost,
    /// water-touching part.
    pub fn top_profile(&self) -> Vec<i32> {
        let mut top = vec![i32::MIN; self.length.max(0) as usize];
        for c in &self.cells {
            let x = c.local.x;
            if x >= 0 && (x as usize) < top.len() {
                let t = &mut top[x as usize];
                *t = if *t == i32::MIN { c.local.y } else { (*t).max(c.local.y) };
            }
        }
        top
    }
}

/// Keel depth (underwater height) as a function of length: ~`length / 5.5`, min 1.
/// A ~30-long keel → ~5–6 deep; the smallest ships → 1–2.
pub fn keel_depth(length: i32) -> i32 {
    ((length as f32) / 5.5).round().max(1.0) as i32
}

/// Normalized bow-stem curve: height in `[0, 1]` for `t` in `[0, 1]` (0 = flat run,
/// 1 = stem tip). A parabola (`t^p`, p≈2): tangent to the flat run, steepening to
/// the bow. Sampled per block-column and approximated with slabs/stairs/blocks.
fn bow_curve(t: f32) -> f32 {
    t.powf(BOW_CURVE_POW)
}

/// Build the keel geometry for a given tip-to-tip `length`.
pub fn build_keel_model(length: i32) -> KeelModel {
    let depth = keel_depth(length);

    let bow_rake_len = ((length as f32 * 0.18).round() as i32).max(3);
    let bow_rake_start = length - bow_rake_len; // first bow-curve station

    // A couple of small steps at the stern base (size-scaled), then the straight post.
    // Steps are elongated for longer hulls: each runs `stern_step_run` blocks.
    let stern_steps = (depth / 2).clamp(1, 3);
    let stern_step_run = ((length as f32) / 18.0).round().max(1.0) as i32;
    let flat_start = 1 + stern_steps * stern_step_run; // first flat station (after the stern wedge)
    let flat_end = bow_rake_start - 1; // last flat station

    let mut cells = Vec::new();

    let slab = |x: i32, y: i32| KeelCell {
        local: Point3D::new(x, y, 0),
        form: BlockForm::Slab,
        part: ShipPart::Keel,
        facing: None,
        top_half: true, // top slab → recessed, tapered underside
    };
    let block = |x: i32, y: i32| KeelCell {
        local: Point3D::new(x, y, 0),
        form: BlockForm::Block,
        part: ShipPart::Keel,
        facing: None,
        top_half: false,
    };
    // Upside-down (top-half) stair: solid top (the keel's inside), curve underneath.
    let rake_stair = |x: i32, y: i32, face: ShipDir| KeelCell {
        local: Point3D::new(x, y, 0),
        form: BlockForm::Stairs,
        part: ShipPart::Keel,
        facing: Some(face),
        top_half: true,
    };

    // --- Bottom edge: top slabs under the stern tip + the flat run. ---
    cells.push(slab(0, 0));
    for x in flat_start..=flat_end {
        cells.push(slab(x, 0));
    }

    // --- Sternpost: a straight vertical column above the stern-tip slab. ---
    for y in 1..=depth {
        cells.push(block(0, y));
    }

    // --- Stern steps: a couple of *solid* block steps at the post base, each
    // elongated to `stern_step_run` blocks for longer hulls. Filled from the bottom
    // slab up so the stern is a solid mass exposed to water like the rest of the
    // keel, giving the hull something to shape against. Highest step is next to the
    // post; they descend toward the flat run. ---
    let mut sx = 1;
    for level in 0..stern_steps {
        let h = stern_steps - level; // nearest the post (high) → flat run (low)
        for _ in 0..stern_step_run {
            cells.push(slab(sx, 0)); // bottom course: top slab
            for y in 1..=h {
                cells.push(block(sx, y)); // fill the step solid
            }
            sx += 1;
        }
    }

    // --- Bow curve: sample the parabola and approximate per column. ---
    let mut prev = 0;
    for i in 0..bow_rake_len {
        let x = bow_rake_start + i;
        let t = (i as f32 + 1.0) / bow_rake_len as f32; // (0, 1]
        let h = (((depth as f32) * bow_curve(t)).round() as i32).clamp(prev, depth);
        match h - prev {
            // gentle / flat: a top slab continues the keel line.
            0 => cells.push(slab(x, h)),
            // ~1 slope: a single upside-down stair (solid top, curved underside).
            1 => cells.push(rake_stair(x, h, BOW_RAKE_STAIR_FACE)),
            // steep (near the stem): an upside-down stair at the *bottom* of the
            // rise (the smoothing transition), full blocks above it (the solid
            // stem) — block on top, stair below.
            _ => {
                cells.push(rake_stair(x, prev + 1, BOW_RAKE_STAIR_FACE));
                for y in (prev + 2)..=h {
                    cells.push(block(x, y));
                }
            }
        }
        prev = h;
    }

    KeelModel { length, depth, waterline_y: depth, bow_rake_len, stern_steps, stern_step_run, cells }
}

/// Place the keel cells into the world via the editor, drawing each part's block
/// from its [`ShipPalette`] role. Stairs/slabs are waterlogged only when the ship
/// is built **on water** — never on land (Stage-1 rule).
pub async fn place_keel(
    ctx: &mut ShipV2Ctx<'_>,
    model: &KeelModel,
    placement: &Placement,
    ship_palette: &ShipPalette,
    on_water: bool,
) {
    for cell in &model.cells {
        let role = ship_palette.role(cell.part);
        let material = ctx
            .palette
            .get_material(role)
            .unwrap_or_else(|| panic!("ship palette role {role:?} missing from base palette"))
            .clone();

        let mut placer_rng = ctx.rng.derive();
        let mut placer =
            MaterialPlacer::new(Placer::new(&ctx.data.materials, &mut placer_rng), material);

        let world = placement.to_world(cell.local);
        let state = cell_state(cell, placement, on_water);
        placer.place_block(ctx.editor, world, cell.form, state.as_ref(), None).await;
    }
}

/// Blockstate for a keel cell. Stairs need facing/half; slabs need type. Both are
/// waterlogged only when `on_water` (never on land). Full blocks need no state.
fn cell_state(cell: &KeelCell, placement: &Placement, on_water: bool) -> Option<HashMap<String, String>> {
    let mut state = match cell.form {
        BlockForm::Stairs => {
            let facing = placement.world_cardinal(cell.facing.unwrap_or(ShipDir::Bow));
            HashMap::from([
                ("facing".to_string(), facing.to_string()),
                ("half".to_string(), if cell.top_half { "top" } else { "bottom" }.to_string()),
            ])
        }
        BlockForm::Slab => HashMap::from([(
            "type".to_string(),
            if cell.top_half { "top" } else { "bottom" }.to_string(),
        )]),
        _ => return None,
    };

    if on_water {
        state.insert("waterlogged".to_string(), "true".to_string());
    }
    Some(state)
}
