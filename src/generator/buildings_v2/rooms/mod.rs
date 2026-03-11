#[cfg(test)]
mod test;

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Point2D, Point3D, Rect2D};
use crate::minecraft::BlockForm;
use crate::noise::RNG;
use std::collections::HashSet;
use super::footprint::merge::{walk_edge_cells, concave_corner_cells};
use super::frame::Frame;
use super::floors::FloorPlan;
use super::walls::{self, WallSegments};

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
}

/// Result of room partitioning, consumed by the interior/furniture module.
pub struct RoomPlan {
    pub rooms: Vec<Room>,
}

impl RoomPlan {
    pub fn rooms_on_floor(&self, floor: u32) -> Vec<&Room> {
        self.rooms.iter().filter(|r| r.floor == floor).collect()
    }
}

/// A boundary between two adjacent rects where an interior wall goes.
pub struct RectBoundary {
    pub rect_a: usize,
    pub rect_b: usize,
    /// Cell positions where wall blocks are placed.
    pub wall_cells: Vec<Point2D>,
}

/// Find pairs of adjacent rects and compute the cells for each shared boundary wall.
/// The wall is placed on the inside edge of the core rect (index 0) so that
/// wings keep their full interior space. For wing-to-wing boundaries, the wall
/// goes on the lower-indexed rect's edge.
pub fn find_boundaries(rects: &[Rect2D]) -> Vec<RectBoundary> {
    let mut boundaries = Vec::new();

    for i in 0..rects.len() {
        for j in (i + 1)..rects.len() {
            let a = &rects[i];
            let b = &rects[j];

            // East: A's east side adjacent to B's west side
            if a.max().x + 1 == b.min().x {
                let z_start = a.min().y.max(b.min().y);
                let z_end = a.max().y.min(b.max().y);
                if z_start <= z_end {
                    // Wall on A's inside edge (last column of A)
                    let cells = (z_start..=z_end)
                        .map(|z| Point2D::new(a.max().x, z))
                        .collect();
                    boundaries.push(RectBoundary { rect_a: i, rect_b: j, wall_cells: cells });
                }
            }
            // West: B's east side adjacent to A's west side
            else if b.max().x + 1 == a.min().x {
                let z_start = a.min().y.max(b.min().y);
                let z_end = a.max().y.min(b.max().y);
                if z_start <= z_end {
                    // Wall on A's inside edge (first column of A)
                    let cells = (z_start..=z_end)
                        .map(|z| Point2D::new(a.min().x, z))
                        .collect();
                    boundaries.push(RectBoundary { rect_a: i, rect_b: j, wall_cells: cells });
                }
            }
            // South: A's south side adjacent to B's north side
            else if a.max().y + 1 == b.min().y {
                let x_start = a.min().x.max(b.min().x);
                let x_end = a.max().x.min(b.max().x);
                if x_start <= x_end {
                    // Wall on A's inside edge (last row of A)
                    let cells = (x_start..=x_end)
                        .map(|x| Point2D::new(x, a.max().y))
                        .collect();
                    boundaries.push(RectBoundary { rect_a: i, rect_b: j, wall_cells: cells });
                }
            }
            // North: B's south side adjacent to A's north side
            else if b.max().y + 1 == a.min().y {
                let x_start = a.min().x.max(b.min().x);
                let x_end = a.max().x.min(b.max().x);
                if x_start <= x_end {
                    // Wall on A's inside edge (first row of A)
                    let cells = (x_start..=x_end)
                        .map(|x| Point2D::new(x, a.min().y))
                        .collect();
                    boundaries.push(RectBoundary { rect_a: i, rect_b: j, wall_cells: cells });
                }
            }
        }
    }

    boundaries
}

/// Find which rect index contains the primary exterior door on the ground floor.
fn find_entry_rect(rects: &[Rect2D], wall_segs: &WallSegments) -> Option<usize> {
    for (seg, opening) in wall_segs.doors() {
        if seg.floor != 0 {
            continue;
        }
        let cells = walls::segment_cells(seg);
        let idx = opening.offset as usize;
        if idx >= cells.len() {
            continue;
        }
        let door_cell = cells[idx];
        // The room is one cell inward from the door
        let inward: Point2D = (-seg.facing).into();
        let interior_cell = door_cell + inward;

        for (i, rect) in rects.iter().enumerate() {
            if rect.contains(interior_cell) {
                return Some(i);
            }
        }
    }
    None
}

/// Assign roles to active rects on a given floor.
fn assign_roles(
    rects: &[Rect2D],
    active_indices: &[usize],
    floor: u32,
    entry_rect: Option<usize>,
) -> Vec<(usize, RoomRole)> {
    if floor > 0 {
        return active_indices.iter().map(|&i| (i, RoomRole::Upper)).collect();
    }

    let mut assignments: Vec<(usize, RoomRole)> = Vec::new();
    let mut entry_assigned = false;

    // Entry goes to the rect containing the door
    if let Some(entry_idx) = entry_rect {
        if active_indices.contains(&entry_idx) {
            assignments.push((entry_idx, RoomRole::Entry));
            entry_assigned = true;
        }
    }

    // Main goes to the largest remaining rect
    let remaining: Vec<usize> = active_indices
        .iter()
        .filter(|&&i| !assignments.iter().any(|(ai, _)| *ai == i))
        .copied()
        .collect();

    if let Some(&main_idx) = remaining.iter().max_by_key(|&&i| rects[i].area()) {
        if !entry_assigned {
            // No door found — treat the largest room as entry
            assignments.push((main_idx, RoomRole::Entry));
        } else {
            assignments.push((main_idx, RoomRole::Main));
        }
    }

    // Rest are secondary
    for &i in active_indices {
        if !assignments.iter().any(|(ai, _)| *ai == i) {
            assignments.push((i, RoomRole::Secondary));
        }
    }

    assignments
}

