//! Deck addition · **Bowsprit** — a beakhead prow + the forward-poking spar.
//!
//! Rather than two thin lines floating off the deck, the bow is **extended forward
//! into a tapered prow** (a "beakhead") that carries the bowsprit: top slabs continue
//! the bow outline to a point at the weather-deck level. The prow's deck edge is fed
//! into [`DeckState::top_outline`] so the **shared main railing** wraps it (the bowsprit
//! no longer builds its own rail; it runs *before* the railing in `BUILD_ORDER`). The
//! centerline (z=0) **spar** projects on from the prow tip.
//!
//! **Spar — easy-to-build blocks + slabs (no stairs).** The forward spar is laid the
//! way you'd build it by hand: a flat run of **full blocks**, then a one-block climb
//! made of a **bottom slab then a top slab** (two slabs at successive columns — the
//! bottom slab fills the lower half, the next column's top slab the upper half, so the
//! surface rises half-then-half into a smooth diagonal), repeating one level up. The
//! flat-run length sets the rake (see [`BowspritRake`]). (The `Spar` role is a plank
//! wood so the slab variants exist.)
//!
//! Rake adapts to the anchor height: with a raised deck the anchor is already high, so
//! the spar can run **straight** (a flat run of blocks); without one it rakes **upward**
//! from the low stem. Length ≈ a little less than half the hull (the visible forward
//! reach is ~0.4·length). Figurehead / rigging / decoration come in a later pass.

use std::collections::HashMap;

use crate::generator::materials::{MaterialPlacer, Placer};
use crate::geometry::Point3D;
use crate::minecraft::BlockForm;
use crate::noise::RNG;

use super::super::palette::ShipPart;
use super::super::tuning::{
    BOWSPRIT_GENTLE_FLAT_RUN, BOWSPRIT_GENTLE_REACH, BOWSPRIT_STEEP_FLAT_RUN, BOWSPRIT_STEEP_REACH,
    BOWSPRIT_STRAIGHT_REACH, BOWSPRIT_TIERED_CLIMB_RUN, BOWSPRIT_TIERED_FLAT_RUN,
    BOWSPRIT_TIERED_REACH, PROW_FLARE_POW, RAKE_STAIR_FACE, REACH_FRACTION,
};
use super::super::{Placement, ShipDir, ShipCtx};
use super::{DeckContext, DeckState, SizeTier};

/// Upward rake of the spar, built from **full blocks + slabs only** (no stairs). A rake
/// alternates a run of `flat_run` flat blocks with a group of `climb_run` one-block slab
/// climbs: [`Straight`] never climbs; [`Gentle`]/[`Steep`] climb one level per group;
/// [`Tiered`] climbs two (a "2 blocks then 2 double-slabs" terrace).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BowspritRake {
    /// Horizontal — a flat run of blocks. Used when a raised deck gives a high anchor.
    Straight,
    /// Gentle climb — 2 flat blocks, then one single-column slab step (1 per 3 cols).
    Gentle,
    /// Steep climb — 1 flat block, then one single-column slab step (1 per 2 cols).
    Steep,
    /// Tiered climb — 2 flat blocks, then **two** stacked slab steps, repeating: a
    /// "2 blocks, 2 double-slabs" terrace (climbs 2 levels per 4 columns).
    Tiered,
}

impl BowspritRake {
    /// Full-block columns laid at the current level before each climb group. Larger =
    /// gentler; [`Straight`] never climbs.
    fn flat_run(self) -> i32 {
        match self {
            BowspritRake::Straight => i32::MAX, // never climbs
            BowspritRake::Gentle => BOWSPRIT_GENTLE_FLAT_RUN,
            BowspritRake::Steep => BOWSPRIT_STEEP_FLAT_RUN,
            BowspritRake::Tiered => BOWSPRIT_TIERED_FLAT_RUN,
        }
    }

    /// Consecutive one-block slab climbs per group (between flat runs). `1` for the
    /// simple rakes; [`Tiered`] widens the tread with stacked equal-height double-slabs.
    fn climb_run(self) -> i32 {
        match self {
            BowspritRake::Tiered => BOWSPRIT_TIERED_CLIMB_RUN,
            _ => 1,
        }
    }

