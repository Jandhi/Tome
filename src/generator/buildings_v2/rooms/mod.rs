#[cfg(test)]
mod test;

use std::collections::{HashMap, HashSet};

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Point2D, Point3D, Rect2D};
use crate::minecraft::BlockForm;
use crate::noise::RNG;
use super::footprint::merge::{walk_edge_cells, concave_corner_cells};
use super::footprint::{find_boundaries, SizeClass};
use super::frame::Frame;
use super::floors::FloorPlan;
use super::walls::{self, WallSegments};
use super::RoomType;

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

/// State of a cell in a room's walkability grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloorCell {
    /// Empty floor — walkable, furniture can be placed here.
    Open,
    /// Door/interior door — walkable, no furniture, must be reachable by BFS.
    ReachableOpen,
    /// Must be adjacent to a reachable cell (player can interact), but not
    /// walkable. E.g. foot of a bed, chest.
    ReachableBlocked,
    /// Impassable with no reachability requirement (stairwells, walls, bed head).
    Blocked,
}

/// 2D walkability grid for a room's interior.
/// Keys are world (x, z) coords. Only interior cells (inside walls) are present.
pub type FloorMap = HashMap<(i32, i32), FloorCell>;

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
    /// Walkability grid for furniture placement and connectivity checks.
    pub floor_map: FloorMap,
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

/// A room assignment: rect index, floor, and room type.
pub type RoomAssignment = (usize, u32, RoomType);

/// Assign room types for every room in a building.
/// Returns (rect_index, floor, RoomType) for each room, including attics.
pub fn assign_room_types(
    frame: &Frame,
    size_class: SizeClass,
    has_attic: bool,
    rng: &mut RNG,
) -> Vec<RoomAssignment> {
    match size_class {
        SizeClass::Cottage => assign_cottage_rooms(frame, has_attic),
        SizeClass::House => assign_house_rooms(frame, has_attic),
        SizeClass::Hall => assign_hall_rooms(frame, has_attic, rng),
        SizeClass::Manor => assign_manor_rooms(frame, has_attic, rng),
    }
}

fn assign_cottage_rooms(frame: &Frame, has_attic: bool) -> Vec<RoomAssignment> {
    let rects = frame.footprint().rects();
    let mut rooms = Vec::new();

    for floor in frame.floors() {
        for &idx in &frame.active_rects(floor) {
            let room_type = if idx == 0 { RoomType::Common } else { RoomType::Storage };
            rooms.push((idx, floor, room_type));
        }
    }

    if has_attic {
        for i in 0..rects.len() {
            rooms.push((i, frame.floor_counts()[i], RoomType::Storage));
        }
    }

    rooms
}

fn assign_house_rooms(frame: &Frame, has_attic: bool) -> Vec<RoomAssignment> {
    let rects = frame.footprint().rects();
    let num_rects = rects.len();
    let max_floors = frame.max_floors();
    let mut rooms = Vec::new();

    for floor in frame.floors() {
        for &idx in &frame.active_rects(floor) {
            let room_type = if max_floors == 1 {
                if num_rects == 1 {
                    RoomType::Common
                } else if idx == 0 {
                    RoomType::Hearth
                } else {
                    RoomType::Bedroom
                }
            } else if floor == 0 {
                if idx == 0 { RoomType::Hearth } else { RoomType::Storage }
            } else {
                RoomType::Bedroom
            };
            rooms.push((idx, floor, room_type));
        }
    }

    if has_attic {
        for i in 0..rects.len() {
            rooms.push((i, frame.floor_counts()[i], RoomType::Storage));
        }
    }

    rooms
}

fn assign_hall_rooms(frame: &Frame, has_attic: bool, rng: &mut RNG) -> Vec<RoomAssignment> {
    let rects = frame.footprint().rects();
    let mut rooms = Vec::new();

    // Sort wing indices by area descending so larger wings get priority roles
    let mut wing_indices: Vec<usize> = (1..rects.len()).collect();
    wing_indices.sort_by(|&a, &b| rects[b].area().cmp(&rects[a].area()));

    // Map each rect index to its size rank among wings (0 = largest)
    let mut wing_rank = vec![0usize; rects.len()];
    for (rank, &idx) in wing_indices.iter().enumerate() {
        wing_rank[idx] = rank;
    }

    let ground_wing_sequence = [RoomType::Kitchen, RoomType::Pantry, RoomType::Storage];
    let upper_wing_sequence = [RoomType::MasterBedroom, RoomType::Study];

    for floor in frame.floors() {
        for &idx in &frame.active_rects(floor) {
            let room_type = if floor == 0 {
                if idx == 0 {
                    RoomType::GreatRoom
                } else {
                    *ground_wing_sequence.get(wing_rank[idx]).unwrap_or(&RoomType::Storage)
                }
            } else {
                if idx == 0 {
                    RoomType::MultiBedroom
                } else {
                    match upper_wing_sequence.get(wing_rank[idx]) {
                        Some(&t) => t,
                        None => if rng.chance(1, 2) { RoomType::Bedroom } else { RoomType::Storage },
                    }
                }
            };
            rooms.push((idx, floor, room_type));
        }
    }

    if has_attic {
        for i in 0..rects.len() {
            rooms.push((i, frame.floor_counts()[i], RoomType::Storage));
        }
    }

    rooms
}