/// Find an archway position that doesn't conflict with stairwells.
/// Tries the center first. If blocked, picks the wall corner (index 0 or last)
/// that is furthest from any stair cell, then searches inward from that end.
fn find_archway_pos(
    interior_cells: &[&Point2D],
    stair_cells: &HashSet<(i32, i32)>,
) -> usize {
    let len = interior_cells.len();
    if len == 0 {
        return 0;
    }
    let center = len / 2;

    let is_blocked = |idx: usize| -> bool {
        let cell = interior_cells[idx];
        for (dx, dz) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
            if stair_cells.contains(&(cell.x + dx, cell.y + dz)) {
                return true;
            }
        }
        false
    };

    if !is_blocked(center) {
        return center;
    }

    // Find which corner is further from stair cells and search from that end.
    let min_stair_dist = |idx: usize| -> i32 {
        let cell = interior_cells[idx];
        stair_cells.iter()
            .map(|&(sx, sz)| (cell.x - sx).abs() + (cell.y - sz).abs())
            .min()
            .unwrap_or(i32::MAX)
    };

    let start_dist = min_stair_dist(0);
    let end_dist = min_stair_dist(len - 1);

    // Search from the corner furthest from stairs toward the other end
    let iter: Box<dyn Iterator<Item = usize>> = if end_dist >= start_dist {
        Box::new((0..len).rev())
    } else {
        Box::new(0..len)
    };

    for idx in iter {
        if !is_blocked(idx) {
            return idx;
        }
    }

    center
}

/// Generate rooms and place interior walls between adjacent rects.
/// Returns a RoomPlan for use by the furniture module later.
pub async fn build_rooms(
    editor: &Editor,
    frame: &Frame,
    wall_segs: &WallSegments,
    floor_plan: &FloorPlan,
    has_attic: bool,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
) -> RoomPlan {
    let rects = frame.footprint().rects();
    let boundaries = find_boundaries(rects);
    let entry_rect = find_entry_rect(rects, wall_segs);

    // Collect all stairwell (x,z) positions so archways can avoid them.
    // Skip positions[0] for straight stairs — it's just the landing with no block.
    let stair_cells: HashSet<(i32, i32)> = floor_plan.stairwells.iter()
        .flat_map(|sw| {
            let skip = match sw.kind { super::floors::StairKind::Straight => 1, _ => 0 };
            sw.positions.iter().skip(skip).map(|p| (p.x, p.y))
        })
        .collect();

    let mut rooms = Vec::new();

    // Interior walls use secondary wood (distinct from floors and exterior walls)
    let material_id = palette
        .get_material(MaterialRole::SecondaryWood)
        .or_else(|| palette.get_material(MaterialRole::PrimaryWood))
        .expect("No wood material for interior walls")
        .clone();
    let mut placer_rng = rng.derive();
    let mut placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng),
        material_id,
    );

    for floor in frame.floors() {
        let active = frame.active_rects(floor);

        // Compute perimeter cells so interior walls don't overwrite exterior walls
        let outline = frame.outline_at_floor(floor);
        let n = outline.len();
        let mut perimeter: HashSet<(i32, i32)> = HashSet::new();
        for i in 0..n {
            let start = outline[i];
            let end = outline[(i + 1) % n];
            for cell in walk_edge_cells(start, end) {
                perimeter.insert((cell.x, cell.y));
            }
        }
        for cell in concave_corner_cells(&outline) {
            perimeter.insert((cell.x, cell.y));
        }

        // Place interior walls at boundaries where both rects are active
        for boundary in &boundaries {
            if !active.contains(&boundary.rect_a) || !active.contains(&boundary.rect_b) {
                continue;
            }

            let base_y = frame.floor_y(floor);
            let height = frame.wall_height();

            // Filter out cells that overlap the exterior perimeter
            let interior_cells: Vec<&Point2D> = boundary.wall_cells.iter()
                .filter(|c| !perimeter.contains(&(c.x, c.y)))
                .collect();

            // Archway door: 1 wide, 2 tall, centered unless blocked by stairs
            let door_pos = find_archway_pos(&interior_cells, &stair_cells);

            for (i, cell) in interior_cells.iter().enumerate() {
                for ry in 0..height {
                    // Leave a 1x2 archway opening
                    if i == door_pos && ry < 2 {
                        continue;
                    }
                    let y = base_y + ry as i32;
                    placer.place_block(
                        editor,
                        Point3D::new(cell.x, y, cell.y),
                        BlockForm::Block,
                        None,
                        None,
                    ).await;
                }
            }
        }

        // Assign roles and create rooms
        let assignments = assign_roles(rects, &active, floor, entry_rect);
        for (rect_idx, role) in assignments {
            rooms.push(Room {
                rect: rects[rect_idx],
                rect_index: rect_idx,
                floor,
                role,
            });
        }
    }

    // Add attic rooms for each rect (the attic floor is one above each rect's top floor)
    if has_attic {
        for (i, rect) in rects.iter().enumerate() {
            rooms.push(Room {
                rect: *rect,
                rect_index: i,
                floor: frame.floor_counts()[i],
                role: RoomRole::Attic,
            });
        }
    }

    RoomPlan { rooms }
}