    /// Forward-reach multiplier — steeper rakes are **shortened** so they don't climb
    /// far up into the sky (they read as a bowsprit, not a staircase ramp).
    fn reach_factor(self) -> f32 {
        match self {
            BowspritRake::Straight => BOWSPRIT_STRAIGHT_REACH,
            BowspritRake::Gentle => BOWSPRIT_GENTLE_REACH,
            BowspritRake::Steep => BOWSPRIT_STEEP_REACH,
            BowspritRake::Tiered => BOWSPRIT_TIERED_REACH,
        }
    }

    /// The angled rakes (everything but [`Straight`]).
    const ANGLED: [BowspritRake; 3] =
        [BowspritRake::Gentle, BowspritRake::Steep, BowspritRake::Tiered];

    /// Pick a rake. A raised deck gives a high anchor so **all** are allowed (including
    /// [`Straight`]); without one, [`Straight`] is excluded (it needs the high anchor)
    /// and we pick among the angled rakes. Randomised for now — later a higher-level
    /// system may choose which parts/rake go together per ship.
    pub fn pick(has_deck: bool, rng: &mut RNG) -> BowspritRake {
        if has_deck {
            match rng.rand_i32_range(0, 4) {
                0 => BowspritRake::Straight,
                1 => BowspritRake::Gentle,
                2 => BowspritRake::Steep,
                _ => BowspritRake::Tiered,
            }
        } else {
            BowspritRake::ANGLED[rng.rand_i32_range(0, BowspritRake::ANGLED.len() as i32) as usize]
        }
    }
}

/// One placed bowsprit block in the local frame (a slab or a stair), kept **1 block
/// thick** (z = 0). `top_half` selects a top slab / upside-down stair vs a bottom slab
/// / right-side-up stair, so the beam can follow the line at half-block resolution.
#[derive(Debug, Clone)]
pub struct BowspritCell {
    pub local: Point3D,
    pub form: BlockForm,
    /// For stairs: the ship-local direction the stair faces.
    pub facing: Option<ShipDir>,
    /// `true` = upper half (top slab / upside-down stair); `false` = lower half.
    pub top_half: bool,
}

/// Pure-geometry bowsprit, in the local frame: the **solid prow** (a fully-filled
/// tapered nose extending the bow to a point — no interior), the underside-curve
/// bevel cells, the prow deck-edge outline (so the main railing can wrap it), the
/// projecting spar, the chosen rake, and the forward tip.
#[derive(Debug, Clone)]
pub struct BowspritModel {
    pub rake: BowspritRake,
    /// Solid prow fill — full blocks (completely filled, no hollow interior).
    pub prow: Vec<Point3D>,
    /// Underside-curve bevel — upside-down stairs smoothing the prow's rising bottom.
    pub prow_bevel: Vec<BowspritCell>,
    /// Prow deck-edge outline: `(x, half_width)` per prow station at the deck level.
    /// Merged into the deck outline so the shared main railing wraps the prow (the
    /// bowsprit no longer builds its own rail).
    pub deck_outline: Vec<(i32, i32)>,
    /// Main spar cells (z = 0).
    pub spar: Vec<BowspritCell>,
    /// Forward tip of the spar (local).
    pub tip: Point3D,
}

fn slab_cell(x: i32, y: i32, top_half: bool) -> BowspritCell {
    slab_at(x, y, 0, top_half)
}
fn slab_at(x: i32, y: i32, z: i32, top_half: bool) -> BowspritCell {
    BowspritCell { local: Point3D::new(x, y, z), form: BlockForm::Slab, facing: None, top_half }
}
fn stair_at(x: i32, y: i32, z: i32, facing: ShipDir, top_half: bool) -> BowspritCell {
    BowspritCell { local: Point3D::new(x, y, z), form: BlockForm::Stairs, facing: Some(facing), top_half }
}
/// A full block on the spar centerline (z = 0).
fn block_cell(x: i32, y: i32) -> BowspritCell {
    BowspritCell { local: Point3D::new(x, y, 0), form: BlockForm::Block, facing: None, top_half: false }
}

