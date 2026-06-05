//! Room data types and the per-room interior geometry. `Room`/`RoomPlan` are
//! the structs every later pass operates on; `compute_room_interior` derives
//! the furnishable rect from a footprint rect's wall ownership.

use crate::geometry::{Point2D, Rect2D};

use super::super::{FloorType, RoomType};
use super::constraints::{ConstraintMap, PlacedFurniture};

/// Role of a room within a building, assigned during partitioning.
/// Combined with BuildingType later to determine furniture/decoration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomRole {
    /// Contains an exterior door.
    Entry,
    /// Largest non-entry room on ground floor.
    Main,
    /// Remaining ground-floor rooms.
    Secondary,
    /// Any upper-floor room.
    Upper,
    /// Attic room under a double-pitch roof.
    Attic,
    /// Below-ground cellar under the core rect.
    Cellar,
}

/// A room within a building.
#[derive(Debug, Clone)]
pub struct Room {
    /// The footprint rect this room corresponds to.
    pub rect: Rect2D,
    /// Index into footprint.rects().
    pub rect_index: usize,
    /// Floor level (0 = ground).
    pub floor: u32,
    /// Assigned role.
    pub role: RoomRole,
    /// What furniture/decoration this room gets.
    pub room_type: RoomType,
    /// Furnishable interior rect. Each side is shrunk by 1 only if there's a
    /// wall in this rect's own space on that side (exterior wall, or shared
    /// boundary where this rect is the lower-indexed one — see
    /// `compute_room_interior`). Sides where the wall lives inside a neighbor
    /// rect keep the full rect extent, so wall slots sit flush against walls.
    pub interior: Rect2D,
    /// Constraint map for furniture placement and connectivity checks.
    pub constraints: ConstraintMap,
    /// Furniture placed in this room (populated by furnish_rooms).
    pub furniture: Vec<PlacedFurniture>,
    pub floor_type: Option<FloorType>,
}

/// Compute the interior rect for a room. A side is shrunk by 1 iff this rect
/// has a wall in its own space on that side:
/// - Exterior sides (no adjacent rect) → wall at the perimeter → shrink.
/// - Shared boundary where this rect has the lower index → `find_boundaries`
///   puts the wall on this rect's inside edge → shrink.
/// - Shared boundary where this rect has the higher index → wall lives in the
///   neighbor's rect → don't shrink; the full edge is interior.
///
/// Mixed sides (partly exterior, partly shared with a higher-indexed neighbor,
/// etc.) fall back to shrinking — conservative but safe.
pub fn compute_room_interior(rects: &[Rect2D], rect_idx: usize) -> Rect2D {
    let rect = rects[rect_idx];
    let rmin = rect.min();
    let rmax = rect.max();

    // A cell is "covered by a lower-indexed rect" if some rect with a smaller
    // index contains it. If every cell along a side's adjacency row is covered
    // this way, `find_boundaries` placed the shared wall inside those
    // neighbor rects, so this rect's own edge has no wall and shouldn't shrink.
    let covered_by_lower = |p: Point2D| -> bool {
        rects.iter().take(rect_idx).any(|other| other.contains(p))
    };

    let north_shared = (rmin.x..=rmax.x)
        .all(|x| covered_by_lower(Point2D::new(x, rmin.y - 1)));
    let south_shared = (rmin.x..=rmax.x)
        .all(|x| covered_by_lower(Point2D::new(x, rmax.y + 1)));
    let west_shared = (rmin.y..=rmax.y)
        .all(|y| covered_by_lower(Point2D::new(rmin.x - 1, y)));
    let east_shared = (rmin.y..=rmax.y)
        .all(|y| covered_by_lower(Point2D::new(rmax.x + 1, y)));

    let shrink_w = if west_shared { 0 } else { 1 };
    let shrink_n = if north_shared { 0 } else { 1 };
    let shrink_e = if east_shared { 0 } else { 1 };
    let shrink_s = if south_shared { 0 } else { 1 };

    // Match `Rect2D::shrink` semantics for degenerate cases: size may go to
    // zero or negative, and callers check `size.x > 0 && size.y > 0`.
    Rect2D {
        origin: Point2D {
            x: rmin.x + shrink_w,
            y: rmin.y + shrink_n,
        },
        size: Point2D {
            x: rect.size.x - shrink_w - shrink_e,
            y: rect.size.y - shrink_n - shrink_s,
        },
    }
}

/// Result of room partitioning, consumed by the interior/furniture module.
pub struct RoomPlan {
    pub rooms: Vec<Room>,
    /// Interior door positions: (floor, rect_a, rect_b, cell position).
    pub interior_doors: Vec<(u32, usize, usize, Point2D)>,
}

impl RoomPlan {
    pub fn rooms_on_floor(&self, floor: u32) -> Vec<&Room> {
        self.rooms.iter().filter(|r| r.floor == floor).collect()
    }
}

/// Clamp a point to the nearest cell inside a (non-empty) interior rect.
/// Used to find the entrance cell for a room given a door/interior-door position
/// that may be on the wall edge rather than inside the shrunk interior.
pub(super) fn nearest_interior_cell(point: Point2D, interior: &Rect2D) -> Point2D {
    Point2D::new(
        point.x.clamp(interior.min().x, interior.max().x),
        point.y.clamp(interior.min().y, interior.max().y),
    )
}
