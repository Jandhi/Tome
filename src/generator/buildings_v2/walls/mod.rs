//! Walls: turns a `Frame` into wall segments, then fills, frames, and pierces
//! them. The pipeline runs in two waves around the floors pass — first
//! `build_segments` + `place_doors`, then `place_wall_infill` + `place_frame` +
//! `place_openings` (see the buildings_v2 module docs for ordering).
//!
//! - [`segments`] — segment data types and the frame→segment geometry.
//! - [`openings`] — door/window planning and rendering.
//! - [`infill`] — panel block fill.
//! - [`timber`] — corner posts, crossbeams, and decorative timber.

#[cfg(test)]
mod test;

mod infill;
mod openings;
mod segments;
mod timber;

pub use infill::{WallInfill, place_wall_infill};
pub use openings::{
    WindowFill, boundary_cell_set, place_doors, place_openings, place_terrace_doors, place_windows,
};
pub use segments::{
    DoorStyle, Opening, OpeningKind, WallSegment, WallSegments, WindowStyle, build_segments,
    segment_cells,
};
pub use timber::{TimberPattern, place_frame};
