//! Stage 2 · deck-addition catalog + **size gating**.
//!
//! Ships get a [`SizeTier`] derived from their length; the tier gates which deck
//! additions are built. Some are required (for ships big enough), the rest optional;
//! the smallest ships are restricted from the larger features.
//!
//! This module is the catalog + gating + dispatch. Each addition is complex enough
//! to live in its own submodule under `additions/` (e.g. `additions/gallery.rs`,
//! `additions/railing.rs`, …), declared here as it's built. Every addition exposes
//! a uniform `pub async fn build(ctx: &mut ShipV2Ctx, dc: &DeckContext)`, so adding
//! one is: new file + `pub mod x;` + a match arm in [`build_addition`]. The pipeline
//! just iterates [`BUILD_ORDER`].

use std::collections::HashMap;

use crate::generator::materials::{MaterialPlacer, Placer};
use crate::geometry::Point3D;
use crate::minecraft::BlockForm;

use super::deck::DeckModel;
use super::hull::HullModel;
use super::keel::KeelModel;
use super::palette::{ShipPalette, ShipPart};
use super::tuning::GUN_PORT_STEP;
use super::{Placement, ShipDir, ShipV2Ctx};

pub mod additional_deck;
pub mod bowsprit;
pub mod masts;
pub mod railing;

/// Mutable state threaded through the addition pipeline: the current **topmost open
/// weather deck** — its edge outline (half-beam per station) and floor Y. Later
/// additions (railing, masts, fittings) build against this rather than the raw main
/// deck, so they sit on whatever the structural additions raised. Initialised to the
/// main deck; the additional deck(s) raise it as they stack.
pub struct DeckState {
    /// Half-beam per station (`length` entries) of the topmost open deck's edge — the
    /// **inset** structural outline that later levels stack on.
    pub top_outline: Vec<i32>,
    /// Half-beam per station of the topmost deck's **outer rim** (the outermost solid
    /// cell at `top_y`, including the outer bevel — wider than `top_outline` where the
    /// wall bevels in). The railing sits on this so it caps the real edge.
    pub rail_outline: Vec<i32>,
    /// Local Y of the topmost open deck floor.
    pub top_y: i32,
    /// The railing built around the top weather deck (`None` until `MainRailing`).
    pub railing: Option<railing::RailingModel>,
    /// The bowsprit off the bow (`None` until `Bowsprit`).
    pub bowsprit: Option<bowsprit::BowspritModel>,
    /// The masts (`None` until `Masts`).
    pub masts: Option<masts::MastModel>,
    /// Gun-port cells `(cell, outward dir)` planned by the additional deck, kept so they
    /// can be **re-stamped after the bowsprit** (whose solid prow buries the bow ones).
    pub gun_ports: Vec<(Point3D, ShipDir)>,
    /// Whether the gun ports are trapdoor lids (`true`) or open holes (`false`).
    pub gun_ports_trapdoors: bool,
}

impl DeckState {
    /// Start at the main deck (before any structural additions raise it).
    pub fn initial(hull: &HullModel, deck: &DeckModel) -> Self {
        Self {
            top_outline: hull.top_half.clone(),
            rail_outline: hull.top_half.clone(),
            top_y: deck.deck_y,
            railing: None,
            bowsprit: None,
            masts: None,
            gun_ports: Vec::new(),
            gun_ports_trapdoors: false,
        }
    }
}

/// Ship size tier, derived from tip-to-tip length. Drives which deck additions
/// (and how many masts) a ship gets. Thresholds are tunable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SizeTier {
    Small,
    Medium,
    Large,
    Huge,
}

impl SizeTier {
    /// Map a tip-to-tip length to a tier.
    pub fn from_length(length: i32) -> SizeTier {
        match length {
            ..=20 => SizeTier::Small,
            21..=30 => SizeTier::Medium,
            31..=40 => SizeTier::Large,
            _ => SizeTier::Huge,
        }
    }

    /// Number of masts this tier carries.
    pub fn mast_count(self) -> i32 {
        match self {
            SizeTier::Small => 1,
            SizeTier::Medium => 2,
            SizeTier::Large => 3,
            SizeTier::Huge => 3,
        }
    }

    /// Number of **additional decks** (above the main deck) — extra raised levels
    /// with windows / gun ports on the sides.
    pub fn extra_decks(self) -> i32 {
        match self {
            SizeTier::Small => 0,
            SizeTier::Medium => 1,
            SizeTier::Large => 1,
            SizeTier::Huge => 2,
        }
    }

