//! Palette seam for ship **components**. Each major component (keel, hull, …) is
//! one material, looked up through a [`ShipPart`] role rather than hardcoded, so it
//! stays swappable (see `docs/plans/ship-builder.md`, "palette-driven blocks").
//! Granularity is per component — the keel is a single part, not subdivided.

use std::collections::HashMap;

use crate::generator::materials::MaterialRole;

/// A major ship component whose material can be reassigned. One entry per
/// component (more added as stages land: `Hull`, `Rudder`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShipPart {
    /// The whole keel (post + flat run + both rakes) — one material.
    Keel,
    /// The hull shell — one material.
    Hull,
    /// The rudder (blade + fence attachment) — one material.
    Rudder,
    /// The deck floor — one material.
    Deck,
    /// Above-water topsides (additional deck walls) — one material.
    Topside,
    /// The main railing (bulwark + rail cap) around the top weather deck.
    Railing,
    /// Spars — the bowsprit (and later yards/booms). Drawn with slabs/stairs for a
    /// smooth taper, so this needs a plank wood (a log has no stair/slab variant).
    Spar,
    /// Masts — the vertical keel-stepped poles. A log (vertical axis).
    Mast,
}

/// Maps each [`ShipPart`] to a base-palette [`MaterialRole`]. Swap entries here to
/// re-skin a component without touching shape code.
#[derive(Debug, Clone)]
pub struct ShipPalette {
    roles: HashMap<ShipPart, MaterialRole>,
}

impl ShipPalette {
    pub fn new(roles: HashMap<ShipPart, MaterialRole>) -> Self {
        Self { roles }
    }

    /// Default mapping onto a wood ship palette (e.g. `ship_oak`).
    pub fn ship_oak_default() -> Self {
        Self::new(HashMap::from([
            (ShipPart::Keel, MaterialRole::PrimaryWood),
            (ShipPart::Hull, MaterialRole::PrimaryWood),
            (ShipPart::Rudder, MaterialRole::PrimaryWood),
            (ShipPart::Deck, MaterialRole::PrimaryWood),
            (ShipPart::Topside, MaterialRole::PrimaryWood),
            (ShipPart::Railing, MaterialRole::PrimaryWood),
            (ShipPart::Spar, MaterialRole::PrimaryWood),
            (ShipPart::Mast, MaterialRole::WoodPillar), // a log (vertical pole)
        ]))
    }

    /// The base-palette role a component draws its material from.
    pub fn role(&self, part: ShipPart) -> MaterialRole {
        self.roles.get(&part).copied().unwrap_or(MaterialRole::PrimaryWood)
    }
}
