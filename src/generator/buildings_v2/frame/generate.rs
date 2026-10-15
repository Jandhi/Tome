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
    generate_frame_pinned(footprint, base_y, size_class, None, rng)
}

/// [`generate_frame`] with some floor counts pinned (a house spec's
/// `footprint[i].floors`; `None` entries are drawn). Every draw is still made
/// and a pinned count replaces it, so pinning a rect to the count it would
/// have drawn changes nothing. A pinned core count steers the wings' draws
/// (they take the core's count or one fewer).
pub fn generate_frame_pinned(
    footprint: Footprint,
    base_y: i32,
    size_class: &SizeClass,
    pinned: Option<&[Option<u32>]>,
    rng: &mut RNG,
) -> Frame {
    let pin = |i: usize| pinned.and_then(|p| p.get(i).copied().flatten());
    let drawn_core = rng.rand_i32_range(
        size_class.min_floors() as i32,
        size_class.max_floors() as i32 + 1,
    ) as u32;
    let core_floors = pin(0).unwrap_or(drawn_core);

    let mut floor_counts = vec![core_floors];

    for i in 1..footprint.rects().len() {
        let wing_floors = if core_floors > 1 && rng.chance(1, 2) {
            core_floors - 1
        } else {
            core_floors
        };
        floor_counts.push(pin(i).unwrap_or(wing_floors));
    }

    Frame::new(footprint, base_y, floor_counts, 3)
}