/// The spar beam from the stem point `(x0, y0)` forward to `x1`, one block thick on the
/// centerline (z = 0), built from **full blocks + slabs only** (no stairs) so it's easy
/// to build by hand. [`BowspritRake::Straight`] lays a flat run of blocks. The angled
/// rakes alternate `flat_run` full blocks at the current level with a group of
/// `climb_run` one-block climbs, each climb a **single column holding two slabs** — a top
/// slab in the lower cell plus a bottom slab one cell up (`top-slab + bottom-slab+1`),
/// the top slab supporting the half-block-raised bottom slab into a smooth diagonal.
/// Returns the cells and the forward tip.
fn step_spar(x0: i32, y0: i32, x1: i32, rake: BowspritRake) -> (Vec<BowspritCell>, Point3D) {
    let mut cells = Vec::new();
    let flat_run = rake.flat_run();
    let climb_run = rake.climb_run();
    let mut level = y0;
    let mut flats = 0; // full blocks laid at the current level since the last climb group
    let mut climbs = 0; // climbs done in the current group
    let mut tip = Point3D::new(x0, y0, 0);
    for x in x0..=x1 {
        if flats < flat_run {
            cells.push(block_cell(x, level));
            flats += 1;
            tip = Point3D::new(x, level, 0);
        } else {
            // A climb-group column: a double-slab tread at the **current** level (top
            // slab in the cell + bottom slab one cell up). Every column in the group
            // sits at the same height; the level only steps up once, after the group —
            // so `climb_run > 1` widens the tread rather than climbing repeatedly.
            cells.push(slab_cell(x, level, true));
            cells.push(slab_cell(x, level + 1, false));
            tip = Point3D::new(x, level + 1, 0);
            climbs += 1;
            if climbs >= climb_run {
                level += 1;
                flats = 0;
                climbs = 0;
            }
        }
    }
    (cells, tip)
}

/// Maximum forward reach of the bowsprit **beyond the bow tip** (`bow_x = length - 1`),
/// in local `+x` blocks: the prow's stem extension (`ext`) plus the longest spar (the
/// `Straight` rake — `reach_factor == 1.0`, so this bounds every rake). Returns `0` for
/// ships too small to carry a bowsprit (Small tier), so the placement fit-solver reserves
/// forward open water only when a spar will actually be built. Mirrors the geometry baked
/// into [`build_bowsprit_model`] — kept here so the reserved footprint can't drift from it.
pub fn bowsprit_reach(length: i32) -> i32 {
    if SizeTier::from_length(length) < SizeTier::Medium {
        return 0;
    }
    let ext = (((length as f32) * 0.10).round() as i32).max(2);
    let spar_reach = ((length as f32) * REACH_FRACTION).round().max(3.0) as i32; // reach_factor ≤ 1.0
    ext + spar_reach
}

