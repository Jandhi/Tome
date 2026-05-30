
#[cfg(test)]
mod test;

use crate::geometry::Point2D;
use crate::noise::RNG;
use super::footprint::{Footprint, SizeClass};
use super::footprint::merge::outline_from_rects;

/// 3D skeleton of a building: footprint + per-rect floor counts + uniform wall height.
pub struct Frame {
    footprint: Footprint,
    base_y: i32,
    /// Floor count per rect, parallel to footprint.rects(). Core is index 0.
    floor_counts: Vec<u32>,
    /// Interior wall height in blocks of air, uniform across all floors and rects.
    wall_height: u32,
    /// Pre-computed active rect indices per floor.
    active_rects_cache: Vec<Vec<usize>>,
}

/// Sentinel floor index for a below-ground cellar. Cellars live one story
/// below `base_y` and outside the normal `0..max_floors` range, so they reuse
/// the floor-indexed APIs (`floor_y`, `ceiling_y`) via this special value
/// rather than extending floor indices to signed. Mirrors how attics reuse the
/// index space by sitting *above* the top floor.
pub const CELLAR_FLOOR: u32 = u32::MAX;

impl Frame {
    pub fn new(footprint: Footprint, base_y: i32, floor_counts: Vec<u32>, wall_height: u32) -> Self {
        debug_assert_eq!(
            floor_counts.len(),
            footprint.rects().len(),
            "floor_counts length ({}) must match footprint rects ({})",
            floor_counts.len(),
            footprint.rects().len(),
        );
        debug_assert!(
            floor_counts.iter().all(|&c| c >= 1),
            "all floor counts must be >= 1",
        );
        let max = *floor_counts.iter().max().unwrap_or(&0);
        let active_rects_cache = (0..max)
            .map(|floor| {
                floor_counts.iter().enumerate()
                    .filter(|(_, &count)| floor < count)
                    .map(|(i, _)| i)
                    .collect()
            })
            .collect();
        Self { footprint, base_y, floor_counts, wall_height, active_rects_cache }
    }

    pub fn footprint(&self) -> &Footprint {
        &self.footprint
    }

    pub fn base_y(&self) -> i32 {
        self.base_y
    }

    pub fn wall_height(&self) -> u32 {
        self.wall_height
    }

    pub fn floor_counts(&self) -> &[u32] {
        &self.floor_counts
    }

    /// Max floor count across all rects (the core's count).
    pub fn max_floors(&self) -> u32 {
        self.floor_counts[0]
    }

    /// Height in blocks for a given rect.
    pub fn rect_height(&self, rect_index: usize) -> u32 {
        self.floor_counts[rect_index] * (self.wall_height + 1)
    }

    /// Y level of the floor surface for a given story (0-indexed).
    /// `CELLAR_FLOOR` resolves to one story below `base_y`.
    pub fn floor_y(&self, floor: u32) -> i32 {
        if floor == CELLAR_FLOOR {
            return self.base_y - (self.wall_height as i32 + 1);
        }
        self.base_y + floor as i32 * (self.wall_height as i32 + 1)
    }

    /// Y level of the ceiling for a given story.
    pub fn ceiling_y(&self, floor: u32) -> i32 {
        self.floor_y(floor) + self.wall_height as i32
    }

    /// Y level where the roof starts for a given rect (one above top wall).
    pub fn roof_y(&self, rect_index: usize) -> i32 {
        self.base_y + self.rect_height(rect_index) as i32 + 1
    }

    /// Which rects are active (have floors) at a given story.
    /// Returns indices into footprint.rects().
    pub fn active_rects(&self, floor: u32) -> &[usize] {
        self.active_rects_cache.get(floor as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// The 2D points that have a floor at a given story.
    /// Union of all active rects' filled points.
    pub fn filled_points_at_floor(&self, floor: u32) -> Vec<Point2D> {
        let rects = self.footprint.rects();
        let mut points: Vec<Point2D> = self
            .active_rects(floor)
            .iter()
            .flat_map(|&i| rects[i].iter())
            .collect();
        points.sort_by_key(|p| (p.x, p.y));
        points.dedup();
        points
    }

    /// Clockwise outline polygon for the active rects at a given floor.
    /// On the ground floor this matches the full footprint outline.
    /// On upper floors where wings drop out, the outline shrinks.
    pub fn outline_at_floor(&self, floor: u32) -> Vec<Point2D> {
        let all_rects = self.footprint.rects();
        let active: Vec<_> = self.active_rects(floor)
            .iter()
            .map(|&i| all_rects[i])
            .collect();
        if active.is_empty() { return Vec::new(); }
        outline_from_rects(&active)
    }

    /// All floor indices (0 to max_floors).
    pub fn floors(&self) -> impl Iterator<Item = u32> {
        0..self.max_floors()
    }
}

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
