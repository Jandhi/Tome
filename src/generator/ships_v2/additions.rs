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
use super::palette::ShipPalette;
use super::{Placement, ShipV2Ctx};

pub mod additional_deck;

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
    pub hull: &'a HullModel,
    pub deck: &'a DeckModel,
    pub ship_palette: &'a ShipPalette,
    pub tier: SizeTier,
    pub on_water: bool,
}

/// Order in which additions are built (structure first, then rig, then fittings).
/// Tunable; the pipeline iterates this.
pub const BUILD_ORDER: [DeckAddition; 9] = [
    DeckAddition::AdditionalDeck,
    DeckAddition::Forecastle,
    DeckAddition::Gallery,
    DeckAddition::Cabin,
    DeckAddition::MainRailing,
    DeckAddition::CargoHatch,
    DeckAddition::Masts,
    DeckAddition::Bowsprit,
    DeckAddition::HelmCapstan,
];

/// Dispatch a single addition to its submodule. Add a match arm here (and the
/// `pub mod`) as each addition is implemented; unimplemented ones are no-ops.
#[allow(unused_variables)]
pub async fn build_addition(addition: DeckAddition, ctx: &mut ShipV2Ctx<'_>, dc: &DeckContext<'_>) {
    match addition {
        DeckAddition::AdditionalDeck => additional_deck::build(ctx, dc).await,
        _ => {}
    }
}
