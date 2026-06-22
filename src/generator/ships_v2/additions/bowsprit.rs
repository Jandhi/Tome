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
use super::{DeckContext, DeckState};

/// Which way the spar/knee rake stairs face. The beam ascends toward the bow; the
/// stairs face the **bow** so the bevel runs along the ascending diagonal. Classic MC
/// stair flip point: if a screenshot shows the bevel on the wrong face, swap this.
const RAKE_STAIR_FACE: ShipDir = ShipDir::Bow;

/// Forward projection of the spar past the bow, as a fraction of hull length
/// ("a little less than half" → the poking-out part is ~0.4·length).
const REACH_FRACTION: f32 = 0.4;

/// Length of the tapered prow extension as a fraction of hull length (the beakhead
/// that continues the bow forward to a point and carries the spar).
const BEAK_FRACTION: f32 = 0.18;

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
    BowspritCell { local: Point3D::new(x, y, 0), form: BlockForm::Slab, facing: None, top_half }
}
fn stair_cell(x: i32, y: i32, top_half: bool) -> BowspritCell {
    BowspritCell {
        local: Point3D::new(x, y, 0),
        form: BlockForm::Stairs,
        facing: Some(RAKE_STAIR_FACE),
        top_half,
    }
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

/// Build the bowsprit geometry for a chosen `rake`. `deck_y` is the main deck and
/// `top_y` the topmost weather deck (`top_y == deck_y` ⇒ no raised deck). Pure
/// geometry — pick the rake with [`BowspritRake::pick`].
pub fn build_bowsprit_model(
    length: i32,
    deck_y: i32,
    top_y: i32,
    rake: BowspritRake,
) -> BowspritModel {
    let bow_x = length - 1;
    let has_deck = top_y > deck_y;
    // The prow top sits flush with the topmost weather deck (or just above the main
    // deck), so the two surfaces connect.
    let plat_y = if has_deck { top_y } else { deck_y + 1 };

    // Solid prow: a fully-filled tapered nose continuing the bow forward to a point.
    // Its **underside is a curve anchored at the keel crest** where it peeks out at the
    // bow — in the local frame the keel's bow rake reaches `y = depth = waterline =
    // deck_y` at the bow tip, so `bottom_base = deck_y` is exactly that keel top — and
    // sweeps up to meet the **bowsprit underside** at the point. Width tapers `base_hw →
    // 0` over the same span, so the mass narrows in both plan and section. It starts a
    // couple of stations *behind* the bow so it merges solidly with the hull/deck.
    let bottom_base = deck_y; // top of the keel, peeking out at the bow waterline
    let height = plat_y - bottom_base; // keel crest → weather deck
    // Long enough that the `^1.5` underside curve never rises more than 1/column (its
    // slope peaks at the keel and eases to horizontal where it meets the spar).
    let beak_len = ((length as f32 * BEAK_FRACTION).max(1.5 * height as f32)).round().max(2.0) as i32;
    let base_hw = (length / 14).clamp(2, 4);
    const BACK_OVERLAP: i32 = 2;
    let x_start = (bow_x - BACK_OVERLAP).max(0);
    let x_tip = bow_x + beak_len;

    // Plan half-width and underside Y at a station (full/low behind the bow, tapering to
    // a point forward). `(1−t)^p` curves give the nice tipped taper.
    let param = |x: i32| -> f32 {
        let dxf = (x - bow_x).max(0) as f32;
        (dxf / beak_len as f32).min(1.0)
    };
    let hw_at = |x: i32| -> i32 {
        let t = param(x);
        (base_hw as f32 * (1.0 - t).powf(0.7)).round() as i32
    };
    let bottom_at = |x: i32| -> i32 {
        let t = param(x);
        plat_y - ((plat_y - bottom_base) as f32 * (1.0 - t).powf(1.5)).round() as i32
    };

    let mut prow = Vec::new();
    let mut prow_bevel = Vec::new();
    let mut rail = Vec::new();
    for x in x_start..=x_tip {
        let hw = hw_at(x);
        let bot = bottom_at(x);
        let rising = bot > bottom_at(x - 1); // underside steps up toward the point
        for z in -hw..=hw {
            // Smooth the rising underside with an upside-down stair (curve below, solid
            // top); otherwise a full block. Fill the rest solid up to the deck.
            if rising {
                prow_bevel.push(stair_cell(x, bot, true));
            } else {
                prow.push(Point3D::new(x, bot, z));
            }
            for y in (bot + 1)..=plat_y {
                prow.push(Point3D::new(x, y, z));
            }
        }
        if hw >= 1 {
            rail.push(Point3D::new(x, plat_y + 1, hw));
            rail.push(Point3D::new(x, plat_y + 1, -hw));
        }
    }

    // Spar: continues from the prow point, raking forward + up. Steeper rakes are
    // shortened (`reach_factor`) so they don't climb away into the sky.
    let spar_reach = ((length as f32) * REACH_FRACTION * rake.reach_factor()).round().max(3.0) as i32;
    let tip_x = x_tip + spar_reach;
    let (spar, tip_y) = ramp(x_tip, plat_y, tip_x, rake.slope());

    BowspritModel { rake, prow, prow_bevel, rail, spar, tip: Point3D::new(tip_x, tip_y, 0) }
}

/// Place the bowsprit (spar + knee) as a 1-thick slab/stair beam and record it in
/// `state`.
pub async fn build(ctx: &mut ShipV2Ctx<'_>, dc: &DeckContext<'_>, state: &mut DeckState) {
    let has_deck = state.top_y > dc.deck.deck_y;
    let rake = BowspritRake::pick(has_deck, ctx.rng);
    let model = build_bowsprit_model(dc.hull.length, dc.deck.deck_y, state.top_y, rake);

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

/// Blockstate for a bowsprit cell: stairs face the rake direction with `half` from
/// `top_half`; slabs take `type` from `top_half`. An **upside-down** stair must face
/// the **opposite** way to a right-side-up one to keep the bevel on the same ascending
/// diagonal (the classic double-stair smoothing trick). Above the deck, so never
/// waterlogged.
fn cell_state(cell: &BowspritCell, placement: &Placement) -> Option<HashMap<String, String>> {
    let half = if cell.top_half { "top" } else { "bottom" };
    match cell.form {
        BlockForm::Stairs => {
            let dir = cell.facing.unwrap_or(RAKE_STAIR_FACE);
            let dir = if cell.top_half { dir.opposite() } else { dir };
            Some(HashMap::from([
                ("facing".to_string(), placement.world_cardinal(dir).to_string()),
                ("half".to_string(), half.to_string()),
            ]))
        }
        BlockForm::Slab => Some(HashMap::from([("type".to_string(), half.to_string())])),
        _ => None,
    }
}