    /// Whether this tier includes a given deck addition (the gating table).
    pub fn has(self, addition: DeckAddition) -> bool {
        use DeckAddition::*;
        match addition {
            // Required for every ship (above the minimum size).
            MainRailing | Masts => true,
            // Restricted from the smallest ships.
            Bowsprit | AdditionalDeck | CargoHatch | HelmCapstan => self >= SizeTier::Medium,
            // Only the larger ships.
            Forecastle | Gallery | Cabin => self >= SizeTier::Large,
        }
    }
}

/// The catalog of deck additions. Required (for ships big enough): `MainRailing`,
/// `Masts` (+ sails), `Bowsprit`. The rest are optional extras.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckAddition {
    /// Railing around the deck perimeter. (Required, all sizes.)
    MainRailing,
    /// Masts + sails. (Required; count scales with size.)
    Masts,
    /// Bowsprit off the bow. (Required for Medium+.)
    Bowsprit,
    /// Additional raised deck level(s) with windows / gun ports on the sides.
    AdditionalDeck,
    /// A square/rectangular raised structure at the **bow** (forecastle).
    Forecastle,
    /// A square/rectangular raised structure at the **stern** (gallery / cabin box).
    Gallery,
    /// An enclosed cabin / deckhouse on deck.
    Cabin,
    /// Ship's wheel (helm) + capstan.
    HelmCapstan,
    /// Cargo hatch(es) opening to the hold, with stairs/ladder down.
    CargoHatch,
}

/// How the sails are rendered. `None` is bare yards; `Furled` is rolled-up canvas
/// (alternating quartz stairs along each yard); `Full` is set, billowing square sails
/// (a curved white-wool sheet hung from each yard, depth driven by [`DeckContext::wind`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SailState {
    None,
    Furled,
    Full,
}

/// The billow *shape* of a deployed (`Full`) square sail — two looks to choose between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SailBillow {
    /// Attempt 1 — a pillow/dome: the curve runs **across the width** (deepest at the
    /// centre, pinned at the luff edges), so the vertical sides stay straight at the yard.
    Domed,
    /// Attempt 2 — a curtain/scoop: each horizontal row is flat (all blocks at one `x`), so
    /// the curve runs **down the whole length** and the **sides curve too**. The vertical
    /// profile bulges more drastically near the head/foot and flattens through the middle;
    /// larger sails curve slightly deeper.
    Curtain,
    /// Attempt 3 — a blend of the two: a domed `sin`×parabola belly (deepest at the centre)
    /// **but the luff sides are not pinned flat** — they billow partway (curtain-like) instead
    /// of back to the yard. Fuller and rounder than `Domed`, more centre-weighted than
    /// `Curtain`. The blend is set by `SAIL_COMBINED_EDGE`.
    Combined,
}

impl SailBillow {
    /// Roll a billow shape for a ship, weighted by [`SAIL_BILLOW_COMBINED_CHANCE`] /
    /// [`SAIL_BILLOW_CURTAIN_CHANCE`] (the remainder is `Domed`).
    pub fn pick(rng: &mut crate::noise::RNG) -> SailBillow {
        let r = rng.rand_i32_range(0, 100);
        if r < super::tuning::SAIL_BILLOW_COMBINED_CHANCE {
            SailBillow::Combined
        } else if r
            < super::tuning::SAIL_BILLOW_COMBINED_CHANCE + super::tuning::SAIL_BILLOW_CURTAIN_CHANCE
        {
            SailBillow::Curtain
        } else {
            SailBillow::Domed
        }
    }
}

/// Read-only context every deck addition builds against: the ship-so-far (placement
/// + hull/deck geometry), the ship palette, the size tier, and the footing. Mutable
/// access (editor/rng/data/base palette) comes via the [`ShipV2Ctx`] passed
/// alongside.
pub struct DeckContext<'a> {
    pub placement: &'a Placement,
    pub keel: &'a KeelModel,
    pub hull: &'a HullModel,
    pub deck: &'a DeckModel,
    pub ship_palette: &'a ShipPalette,
    pub tier: SizeTier,
    pub on_water: bool,
    /// Forward mast rake (blocks of `+x` per block of height; `0.0` = vertical).
    pub mast_lean: f32,
    /// How sails are rendered (none / furled / full).
    pub sail_state: SailState,
    /// Wind strength — deepest billow (blocks) of a deployed [`SailState::Full`] sail.
    pub wind: f32,
}

/// **Debug toggle** — additions to skip building, for isolating issues. Normally empty;
/// e.g. set to `&[DeckAddition::Bowsprit, DeckAddition::MainRailing]` to build a ship
/// without the bowsprit or railing. The build pipeline skips anything listed here.
pub const DEBUG_SKIP: &[DeckAddition] = &[];

