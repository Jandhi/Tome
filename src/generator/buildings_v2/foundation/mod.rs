//! Foundation: terrain analysis, base-Y selection, cut/fill, and the stone
//! course a building sits on.
//!
//! - [`course`] — the full foundation pipeline (`place_foundation`).
//! - [`terraform`] — blends the surrounding terrain into the new base.

pub mod terraform;
#[cfg(test)]
mod test;

mod course;

pub use course::{analyze_terrain, place_foundation, TerrainProfile};
