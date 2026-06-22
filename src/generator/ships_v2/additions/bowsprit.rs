//! Deck addition · **Bowsprit** — a beakhead prow + the forward-poking spar.
//!
//! Rather than two thin lines floating off the deck, the bow is **extended forward
//! into a tapered prow** (a "beakhead") that carries the bowsprit: a `beak_floor` of
//! top slabs continuing the bow outline to a point, edged with `beak_rail` fences. The
//! centerline (z=0) **spar** projects on from the prow tip, and a **support knee**
//! braces the prow from the hull bow below — so the front reads as real structure with
//! a tipped taper sized to hold the bowsprit.
//!
//! **Smoothed, not blocky:** both spar and knee are drawn the same way the keel's bow
//! rake is — a **top slab** where the line is flat and an **upside-down stair** where
//! it steps up one — so the beam reads as a smooth taper rather than a staircase of
//! cubes. (That needs a material with stair/slab variants, so the `Spar` role is a
//! plank wood, not a log.) Both rake and knee are kept to slope ≤ 1 so only slabs +
//! stairs are ever needed (no full blocks).
//!
//! Rake adapts to the anchor height: with a raised deck the anchor is already high, so
//! the spar can run **straight** (horizontal); without one it rakes **upward** from the
//! low stem. Length ≈ a little less than half the hull (the visible forward reach is
//! ~0.4·length). Figurehead / rigging / decoration come in a later pass.

use std::collections::HashMap;

use crate::generator::materials::{MaterialPlacer, Placer};
use crate::geometry::Point3D;
use crate::minecraft::BlockForm;
use crate::noise::RNG;

use super::super::palette::ShipPart;
use super::super::{Placement, ShipDir, ShipV2Ctx};
use super::{DeckContext, DeckState, SizeTier};

/// Which way the spar/knee rake stairs face. The beam ascends toward the bow; the
/// stairs face the **bow** so the bevel runs along the ascending diagonal. Classic MC
/// stair flip point: if a screenshot shows the bevel on the wrong face, swap this.
const RAKE_STAIR_FACE: ShipDir = ShipDir::Bow;

/// Forward projection of the spar past the bow, as a fraction of hull length
/// ("a little less than half" → the poking-out part is ~0.4·length).
const REACH_FRACTION: f32 = 0.4;

/// Flare exponent of the prow cross-section below the waterline (mirrors the hull's
/// rounded bilge): half-width = deck_half · (h/span)^p, tapering to 0 at the keel
/// point. `< 1` = rounder bilge, `> 1` = sharper V.
const PROW_FLARE_POW: f32 = 0.9;

/// Upward rake of the spar: 0 (straight) or a slope, selected by [`BowspritRake`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BowspritRake {
    /// Horizontal — used when a raised deck gives a high anchor.
    Straight,
    /// ~20° up.
    Gentle,
    /// ~30° up.
    Medium,
    /// ~40° up.
    Steep,
}

impl BowspritRake {
    /// Rise per unit forward run.
    fn slope(self) -> f32 {
        match self {
            BowspritRake::Straight => 0.0,
            BowspritRake::Gentle => 0.36,
            BowspritRake::Medium => 0.58,
            BowspritRake::Steep => 0.84,
        }
    }

    /// Forward-reach multiplier — steeper rakes are **shortened** so they don't climb
    /// far up into the sky (they read as a bowsprit, not a staircase ramp).
    fn reach_factor(self) -> f32 {
        match self {
            BowspritRake::Straight => 1.0,
            BowspritRake::Gentle => 0.85,
            BowspritRake::Medium => 0.6,
            BowspritRake::Steep => 0.4,
        }
    }

    /// The angled rakes (everything but [`Straight`]).
    const ANGLED: [BowspritRake; 3] =
        [BowspritRake::Gentle, BowspritRake::Medium, BowspritRake::Steep];

