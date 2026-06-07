//! Frame generation: derive per-rect floor counts from a footprint and size
//! class, with wings allowed to drop one story below the core.

use crate::noise::RNG;

use super::super::footprint::{Footprint, SizeClass};
use super::model::Frame;

/// Generate a frame from a footprint and size class.
/// Core (rects[0]) gets the full floor count; wings get the same or one fewer.
pub fn generate_frame(
    footprint: Footprint,
    base_y: i32,
    size_class: &SizeClass,
    rng: &mut RNG,
) -> Frame {
    let core_floors = rng.rand_i32_range(
        size_class.min_floors() as i32,
        size_class.max_floors() as i32 + 1,
    ) as u32;

    let mut floor_counts = vec![core_floors];

    for _ in 1..footprint.rects().len() {
        let wing_floors = if core_floors > 1 && rng.chance(1, 2) {
            core_floors - 1
        } else {
            core_floors
        };
        floor_counts.push(wing_floors);
    }

    Frame::new(footprint, base_y, floor_counts, 3)
}
