//! Ship builder **v2** — interactive co-design (see `docs/plans/ship-builder-v2.md`).
//!
//! Built fresh alongside v1 (`super::ships`), which stays intact. v2 reuses only
//! the harness — [`Placement`] / [`ShipDir`] (local→world transform + ship-relative
//! facings), the materials system, and the offline/live [`Editor`] — and writes
//! its geometry from scratch, one algorithm at a time, verified in-game per step.
//!
//! Local frame (same as v1): `x` = stern(0) → bow(+x), `z` = beam (centerline 0),
//! `y` = up from the keel bottom (0).
//!
//! **Current scope:** Stage 1 → the **keel** step only (`keel`). Hull and rudder
//! follow.

pub mod palette;
pub mod additions;
pub mod keel;
pub mod hull;
pub mod rudder;
pub mod deck;
pub mod blueprint;

#[cfg(test)]
mod test;

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::Palette;
use crate::geometry::{Cardinal, Point2D, Point3D};
use crate::noise::RNG;

// Reused harness from v1: the local→world transform and ship-relative facings.
pub use super::ships::{Placement, ShipDir};

pub use additions::{DeckAddition, SizeTier};
pub use hull::HullShape;
use additions::bowsprit::BowspritModel;
use additions::railing::RailingModel;
use deck::DeckModel;
use hull::HullModel;
use keel::KeelModel;
use palette::ShipPalette;
use rudder::RudderModel;

/// Default length:beam ratio (max beam = length / ratio). ~2.7 ≈ the tutorial's
/// stout hull; higher is sleeker.
pub const DEFAULT_BEAM_RATIO: f32 = 2.7;

/// Cross-cutting context threaded through every v2 ship stage (mirrors v1 `ShipCtx`).
pub struct ShipV2Ctx<'a> {
    pub editor: &'a mut Editor,
    pub data: &'a LoadedData,
    pub palette: &'a Palette,
    pub rng: &'a mut RNG,
}

impl<'a> ShipV2Ctx<'a> {
    pub fn new(editor: &'a mut Editor, data: &'a LoadedData, palette: &'a Palette, rng: &'a mut RNG) -> Self {
        Self { editor, data, palette, rng }
    }
}

/// Per-ship inputs. `length` is passed in (chosen per ship class upstream); the
/// keel derives its depth / rake / post from it. The vertical footing is decided
/// automatically from the terrain at the anchor (land vs water).
#[derive(Debug, Clone, Copy)]
pub struct ShipV2Context {
    /// Bow direction (cardinal headings only, like v1).
    pub heading: Cardinal,
    /// Tip-to-tip keel length.
    pub length: i32,
    /// Length:beam ratio — max hull beam = `length / beam_ratio`.
    pub beam_ratio: f32,
    /// Plan-view hull shape.
    pub hull_shape: HullShape,
}

impl ShipV2Context {
    /// Build a context with default beam ratio ([`DEFAULT_BEAM_RATIO`]) and a
    /// teardrop hull.
    pub fn new(heading: Cardinal, length: i32) -> Self {
        Self { heading, length, beam_ratio: DEFAULT_BEAM_RATIO, hull_shape: HullShape::Teardrop }
    }

    /// Set the length:beam ratio (lower = beamier, higher = sleeker).
    pub fn with_beam_ratio(mut self, beam_ratio: f32) -> Self {
        self.beam_ratio = beam_ratio;
        self
    }

    /// Set the plan-view hull shape.
    pub fn with_hull_shape(mut self, hull_shape: HullShape) -> Self {
        self.hull_shape = hull_shape;
        self
    }
}

