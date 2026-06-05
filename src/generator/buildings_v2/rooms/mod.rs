//! Rooms: partition a `Frame` into furnishable rooms, place interior walls,
//! assign room types/roles, wire up attic access, and check the structural
//! invariants the rest of the pipeline relies on.
//!
//! - [`plan`] — `Room`/`RoomPlan` types and interior geometry.
//! - [`assign`] — room type/role assignment.
//! - [`build`] — partitioning + interior wall placement (`build_rooms`).
//! - [`attic`] — ladder access for unreachable attics.
//! - [`annotate`] — gable doorway / window constraint marking.
//! - [`invariants`] — end-of-pipeline structural checks.
//! - [`constraints`] — per-cell `CellState` map used for furniture placement.

#[cfg(test)]
mod test;
pub mod constraints;

mod annotate;
mod assign;
mod attic;
mod build;
mod invariants;
mod plan;

pub use constraints::{CellState, ConstraintMap, PlacedFurniture};
pub use annotate::{mark_gable_doorways, mark_windows};
pub use assign::{assign_attic_types, assign_room_floors, assign_types_to_rooms};
pub use attic::place_attic_ladders;
pub use build::build_rooms;
pub use invariants::check_building_invariants;
pub use plan::{Room, RoomPlan, RoomRole, compute_room_interior};