    /// Pick a rake. A raised deck gives a high anchor so **all four** are allowed
    /// (including [`Straight`]); without one, [`Straight`] is excluded (it needs the
    /// high anchor) and we pick among the angled rakes. Randomised for now — later a
    /// higher-level system may choose which parts/rake go together per ship.
    pub fn pick(has_deck: bool, rng: &mut RNG) -> BowspritRake {
        if has_deck {
            match rng.rand_i32_range(0, 4) {
                0 => BowspritRake::Straight,
                1 => BowspritRake::Gentle,
                2 => BowspritRake::Medium,
                _ => BowspritRake::Steep,
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
/// bevel cells, the prow-top edge rails, the projecting spar, the chosen rake, and the
/// forward tip.
#[derive(Debug, Clone)]
pub struct BowspritModel {
    pub rake: BowspritRake,
    /// Solid prow fill — full blocks (completely filled, no hollow interior).
    pub prow: Vec<Point3D>,
    /// Underside-curve bevel — upside-down stairs smoothing the prow's rising bottom.
    pub prow_bevel: Vec<BowspritCell>,
    /// Prow-top edge rails — fences along the prow sides.
    pub rail: Vec<Point3D>,
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
/// A centerline spar/wedge stair: it ascends toward the bow, so an **upside-down**
/// stair faces the opposite way to a right-side-up one to keep both halves on the same
/// diagonal (the double-stair smoothing trick).
fn stair_cell(x: i32, y: i32, top_half: bool) -> BowspritCell {
    let facing = if top_half { RAKE_STAIR_FACE.opposite() } else { RAKE_STAIR_FACE };
    stair_at(x, y, 0, facing, top_half)
}

/// A **full-block step** rendered as a smooth double-stair wedge in one column: an
/// upside-down stair filling the top half of the lower cell + a right-side-up stair
/// filling the bottom half of the upper cell. With their facings flipped opposite
/// (handled in `cell_state`) the two bevels meet into one continuous diagonal — stairs
/// "on both sides" — instead of a jagged single-stair step.
fn push_step(cells: &mut Vec<BowspritCell>, x: i32, low_cell: i32) {
    cells.push(stair_cell(x, low_cell, true)); // upside-down, top half of the lower cell
    cells.push(stair_cell(x, low_cell + 1, false)); // right-side-up, bottom half of the upper cell
}

/// A smoothed ascending beam from `(x0, y0)` forward to `x1` at slope `s` (0..=1), one
/// block thick in plan (z=0), tracked at **half-block resolution**. Per column: a
/// **flat** half-level → slab; a **half-block** rise → a single stair (upside-down in
/// the cell's upper half, right-side-up in the lower); a **full-block** rise → a smooth
/// double-stair wedge ([`push_step`], stairs on both sides). Top/bottom slabs across
/// columns give the shallow "two slabs" ramp. Returns the cells and the final cell Y.
fn ramp(x0: i32, y0: i32, x1: i32, s: f32) -> (Vec<BowspritCell>, i32) {
    let mut cells = Vec::new();
    // Work in half-units so a stair can capture a half-block rise.
    let h0 = y0 * 2;
    let mut prev_hh = h0;
    let mut last_cell_y = y0;
    for x in x0..=x1 {
        let hh = (h0 as f32 + (x - x0) as f32 * s * 2.0).round() as i32;
        let cell_y = hh.div_euclid(2);
        let upper = hh.rem_euclid(2) == 1; // odd half-unit ⇒ upper half of the cell
        match hh - prev_hh {
            d if d <= 0 => cells.push(slab_cell(x, cell_y, upper)), // flat
            1 => cells.push(stair_cell(x, cell_y, upper)),          // half-block step
            _ => push_step(&mut cells, x, cell_y - 1),              // full-block step → wedge
        }
        prev_hh = hh;
        last_cell_y = cell_y;
    }
    (cells, last_cell_y)
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
    let mut rail = Vec::new();
    for x in x0..=x_stem {
        let hw = hw_w(x);
        for y in kb(x)..=plat_y {
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
                        prow_bevel.push(stair_at(x, y, z, bevel_dir, true)); // deck rim
                    } else {
                        prow_bevel.push(slab_at(x, y, z, true)); // deck surface (top slab)
                    }
                } else if outer && widening && z != 0 {
                    prow_bevel.push(stair_at(x, y, z, bevel_dir, true)); // bilge bevel
                } else {
                    prow.push(Point3D::new(x, y, z));
                }
            }
        }
        if hw >= 1 {
            rail.push(Point3D::new(x, plat_y + 1, hw));
            rail.push(Point3D::new(x, plat_y + 1, -hw));
        }
    }

    // Spar: continues from the stem point, raking forward + up. Steeper rakes are
    // shortened (`reach_factor`) so they don't climb away into the sky.
    let spar_reach = ((length as f32) * REACH_FRACTION * rake.reach_factor()).round().max(3.0) as i32;
    let tip_x = x_stem + spar_reach;
    let (spar, tip_y) = ramp(x_stem, plat_y, tip_x, rake.slope());

    BowspritModel { rake, prow, prow_bevel, rail, spar, tip: Point3D::new(tip_x, tip_y, 0) }
}

/// Place the bowsprit (spar + knee) as a 1-thick slab/stair beam and record it in
/// `state`.
pub async fn build(ctx: &mut ShipV2Ctx<'_>, dc: &DeckContext<'_>, state: &mut DeckState) {
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
    // Prow-top edge rails (fences).
    let fence_state: HashMap<String, String> = HashMap::new();
    for &cell in &model.rail {
        placer
            .place_block(ctx.editor, place.to_world(cell), BlockForm::Fence, Some(&fence_state), None)
            .await;
    }

    // Underside bevel + spar (smoothed slab/stair beam).
    for cell in model.prow_bevel.iter().chain(model.spar.iter()) {
        let st = cell_state(cell, place);
        placer
            .place_block(ctx.editor, place.to_world(cell.local), cell.form, st.as_ref(), None)
            .await;
    }

    state.bowsprit = Some(model);
}

/// Blockstate for a bowsprit cell: stairs use their stored `facing` (already correct —
/// see [`stair_cell`]) with `half` from `top_half`; slabs take `type` from `top_half`.
/// Above the deck, so never waterlogged.
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