/// Order in which additions are built (structure first, then rig, then fittings).
/// Tunable; the pipeline iterates this.
pub const BUILD_ORDER: [DeckAddition; 9] = [
    DeckAddition::AdditionalDeck,
    DeckAddition::Forecastle,
    DeckAddition::Gallery,
    DeckAddition::Cabin,
    // Bowsprit before the railing: it extends the weather-deck outline forward over the
    // prow (`DeckState::top_outline`), and the shared main railing then wraps that too.
    DeckAddition::Bowsprit,
    DeckAddition::MainRailing,
    DeckAddition::CargoHatch,
    DeckAddition::Masts,
    DeckAddition::HelmCapstan,
];

/// Dispatch a single addition to its submodule. Add a match arm here (and the
/// `pub mod`) as each addition is implemented; unimplemented ones are no-ops.
///
/// `state` carries the running top-weather-deck info between additions: structural
/// additions (the additional deck) raise it; fittings (the railing) read it.
#[allow(unused_variables)]
pub async fn build_addition(
    addition: DeckAddition,
    ctx: &mut ShipV2Ctx<'_>,
    dc: &DeckContext<'_>,
    state: &mut DeckState,
) {
    match addition {
        DeckAddition::AdditionalDeck => additional_deck::build(ctx, dc, state).await,
        DeckAddition::MainRailing => railing::build(ctx, dc, state).await,
        DeckAddition::Bowsprit => bowsprit::build(ctx, dc, state).await,
        DeckAddition::Masts => masts::build(ctx, dc, state).await,
        _ => {}
    }
}

/// Gun ports cut into the **prow's side surface** at the gun-deck row — the outermost
/// prow cell per station, spaced by [`GUN_PORT_STEP`]. The prow flares a block or two
/// past the deck wall, so these add windows in the bow (often forming a through-window
/// with the re-stamped inner deck port).
fn prow_side_ports(prow: &[Point3D], gun_row: i32) -> Vec<(Point3D, ShipDir)> {
    // Outermost |z| of the solid prow per station, at the gun-deck row.
    let mut outer: HashMap<i32, i32> = HashMap::new();
    for c in prow {
        if c.y == gun_row && c.z != 0 {
            let e = outer.entry(c.x).or_insert(0);
            *e = (*e).max(c.z.abs());
        }
    }
    let mut xs: Vec<i32> = outer.keys().copied().collect();
    xs.sort_unstable();
    let mut ports = Vec::new();
    if let (Some(&minx), Some(&maxx)) = (xs.first(), xs.last()) {
        let mut x = minx;
        while x <= maxx {
            if let Some(&oz) = outer.get(&x) {
                if oz >= 2 {
                    ports.push((Point3D::new(x, gun_row, oz), ShipDir::Starboard));
                    ports.push((Point3D::new(x, gun_row, -oz), ShipDir::Port));
                }
            }
            x += GUN_PORT_STEP;
        }
    }
    ports
}

/// Finish the gun ports **after** the bowsprit: re-stamp the additional deck's ports (the
/// solid prow buried the bow ones) **and** cut new ports into the prow's side surface.
/// Trapdoor lids are re-placed; open holes are forced back to air. Forced placement is
/// needed to punch through the prow's solid blocks.
pub async fn restamp_gun_ports(
    ctx: &mut ShipV2Ctx<'_>,
    place: &Placement,
    ship_palette: &ShipPalette,
    state: &DeckState,
) {
    if state.gun_ports.is_empty() {
        return;
    }
    // The deck's planned ports + new ports on the prow sides at the same gun-deck row.
    let mut ports = state.gun_ports.clone();
    let gun_row = state.gun_ports[0].0.y;
    if let Some(bowsprit) = &state.bowsprit {
        ports.extend(prow_side_ports(&bowsprit.prow, gun_row));
    }

    if state.gun_ports_trapdoors {
        let material = ctx
            .palette
            .get_material(ship_palette.role(ShipPart::Topside))
            .expect("Topside role missing from base palette")
            .clone();
        let mut placer_rng = ctx.rng.derive();
        let mut placer = MaterialPlacer::new(Placer::new(&ctx.data.materials, &mut placer_rng), material);
        for (cell, dir) in &ports {
            let st = HashMap::from([
                ("facing".to_string(), place.world_cardinal(*dir).to_string()),
                ("half".to_string(), "bottom".to_string()),
                ("open".to_string(), "true".to_string()),
            ]);
            placer
                .place_block_forced(ctx.editor, place.to_world(*cell), BlockForm::Trapdoor, Some(&st), None)
                .await;
        }
    } else {
        let air = crate::minecraft::Block::from("minecraft:air");
        for (cell, _) in &ports {
            ctx.editor.place_block_forced(&air, place.to_world(*cell)).await;
        }
    }
}
