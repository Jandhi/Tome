//! Furnish: places furniture into furnished rooms using the loaded furniture
//! data, respecting wall slots, connectivity, and attic roof clearance.
//!
//! - [`types`] — placement enums (`CellConstraint`, `FacingMode`, `BlockLayer`).
//! - [`block`] — block-string parsing, rotation, facing, palette swaps.
//! - [`loot`] — container loot-table rolling into SNBT.
//! - [`placement`] — wall-slot/freestanding/ceiling placement + connectivity.
//! - [`room`] — the per-room furnishing driver (`furnish_rooms`).
//! - [`data`] — JSON/YAML furniture definitions.

#[cfg(test)]
mod test;
pub mod data;

mod block;
mod loot;
mod placement;
mod room;
mod roof;
mod types;

pub use roof::decorate_rooftops;
pub use room::furnish_rooms;
pub(crate) use room::{furnish_interior, harvest_anchors};
pub(crate) use placement::RoofClearance;
pub use types::{BlockLayer, CellConstraint, FacingMode};
