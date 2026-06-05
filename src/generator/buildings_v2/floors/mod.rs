//! Floors: floor slabs, ceilings, stairs, and custom room floors.
//!
//! - [`plan`] — `Stairwell`/`FloorPlan` data types.
//! - [`place`] — floor/ceiling/stair placement and room-floor painting.
//! - `stairs` — stairwell selection and stair-block geometry.

mod place;
mod plan;
mod stairs;

pub use place::{clear_attic_stair_headroom, place_floors, place_room_floors};
pub use plan::{FloorPlan, StairKind, Stairwell};