/// What a v2 build produces so far (keel + hull shell + rudder).
pub struct ShipV2Output {
    pub placement: Placement,
    pub keel: KeelModel,
    pub hull: HullModel,
    pub rudder: RudderModel,
    pub deck: DeckModel,
    /// The main railing around the top weather deck (built by the additions pipeline).
    pub railing: Option<RailingModel>,
    /// The bowsprit off the bow (built by the additions pipeline).
    pub bowsprit: Option<BowspritModel>,
    /// Size tier derived from length — gates which deck additions are built.
    pub tier: SizeTier,
    /// `true` if the anchor was over water (built below the surface), `false` if on
    /// land (built resting on the ground).
    pub on_water: bool,
}

/// Build the current v2 ship at `anchor` (the stern keel point, in world X/Z).
///
/// The footing adapts to the terrain at `anchor` (Stage-1 land/water rule):
/// - **water:** the keel's flat bottom (local `y = 0`) sits `depth` below the
///   surface, clamped so it never digs below the seabed (shallow water → rests on
///   the bottom);
/// - **land:** the flat bottom rests on the ground, so everything is built above.
pub async fn build_ship_v2(
    ctx: &mut ShipV2Ctx<'_>,
    context: &ShipV2Context,
    anchor: Point2D,
) -> ShipV2Output {
    let ship_palette = ShipPalette::ship_oak_default();

    // 1. Keel model — pure geometry in the local frame.
    let keel = keel::build_keel_model(context.length);

    // 2. Resolve the footing: world Y that the keel's flat bottom (local y=0) sits
    //    at. Heightmaps are local to the build origin, usable directly as origin.y.
    let world = ctx.editor.world();
    let on_water = world.is_water(anchor);
    let bottom_y = if on_water {
        let surface = world.get_motion_blocking_height_at(anchor);
        let seabed = world.get_ocean_floor_height_at(anchor);
        (surface - keel.depth).max(seabed)
    } else {
        world.get_height_at(anchor)
    };
    let placement = Placement::new(Point3D::new(anchor.x, bottom_y, anchor.y), context.heading);

    // 3. Place the keel (waterlog stairs/slabs only when on water).
    keel::place_keel(ctx, &keel, &placement, &ship_palette, on_water).await;

    // 4. Build + place the hull shell upon the keel (blocks only for now). The hull
    //    sits on the keel's crest so the keel stays the outermost, water-touching part.
    let hull = hull::build_hull_model(
        context.length,
        keel.depth,
        context.beam_ratio,
        context.hull_shape,
        &keel.top_profile(),
    );
    hull::place_hull(ctx, &hull, &placement, &ship_palette, on_water).await;

    // 5. Rudder: a raked blade hung aft of the sternpost via fences.
    let rudder = rudder::build_rudder_model(keel.depth);
    rudder::place_rudder(ctx, &rudder, &placement, &ship_palette, on_water).await;

    // 6. Deck: cap the hull's open top with a slab deck.
    let deck = deck::build_deck_model(&hull);
    deck::place_deck(ctx, &deck, &placement, &ship_palette).await;

    // 7. Deck additions — each a modular submodule, run in order. The additional deck
    //    now respects size gating (Small ships, length ≤20, skip it → just a railed
    //    main deck); the other additions still build for all sizes until their gating
    //    is wired up.
    let tier = SizeTier::from_length(context.length);
    let deck_ctx = additions::DeckContext {
        placement: &placement,
        hull: &hull,
        deck: &deck,
        ship_palette: &ship_palette,
        tier,
        on_water,
    };
    // Running top-weather-deck state: structural additions raise it, fittings read it.
    let mut deck_state = additions::DeckState::initial(&hull, &deck);
    for &addition in &additions::BUILD_ORDER {
        if addition == DeckAddition::AdditionalDeck && !tier.has(addition) {
            continue; // Small ships have no additional deck.
        }
        additions::build_addition(addition, ctx, &deck_ctx, &mut deck_state).await;
    }
    drop(deck_ctx);
    let railing = deck_state.railing;
    let bowsprit = deck_state.bowsprit;

    ShipV2Output { placement, keel, hull, rudder, deck, railing, bowsprit, tier, on_water }
}
