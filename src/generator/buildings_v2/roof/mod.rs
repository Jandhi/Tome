//! Roofs: dispatches to a gable, hipped, or flat roof builder for a building's frame.
//!
//! - [`gable_roof`] — pitched gable roofs (+ chimney, attic lantern).
//! - [`hipped_roof`] — four-sided pyramidal roofs with upturned eave corners.
//! - [`irimoya_roof`] — hip-and-gable roofs: a hipped skirt closed by a central gable.
//! - [`flat_roof`] — slab decks with parapets (+ roof-access ladder).
//! - [`blocks`] / [`gable`] / [`hipped`] / [`irimoya`] / [`heightmap`] — shared roof geometry primitives.

#[cfg(test)]
mod test;

pub mod blocks;
pub mod dome;
pub mod gable;
pub mod heightmap;
pub mod hipped;
pub mod irimoya;

mod flat_roof;
mod gable_roof;
mod hipped_roof;
mod irimoya_roof;

use crate::geometry::{Point2D, Rect2D};

use super::frame::Frame;
use super::pipeline::BuildCtx;
use super::HouseSeed;
use gable::GablePitch;
use heightmap::RoofHeightmap;
use hipped::HippedPitch;

pub use flat_roof::place_roof_ladder;

/// Top-level roof style. Determines which roof algorithm runs.
/// Serialised (house spec) as `flat`, `irimoya`, or a one-key map with the
/// pitch: `{ gable: stairs }`, `{ hipped: slab }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoofStyle {
    Gable(GablePitch),
    Hipped(HippedPitch),
    /// Hip-and-gable (irimoya): a hipped skirt closed by a central long-axis gable.
    Irimoya,
    Flat,
}

/// Per-rect extents at each rect's top floor. The roof sits on the topmost
/// extent of each rect — for jettied buildings this is the grown upper rect,
/// for un-jettied buildings it equals `footprint().rects()[i]`. Caller indexes
/// the result by rect index (parallel to `footprint().rects()`).
pub(super) fn top_floor_rects(frame: &Frame) -> Vec<Rect2D> {
    (0..frame.rect_count())
        .map(|i| frame.rect_at_top(i).expect("rect must exist at its top floor"))
        .collect()
}

/// Place roofs on all rects of a building.
///
/// Returns gable doorways and per-rect heightmaps. Heightmap `i` is the gable
/// heightmap of `rects[i]` using the extended-roof bounds, suitable for asking
/// "what's the roof block y at this (x, z)?" for furnish-time clearance checks
/// inside attics. For flat roofs the heightmaps are trivial (height 0 everywhere).
///
/// `ctx.rng` only varies the placed blocks; the decisions draw from the house's
/// `roof.ridge`, `roof.chimney` and `roof.parapet` streams of `seed`.
pub async fn place_roof(
    ctx: &mut BuildCtx<'_>,
    seed: HouseSeed,
    frame: &Frame,
    style: RoofStyle,
) -> (Vec<Point2D>, Vec<RoofHeightmap>) {
    match style {
        RoofStyle::Gable(pitch) => gable_roof::place_gable_roof(ctx, seed, frame, pitch).await,
        RoofStyle::Hipped(pitch) => hipped_roof::place_hipped_roof(ctx, frame, pitch).await,
        RoofStyle::Irimoya => irimoya_roof::place_irimoya_roof(ctx, frame).await,
        RoofStyle::Flat => flat_roof::place_flat_roof(ctx, seed, frame).await,
    }
}
