//! Frame: a building's 3D skeleton and the transforms that produce it.
//!
//! - [`model`] — the `Frame` type and its geometric queries.
//! - [`generate`] — derive a frame from a footprint + size class.
//! - [`jetty`] — grow upper floors outward over the ground floor.

#[cfg(test)]
mod test;

mod generate;
mod jetty;
mod model;

pub use generate::{generate_frame, generate_frame_pinned};
pub use jetty::apply_jetty;
pub use model::{CELLAR_FLOOR, Frame};