fn assign_manor_rooms(frame: &Frame, has_attic: bool, rng: &mut RNG) -> Vec<RoomAssignment> {
    let rects = frame.footprint().rects();
    let mut rooms = Vec::new();
    let mut has_dining = false;
    let mut has_bedroom = false;
    let mut has_study = false;
    let mut has_library = false;
    let mut has_studio = false;
    let mut has_armory = false;

    for floor in frame.floors() {
        for &idx in &frame.active_rects(floor) {
            let room_type = if floor == 0 {
                if idx == 0 {
                    RoomType::Hearth
                } else if !has_dining && rng.chance(1, 2) {
                    has_dining = true;
                    RoomType::Dining
                } else {
                    RoomType::Storage
                }
            } else {
                if !has_bedroom {
                    has_bedroom = true;
                    RoomType::Bedroom
                } else if !has_library && rng.chance(1, 5) {
                    has_library = true;
                    RoomType::Library
                } else if !has_studio && rng.chance(1, 5) {
                    has_studio = true;
                    RoomType::Studio
                } else if !has_armory && rng.chance(1, 5) {
                    has_armory = true;
                    RoomType::Armory
                } else if !has_study && rng.chance(1, 4) {
                    has_study = true;
                    RoomType::Study
                } else {
                    RoomType::Bedroom
                }
            };
            rooms.push((idx, floor, room_type));
        }
    }

    if has_attic {
        for i in 0..rects.len() {
            rooms.push((i, frame.floor_counts()[i], RoomType::Storage));
        }
    }

    rooms
}


/// Clamp a point to the nearest cell inside a (non-empty) interior rect.
/// Used to find the entrance cell for a room given a door/interior-door position
/// that may be on the wall edge rather than inside the shrunk interior.
fn nearest_interior_cell(point: Point2D, interior: &Rect2D) -> Point2D {
    Point2D::new(
        point.x.clamp(interior.min().x, interior.max().x),
        point.y.clamp(interior.min().y, interior.max().y),
    )
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
    size_class: SizeClass,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
) -> RoomPlan {
    let rects = frame.footprint().rects();
    let boundaries = find_boundaries(rects);

    // Collect all stairwell (x,z) positions so archways can avoid them.
    // Skip positions[0] for straight stairs — it's just the landing with no block.
    let stair_cells: HashSet<(i32, i32)> = floor_plan.stairwells.iter()
        .flat_map(|sw| {
            let skip = match sw.kind { super::floors::StairKind::Straight => 1, _ => 0 };
            sw.positions.iter().skip(skip).map(|p| (p.x, p.y))
        })
        .collect();

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

    // Interior doors: (floor, rect_a, rect_b, cell position of the gap).
    let mut interior_doors: Vec<(u32, usize, usize, Point2D)> = Vec::new();

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

            // Interior door: 1 wide, 2 tall, centered unless blocked by stairs
            let door_pos = find_archway_pos(&interior_cells, &stair_cells);

            if door_pos < interior_cells.len() {
                interior_doors.push((
                    floor,
                    boundary.rect_a,
                    boundary.rect_b,
                    *interior_cells[door_pos],
                ));
            }

            for (i, cell) in interior_cells.iter().enumerate() {
                for ry in 0..height {
                    // Leave a 1x2 opening for the interior door
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
    }

    // All stairwell cells (for marking Blocked in floor maps)
    let all_stair_cells: HashSet<(i32, i32)> = floor_plan.stairwells.iter()
        .flat_map(|sw| sw.positions.iter().map(|p| (p.x, p.y)))
        .collect();

    // Assign room types for the whole building
    let assignments = assign_room_types(frame, size_class, has_attic, rng);

    // Build Room structs with floor maps
    let entry_rect = find_entry_rect(rects, wall_segs);
    let mut rooms = Vec::new();
    for (rect_idx, floor, room_type) in assignments {
        let role = if floor >= frame.floor_counts().get(rect_idx).copied().unwrap_or(0) {
            RoomRole::Attic
        } else if floor > 0 {
            RoomRole::Upper
        } else if Some(rect_idx) == entry_rect || (entry_rect.is_none() && rect_idx == 0) {
            RoomRole::Entry
        } else {
            RoomRole::Secondary
        };

        let rect = rects[rect_idx];
        let interior = rect.shrink(1);
        let has_interior = interior.size.x > 0 && interior.size.y > 0;
        let mut floor_map = FloorMap::new();

        if has_interior {
            // Start with all interior cells as Open
            for cell in interior.iter() {
                floor_map.insert((cell.x, cell.y), FloorCell::Open);
            }

            // Mark stairwell cells as Blocked
            for cell in interior.iter() {
                if all_stair_cells.contains(&(cell.x, cell.y)) {
                    floor_map.insert((cell.x, cell.y), FloorCell::Blocked);
                }
            }

            // Interior doors → Entrance
            for &(door_floor, rect_a, rect_b, door_cell) in &interior_doors {
                if door_floor != floor { continue; }
                if rect_a != rect_idx && rect_b != rect_idx { continue; }
                let entrance = nearest_interior_cell(door_cell, &interior);
                floor_map.insert((entrance.x, entrance.y), FloorCell::ReachableOpen);
            }

            // Exterior doors → Entrance
            for (seg, opening) in wall_segs.doors() {
                if seg.floor != floor { continue; }
                let cells = walls::segment_cells(seg);
                let idx = opening.offset as usize;
                if idx >= cells.len() { continue; }
                let door_cell = cells[idx];
                let inward: Point2D = (-seg.facing).into();
                let stepped = door_cell + inward;
                if rect.contains(stepped) {
                    let entrance = nearest_interior_cell(stepped, &interior);
                    floor_map.insert((entrance.x, entrance.y), FloorCell::ReachableOpen);
                }
            }
        }

        rooms.push(Room {
            rect,
            rect_index: rect_idx,
            floor,
            role,
            room_type,
            floor_map,
        });
    }

    RoomPlan { rooms }
}
