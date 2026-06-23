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

use super::deck::DeckModel;
use super::hull::HullModel;
use super::keel::KeelModel;
use super::palette::ShipPalette;
use super::{Placement, ShipV2Ctx};

pub mod additional_deck;
pub mod bowsprit;
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
        _ => {}
    }
}
