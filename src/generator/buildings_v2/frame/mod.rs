
#[cfg(test)]
mod test;

use crate::geometry::{Point2D, Rect2D};
use crate::noise::RNG;
use super::footprint::{Footprint, SizeClass};
use super::footprint::merge::outline_from_rects;

/// 3D skeleton of a building: footprint + per-rect floor counts + uniform wall height.
///
/// `footprint` carries the logical rect identity (core = rects[0], wings = 1..N)
/// and the *ground-floor* geometry. `rect_extents` carries the per-floor extent
/// of each rect — for plain (un-jettied) buildings every floor uses the same
/// extent as the ground rect; for jettied buildings upper floors store grown
/// extents. Floor presence is encoded as `Option<Rect2D>`: `None` means the rect
/// has no walls/roof/floor on that level (e.g. a wing under the eaves of a
/// taller core).
pub struct Frame {
    footprint: Footprint,
    base_y: i32,
    /// Floor count per rect, parallel to footprint.rects(). Core is index 0.
    floor_counts: Vec<u32>,
    /// Interior wall height in blocks of air, uniform across all floors and rects.
    wall_height: u32,
    /// Per-rect per-floor geometric extents. Indexed `[rect_index][floor]`.
    /// `Some(r)` means rect `i` occupies extent `r` on that floor; `None` means
    /// the rect isn't present at that level. Used by all geometric queries
    /// (`outline_at_floor`, `filled_points_at_floor`, `rect_at`). For un-jettied
    /// buildings the extents at every Some-floor equal `footprint.rects()[i]`.
    rect_extents: Vec<Vec<Option<Rect2D>>>,
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
        let rect_extents: Vec<Vec<Option<Rect2D>>> = floor_counts.iter().enumerate()
            .map(|(i, &count)| {
                let rect = footprint.rects()[i];
                (0..max)
                    .map(|f| if f < count { Some(rect) } else { None })
                    .collect()
            })
            .collect();
        let active_rects_cache = (0..max as usize)
            .map(|floor| {
                rect_extents.iter().enumerate()
                    .filter(|(_, exts)| exts[floor].is_some())
                    .map(|(i, _)| i)
                    .collect()
            })
            .collect();
        Self { footprint, base_y, floor_counts, wall_height, rect_extents, active_rects_cache }
    }

    /// Construct a Frame with explicit per-rect per-floor extents. Used when the
    /// extent at floor `f` differs from the ground rect (jettied upper floors).
    /// `rect_extents[i]` must have length `max_floors`; entries before the
    /// rect's top must be `Some(_)`, entries after must be `None`. Floor counts
    /// are derived from the position of the first `None` per rect.
    pub fn with_per_floor_extents(
        footprint: Footprint,
        base_y: i32,
        rect_extents: Vec<Vec<Option<Rect2D>>>,
        wall_height: u32,
    ) -> Self {
        debug_assert_eq!(
            rect_extents.len(),
            footprint.rects().len(),
            "rect_extents length ({}) must match footprint rects ({})",
            rect_extents.len(),
            footprint.rects().len(),
        );
        let max = rect_extents.iter().map(|exts| exts.len()).max().unwrap_or(0);
        debug_assert!(
            rect_extents.iter().all(|exts| exts.len() == max),
            "all rect_extents must have the same per-floor length (= max_floors)",
        );
        let floor_counts: Vec<u32> = rect_extents.iter()
            .map(|exts| exts.iter().take_while(|e| e.is_some()).count() as u32)
            .collect();
        debug_assert!(
            floor_counts.iter().all(|&c| c >= 1),
            "every rect must be present on at least floor 0",
        );
        debug_assert!(
            rect_extents.iter().zip(&floor_counts).all(|(exts, &count)| {
                exts.iter().skip(count as usize).all(|e| e.is_none())
            }),
            "rect floors must be contiguous from 0; no Some after the first None",
        );
        let active_rects_cache: Vec<Vec<usize>> = (0..max)
            .map(|floor| {
                rect_extents.iter().enumerate()
                    .filter(|(_, exts)| exts[floor].is_some())
                    .map(|(i, _)| i)
                    .collect()
            })
            .collect();
        Self { footprint, base_y, floor_counts, wall_height, rect_extents, active_rects_cache }
    }

    /// Geometric extent of rect `rect_index` at the given floor, or `None` if
    /// the rect has no presence on that floor. For un-jettied buildings this
    /// equals `footprint().rects()[rect_index]` whenever it returns `Some`.
    /// For jettied buildings upper floors return a grown extent.
    pub fn rect_at(&self, rect_index: usize, floor: u32) -> Option<Rect2D> {
        self.rect_extents.get(rect_index)?.get(floor as usize).copied().flatten()
    }

    /// Geometric extent of rect `rect_index` at its top regular floor — the
    /// floor where the roof, ceiling, and attic (if any) sit. For jettied
    /// buildings this is the grown extent.
    pub fn rect_at_top(&self, rect_index: usize) -> Option<Rect2D> {
        let top_floor = self.floor_counts.get(rect_index)?.checked_sub(1)?;
        self.rect_at(rect_index, top_floor)
    }

    /// Number of distinct logical rects (core + wings). Independent of per-floor
    /// extents — a rect that's absent on some floors is still counted.
    pub fn rect_count(&self) -> usize {
        self.rect_extents.len()
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
    /// Union of all active rects' filled points, using per-floor extents.
    pub fn filled_points_at_floor(&self, floor: u32) -> Vec<Point2D> {
        let mut points: Vec<Point2D> = self
            .active_rects(floor)
            .iter()
            .filter_map(|&i| self.rect_at(i, floor))
            .flat_map(|r| r.iter())
            .collect();
        points.sort_by_key(|p| (p.x, p.y));
        points.dedup();
        points
    }

    /// Clockwise outline polygon for the active rects at a given floor.
    /// On the ground floor this matches the full footprint outline.
    /// On upper floors where wings drop out (or where jetty grows the extent),
    /// the outline shrinks or expands accordingly.
    pub fn outline_at_floor(&self, floor: u32) -> Vec<Point2D> {
        let active: Vec<Rect2D> = self.active_rects(floor)
            .iter()
            .filter_map(|&i| self.rect_at(i, floor))
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

/// Eligibility gate + transform: returns a new Frame with upper floors grown
/// by 1 on each side when the building is eligible for jettying. Falls back to
/// the input frame unchanged when the shape, floor count, or plot bounds rule
/// it out. Phase 2 supports single-rect buildings only — multi-rect compensation
/// arrives in Phase 3.
pub fn apply_jetty(frame: Frame, plot_bounds: &Rect2D) -> Frame {
    if frame.rect_count() != 1 { return frame; }
    if frame.max_floors() < 2 { return frame; }

    let Some(ground) = frame.rect_at(0, 0) else { return frame; };
    let grown = Rect2D::from_points(
        Point2D::new(ground.min().x - 1, ground.min().y - 1),
        Point2D::new(ground.max().x + 1, ground.max().y + 1),
    );

    if !plot_bounds.contains_rect(&grown) {
        return frame;
    }

    let max_floors = frame.max_floors();
    let extents: Vec<Vec<Option<Rect2D>>> = vec![
        (0..max_floors)
            .map(|f| Some(if f == 0 { ground } else { grown }))
            .collect()
    ];

    Frame::with_per_floor_extents(
        frame.footprint().clone(),
        frame.base_y(),
        extents,
        frame.wall_height(),
    )
}