/// Build the bowsprit geometry for a chosen `rake` (Approach B: the prow **mimics the
/// hull** — a flared nose tapering to a stem point and a keel point, decked + railed,
/// solid for Small ships / a hollow shell for larger, with the spar projecting on).
///
/// `deck_y` = main deck (waterline); `top_y` = topmost weather deck (`== deck_y` ⇒ no
/// raised deck); `keel_top` = keel crest Y per station; `hull_top_half` = the hull's
/// waterline half-beam per station (the prow blends from it); `solid` fills the prow
/// (Small) instead of leaving a hollow interior. Pure geometry.
pub fn build_bowsprit_model(
    length: i32,
    deck_y: i32,
    top_y: i32,
    keel_top: &[i32],
    hull_top_half: &[i32],
    solid: bool,
    rake: BowspritRake,
) -> BowspritModel {
    let bow_x = length - 1;
    let plat_y = top_y; // prow deck = weather deck (= main deck when there's no raised deck)
    let water = deck_y.min(plat_y);

    // The prow rebuilds the forward bow as a sharp flared nose, then runs out to a stem
    // point a little past the bow.
    let prow_back = ((length as f32) * 0.30).round() as i32;
    let ext = (((length as f32) * 0.10).round() as i32).max(2);
    let x0 = (bow_x - prow_back).max(0);
    let x_stem = bow_x + ext;

    let hull_half = |x: i32| -> i32 {
        hull_top_half.get(x.max(0) as usize).copied().unwrap_or(0)
    };
    let base_w = hull_half(x0).max(2);

    // Waterline half-beam along the prow: blends from the hull at `x0`, tapering to a
    // point at the stem.
    let hw_w = |x: i32| -> i32 {
        let u = ((x - x0) as f32 / (x_stem - x0).max(1) as f32).clamp(0.0, 1.0);
        (base_w as f32 * (1.0 - u).powf(0.8)).round() as i32
    };
    // Keel point (bottom) per station: the keel crest where it exists, sweeping up to
    // the deck at the stem so the underside meets the bowsprit.
    let kb = |x: i32| -> i32 {
        if x <= bow_x {
            match keel_top.get(x.max(0) as usize).copied() {
                Some(v) if v != i32::MIN => v.max(0),
                _ => 0,
            }
        } else {
            let u = (x - bow_x) as f32 / (x_stem - bow_x).max(1) as f32;
            water + ((plat_y - water) as f32 * u).round() as i32
        }
    };
    // Cross-section half-width at (x, y): flared below the waterline to the keel point,
    // vertical topside above. `-1` = outside the hull.
    let half_at = |x: i32, y: i32| -> i32 {
        let bottom = kb(x);
        if y < bottom || y > plat_y {
            return -1;
        }
        let hw = hw_w(x);
        if hw < 1 {
            return 0; // single keel/stem column
        }
        if y <= water {
            let span = (water - bottom).max(1) as f32;
            let t = ((y - bottom) as f32 / span).clamp(0.0, 1.0);
            (hw as f32 * t.powf(PROW_FLARE_POW)).round() as i32
        } else {
            hw
        }
    };
    let occ = |x: i32, y: i32, z: i32| -> bool {
        let h = half_at(x, y);
        h >= 0 && z.abs() <= h
    };

    let mut prow = Vec::new();
    let mut prow_bevel = Vec::new();
    let mut deck_outline = Vec::new();
    for x in x0..=x_stem {
        let hw = hw_w(x);
        // Don't overwrite the keel: where it exists (`x <= bow_x`) start one above its
        // crest so its bottom course (a top slab) stays intact — like the hull, which
        // sits strictly above the keel. The stem extension (`x > bow_x`, no keel) fills
        // from its own swept bottom.
        let y_start = if x <= bow_x { kb(x) + 1 } else { kb(x) };
        for y in y_start..=plat_y {
            let h = half_at(x, y);
            if h < 0 {
                continue;
            }
            // The bilge flares out going up; bevel the widening outer edge with an
            // upside-down stair (facing outboard) so the hull reads as a smooth curve
            // rather than blocky steps.
            let widening = y <= water && h >= 1 && h > half_at(x, y - 1);
            for z in -h..=h {
                let is_top = y == plat_y;
                let outer = z.abs() == h;
                let perim = outer || !occ(x - 1, y, z) || !occ(x + 1, y, z) || !occ(x, y - 1, z);
                // Bevel stair on the flaring outer face — faces inboard so it sits on
                // the slope (the widening bilge and the deck rim, //---//). Where the
                // prow narrows forward the outward face points toward the bow, so those
                // cells bevel toward the stern instead of port/starboard.
                let bevel_dir = if !occ(x + 1, y, z) {
                    ShipDir::Stern
                } else if z > 0 {
                    ShipDir::Port
                } else {
                    ShipDir::Starboard
                };
                if !solid && !perim && !is_top {
                    continue; // hollow interior for larger ships
                }
                if is_top {
                    if outer && z != 0 {
                        // Deck rim: a right-side-up stair (solid bottom, step on top) so
                        // the rim reads as a raised lip rather than an overhang.
                        prow_bevel.push(stair_at(x, y, z, bevel_dir, false));
                    } else {
                        prow_bevel.push(slab_at(x, y, z, true)); // deck surface (top slab)
                    }
                } else if outer && widening && z != 0 {
                    prow_bevel.push(stair_at(x, y, z, bevel_dir, true)); // bilge bevel
                } else if y > water && occ(x, y, z + 1) && occ(x, y, z - 1) && !occ(x + 1, y, z) {
                    // Above-water forward plan taper: rake the bow-facing end with an
                    // upside-down stair (the hull bilge-bevel idea, applied along x) so the
                    // nose narrows smoothly instead of stepping blockily.
                    prow_bevel.push(stair_at(x, y, z, ShipDir::Bow, true));
                } else {
                    prow.push(Point3D::new(x, y, z));
                }
            }
        }
        // The outermost prow cells (z = ±hw) are the deck-rim stairs; the solid deck
        // surface (top slabs) sits one in, at hw-1. Feed *that* to the railing so it
        // stands on the deck, not on the rim stair. (hw == 1 is too narrow for a rail
        // inside the rim, so skip it — the spar covers the very tip.)
        if hw >= 2 {
            deck_outline.push((x, hw - 1));
        }
    }

    // Spar: continues from the stem point, raking forward + up. Steeper rakes are
    // shortened (`reach_factor`) so they don't climb away into the sky. It starts one
    // station *behind* the stem point so the beam begins flush in the prow (no slab
    // sitting between the prow and the spar).
    let spar_reach = ((length as f32) * REACH_FRACTION * rake.reach_factor()).round().max(3.0) as i32;
    let spar_start = x_stem - 1;
    let tip_x = x_stem + spar_reach;
    let (spar, tip) = step_spar(spar_start, plat_y, tip_x, rake);

    BowspritModel { rake, prow, prow_bevel, deck_outline, spar, tip }
}

/// Place the bowsprit (flared prow + the projecting block/slab spar) and record it in
/// `state`.
pub async fn build(ctx: &mut ShipCtx<'_>, dc: &DeckContext<'_>, state: &mut DeckState) {
    let has_deck = state.top_y > dc.deck.deck_y;
    let rake = BowspritRake::pick(has_deck, ctx.rng);
    let keel_top = dc.keel.top_profile();
    let solid = dc.tier == SizeTier::Small;
    let model = build_bowsprit_model(
        dc.hull.length,
        dc.deck.deck_y,
        state.top_y,
        &keel_top,
        &dc.hull.top_half,
        solid,
        rake,
    );

    let material = ctx
        .palette
        .get_material(dc.ship_palette.role(ShipPart::Spar))
        .expect("Spar role missing from base palette")
        .clone();
    let mut placer_rng = ctx.rng.derive();
    let mut placer = MaterialPlacer::new(Placer::new(&ctx.data.materials, &mut placer_rng), material);
    let place = dc.placement;

    // Solid prow fill (full blocks — no interior).
    for &cell in &model.prow {
        placer
            .place_block(ctx.editor, place.to_world(cell), BlockForm::Block, None, None)
            .await;
    }

    // Prow underside bevel (stairs) + spar (block/slab beam).
    for cell in model.prow_bevel.iter().chain(model.spar.iter()) {
        let st = cell_state(cell, place);
        placer
            .place_block(ctx.editor, place.to_world(cell.local), cell.form, st.as_ref(), None)
            .await;
    }

    // Extend the top weather-deck outline forward over the prow so the **shared main
    // railing** (built next) wraps the prow too — the bowsprit no longer adds its own.
    // The prow deck sits flush with the top deck (`plat_y == state.top_y`), so the
    // outlines join at the same level. `max` keeps the wider edge in the overlap.
    for &(x, hw) in &model.deck_outline {
        let xi = x as usize;
        if xi >= state.top_outline.len() {
            state.top_outline.resize(xi + 1, 0);
        }
        if xi >= state.rail_outline.len() {
            state.rail_outline.resize(xi + 1, 0);
        }
        state.top_outline[xi] = state.top_outline[xi].max(hw);
        state.rail_outline[xi] = state.rail_outline[xi].max(hw);
    }

    state.bowsprit = Some(model);
}

/// Blockstate for a bowsprit cell: stairs (prow bevel only) use their stored `facing`
/// with `half` from `top_half`; slabs take `type` from `top_half`; full blocks need no
/// state. Above the deck, so never waterlogged.
fn cell_state(cell: &BowspritCell, placement: &Placement) -> Option<HashMap<String, String>> {
    let half = if cell.top_half { "top" } else { "bottom" };
    match cell.form {
        BlockForm::Stairs => {
            let dir = cell.facing.unwrap_or(RAKE_STAIR_FACE);
            Some(HashMap::from([
                ("facing".to_string(), placement.world_cardinal(dir).to_string()),
                ("half".to_string(), half.to_string()),
            ]))
        }
        BlockForm::Slab => Some(HashMap::from([("type".to_string(), half.to_string())])),
        _ => None,
    }
}
