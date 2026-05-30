#[cfg(test)]
mod test;
pub mod constraints;

use std::collections::{HashMap, HashSet};

use crate::editor::Editor;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Placer};
use crate::geometry::{Point2D, Point3D, Rect2D};
use crate::minecraft::{Block, BlockForm};
use crate::noise::RNG;
use super::footprint::merge::{walk_edge_cells, concave_corner_cells};
use super::footprint::{find_boundaries, SizeClass};
use super::frame::Frame;
use super::floors::FloorPlan;
use super::pipeline::BuildCtx;
use super::walls::{self, WallSegments};
use super::{RoomType, FloorType};

pub use constraints::{CellState, ConstraintMap, PlacedFurniture};

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

/// Compute wing size rank: maps each rect index to its rank among wings by area (0 = largest).
fn wing_ranks(frame: &Frame) -> Vec<usize> {
    let rects = frame.footprint().rects();
    let mut wing_indices: Vec<usize> = (1..rects.len()).collect();
    wing_indices.sort_by(|&a, &b| rects[b].area().cmp(&rects[a].area()));
    let mut ranks = vec![0usize; rects.len()];
    for (rank, &idx) in wing_indices.iter().enumerate() {
        ranks[idx] = rank;
    }
    ranks
}

/// Assign a bedroom type if budget allows, otherwise a non-bedroom fallback.
fn try_bedroom(budget: &mut RoomBudget, rng: &mut RNG, room_type: RoomType) -> RoomType {
    if budget.needs_bedroom() {
        budget.add_bedroom();
        room_type
    } else if rng.chance(1, 2) {
        RoomType::Study
    } else {
        RoomType::Storage
    }
}

/// Pick a room type for a non-attic room based on size class, floor, and rect index.
fn pick_room_type(
    size_class: SizeClass,
    floor: u32,
    rect_idx: usize,
    frame: &Frame,
    wing_rank: &[usize],
    rng: &mut RNG,
    budget: &mut RoomBudget,
) -> RoomType {
    match size_class {
        SizeClass::Cottage => {
            if rect_idx == 0 {
                RoomType::Common
            } else {
                try_bedroom(budget, rng, RoomType::Bedroom)
            }
        }
        SizeClass::House => {
            let num_rects = frame.footprint().rects().len();
            if frame.max_floors() == 1 {
                if num_rects == 1 {
                    RoomType::Common
                } else if rect_idx == 0 {
                    RoomType::Hearth
                } else {
                    try_bedroom(budget, rng, RoomType::Bedroom)
                }
            } else if floor == 0 {
                if rect_idx == 0 { RoomType::Hearth } else { RoomType::Storage }
            } else {
                try_bedroom(budget, rng, RoomType::Bedroom)
            }
        }
        SizeClass::Hall => {
            let ground_seq = [RoomType::Kitchen, RoomType::Pantry, RoomType::Storage];
            if floor == 0 {
                if rect_idx == 0 { RoomType::GreatRoom }
                else { *ground_seq.get(wing_rank[rect_idx]).unwrap_or(&RoomType::Storage) }
            } else if rect_idx == 0 {
                if budget.needs_bedroom() {
                    budget.add_bedroom();
                    // MultiBedroom counts as 2 toward the budget
                    budget.add_bedroom();
                    RoomType::MultiBedroom
                } else {
                    RoomType::Study
                }
            } else {
                let rank = wing_rank[rect_idx];
                if rank == 0 {
                    try_bedroom(budget, rng, RoomType::MasterBedroom)
                } else {
                    try_bedroom(budget, rng, RoomType::Bedroom)
                }
            }
        }
        SizeClass::Manor => {
            if floor == 0 {
                if rect_idx == 0 {
                    RoomType::Hearth
                } else if !budget.dining && rng.chance(1, 2) {
                    budget.dining = true;
                    RoomType::Dining
                } else {
                    RoomType::Storage
                }
            } else {
                if budget.bedrooms == 0 && budget.needs_bedroom() {
                    budget.add_bedroom();
                    RoomType::Bedroom
                } else if !budget.library && rng.chance(1, 5) {
                    budget.library = true;
                    RoomType::Library
                } else if !budget.studio && rng.chance(1, 5) {
                    budget.studio = true;
                    RoomType::Studio
                } else if !budget.armory && rng.chance(1, 5) {
                    budget.armory = true;
                    RoomType::Armory
                } else if !budget.study && rng.chance(1, 4) {
                    budget.study = true;
                    RoomType::Study
                } else {
                    try_bedroom(budget, rng, RoomType::Bedroom)
                }
            }
        }
    }
}

/// Tracks bedroom count and unique room assignments across the building.
struct RoomBudget {
    bedrooms: u32,
    target_bedrooms: u32,
    dining: bool,
    study: bool,
    library: bool,
    studio: bool,
    armory: bool,
}

impl RoomBudget {
    fn new(size_class: SizeClass, rng: &mut RNG) -> Self {
        let target = rng.rand_i32_range(
            size_class.min_bedrooms() as i32,
            size_class.max_bedrooms() as i32 + 1,
        ) as u32;
        Self {
            bedrooms: 0,
            target_bedrooms: target,
            dining: false,
            study: false,
            library: false,
            studio: false,
            armory: false,
        }
    }

    fn needs_bedroom(&self) -> bool {
        self.bedrooms < self.target_bedrooms
    }

    fn add_bedroom(&mut self) {
        self.bedrooms += 1;
    }
}

fn is_bedroom_type(room_type: RoomType) -> bool {
    matches!(room_type, RoomType::Bedroom | RoomType::MultiBedroom | RoomType::MasterBedroom)
}

/// Assign types to attic rooms using the building's bedroom budget.
/// Attics above bedrooms stay Storage (redundant sleeping space).
/// Attics above non-bedrooms may become bedrooms if the budget allows.
/// Call after `place_attic_ladders` so all attic rects are accessible.
pub fn assign_attic_types(room_plan: &mut RoomPlan, size_class: SizeClass, rng: &mut RNG) {
    // Count bedrooms already assigned to non-attic rooms
    let existing = room_plan.rooms.iter()
        .filter(|r| r.role != RoomRole::Attic && is_bedroom_type(r.room_type))
        .map(|r| if r.room_type == RoomType::MultiBedroom { 2u32 } else { 1 })
        .sum::<u32>();

    let target = rng.rand_i32_range(
        size_class.min_bedrooms() as i32,
        size_class.max_bedrooms() as i32 + 1,
    ) as u32;
    let mut remaining = target.saturating_sub(existing);

    for i in 0..room_plan.rooms.len() {
        let room = &room_plan.rooms[i];
        if room.role != RoomRole::Attic { continue; }
        let rect_idx = room.rect_index;
        let floor = room.floor;
        let below_is_bedroom = room_plan.rooms.iter()
            .find(|r| r.rect_index == rect_idx && r.floor + 1 == floor)
            .map(|r| is_bedroom_type(r.room_type))
            .unwrap_or(false);

        room_plan.rooms[i].room_type = if below_is_bedroom {
            // Attic above a bedroom — no need for another bedroom here
            RoomType::Storage
        } else if remaining > 0 {
            remaining -= 1;
            RoomType::Bedroom
        } else {
            RoomType::Storage
        };
    }
}

/// Assign all room types (non-attic + attic). Used by tests that construct
/// rooms manually without going through build_rooms.
pub fn assign_types_to_rooms(
    room_plan: &mut RoomPlan,
    frame: &Frame,
    size_class: SizeClass,
    rng: &mut RNG,
) {
    let ranks = wing_ranks(frame);
    let mut budget = RoomBudget::new(size_class, rng);

    let mut indices: Vec<usize> = (0..room_plan.rooms.len()).collect();
    indices.sort_by_key(|&i| (room_plan.rooms[i].floor, room_plan.rooms[i].rect_index));

    for &i in &indices {
        let room = &room_plan.rooms[i];
        if room.role == RoomRole::Attic { continue; }
        let room_type = pick_room_type(
            size_class, room.floor, room.rect_index,
            frame, &ranks, rng, &mut budget,
        );
        room_plan.rooms[i].room_type = room_type;
    }

    assign_attic_types(room_plan, size_class, rng);
}

/// Assign custom floor types to rooms based on their room type.
pub fn assign_room_floors(room_plan: &mut RoomPlan) {
    for room in &mut room_plan.rooms {
        room.floor_type = match room.room_type {
            RoomType::Kitchen => Some(FloorType::Kitchen),
            _ => None,
        };
    }
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
/// Non-attic room types are assigned immediately. Attic types are deferred
/// until `assign_attic_types` is called (after roof/ladders are placed).
pub async fn build_rooms(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
    wall_segs: &WallSegments,
    floor_plan: &FloorPlan,
    has_attic: bool,
    size_class: SizeClass,
) -> RoomPlan {
    let editor: &Editor = &*ctx.editor;
    let data = ctx.data;
    let palette = ctx.palette;
    let rng = &mut *ctx.rng;

    let rects = frame.footprint().rects();
    let boundaries = find_boundaries(rects);

    // Stair cells used to steer archway placement away from the stair
    // footprint, computed per-floor. For straight stairs, drop the topmost
    // step — its block sits at head-clearance + 1 on the lower floor, so the
    // column underneath is walkable and a door approach through that cell
    // is fine.
    let archway_stair_cells = |floor: u32| -> HashSet<(i32, i32)> {
        floor_plan.stairwells.iter()
            .filter(|sw| sw.floor == floor)
            .flat_map(|sw| {
                let take = match sw.kind {
                    super::floors::StairKind::Straight => sw.positions.len().saturating_sub(1),
                    _ => sw.positions.len(),
                };
                sw.positions.iter().take(take).map(|p| (p.x, p.y))
            })
            .collect()
    };

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
        let stair_cells = archway_stair_cells(floor);

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

    // Cache per-floor stair-cell sets so a stairwell only affects the floor
    // it actually starts on. Using the global set here would incorrectly mark
    // attic-stair cells as Blocked on floor 0 (and vice versa).
    let mut stair_cells_by_floor: HashMap<u32, HashSet<(i32, i32)>> = HashMap::new();
    let stair_bottoms = &floor_plan.stair_bottoms;
    let stair_tops = &floor_plan.stair_tops;

    // Build Room structs. Non-attic types are assigned now; attic types
    // are deferred until assign_attic_types() after roof/ladders are placed.
    let entry_rect = find_entry_rect(rects, wall_segs);
    let ranks = wing_ranks(frame);
    let mut budget = RoomBudget::new(size_class, rng);

    // Enumerate all (rect_idx, floor) combos including attics
    let mut room_slots: Vec<(usize, u32)> = Vec::new();
    for floor in frame.floors() {
        for &idx in frame.active_rects(floor) {
            room_slots.push((idx, floor));
        }
    }
    if has_attic {
        for i in 0..rects.len() {
            room_slots.push((i, frame.floor_counts()[i]));
        }
    }

    let mut rooms = Vec::new();
    for (rect_idx, floor) in room_slots {
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
        let interior = compute_room_interior(rects, rect_idx);
        let has_interior = interior.size.x > 0 && interior.size.y > 0;
        let mut constraints = ConstraintMap::new(&interior);

        if has_interior {
            // Stair footprint: non-landing cells get Blocked (physical stair
            // blocks), landings + approaches get UnblockedReachable (player
            // walks through them, but no furniture allowed). stair_bottoms
            // includes both straight-stair flat landings and the approach
            // cells in front of spiral/L-shaped stairs, so it's checked
            // independently of the stair footprint.
            //
            // stair_air_above reserves the air-column cells on the floor
            // directly above each stair so furniture can't land in the
            // player's head-clearance during ascent.
            let this_floor_stair_cells = stair_cells_by_floor
                .entry(floor)
                .or_insert_with(|| floor_plan.stair_cells_on_floor(floor));
            for cell in interior.iter() {
                let xz = (cell.x, cell.y);
                let key = (floor, cell.x, cell.y);
                if stair_bottoms.contains(&key) || stair_tops.contains(&key) {
                    constraints.set(xz, CellState::UnblockedReachable);
                    constraints.set_ceiling(xz);
                } else if this_floor_stair_cells.contains(&xz) {
                    constraints.set(xz, CellState::Blocked);
                    constraints.set_ceiling(xz);
                } else if floor_plan.stair_air_above.contains(&key) {
                    constraints.set(xz, CellState::UnblockedReachable);
                    constraints.set_ceiling(xz);
                }
            }

            // Interior / exterior door entrances are cells the player walks
            // *through*, not claimed approach spots — mark them
            // UnblockedReachable so they stay walkable (and furniture can't
            // drop on them), rather than BR (which would make them impassable
            // and force connectivity checks to find another walkable neighbor).
            for &(door_floor, rect_a, rect_b, door_cell) in &interior_doors {
                if door_floor != floor { continue; }
                if rect_a != rect_idx && rect_b != rect_idx { continue; }
                let entrance = nearest_interior_cell(door_cell, &interior);
                constraints.set((entrance.x, entrance.y), CellState::UnblockedReachable);
            }

            for (seg, opening) in wall_segs.doors() {
                if seg.floor != floor { continue; }
                let cells = walls::segment_cells(seg);
                for dx in 0..opening.width {
                    let idx = (opening.offset + dx) as usize;
                    if idx >= cells.len() { continue; }
                    let door_cell = cells[idx];
                    if !rect.on_edge(door_cell) { continue; }
                    let entrance = nearest_interior_cell(door_cell, &interior);
                    if interior.contains(entrance) {
                        constraints.set((entrance.x, entrance.y), CellState::UnblockedReachable);
                    }
                }
            }

        }

        let room_type = if role == RoomRole::Attic {
            RoomType::Storage // placeholder — assigned by assign_attic_types()
        } else {
            pick_room_type(size_class, floor, rect_idx, frame, &ranks, rng, &mut budget)
        };

        rooms.push(Room {
            rect,
            rect_index: rect_idx,
            floor,
            role,
            room_type,
            interior,
            constraints,
            furniture: Vec::new(),
            floor_type: None,
        });
    }

    RoomPlan { rooms, interior_doors }
}

/// Mark gable doorway cells as BlockedReachable in attic room constraint maps.
/// Call after `place_roof` returns doorway positions.
pub fn mark_gable_doorways(room_plan: &mut RoomPlan, doorways: &[Point2D]) {
    for room in &mut room_plan.rooms {
        if room.role != RoomRole::Attic { continue; }
        let interior = room.interior;
        if interior.size.x <= 0 || interior.size.y <= 0 { continue; }

        for &door_cell in doorways {
            if !room.rect.on_edge(door_cell) { continue; }
            let entrance = nearest_interior_cell(door_cell, &interior);
            if interior.contains(entrance) {
                room.constraints.set((entrance.x, entrance.y), CellState::UnblockedReachable);
            }
        }
    }
}

/// Place ladders in attic rooms that have no stair access and aren't connected
/// (via gable doorways) to an attic room that does. Ladder goes in a corner,
/// marked BlockedReachable on both the attic and the floor below.
/// Call after mark_gable_doorways but before furnishing.
/// Returns the wall cells behind each ladder, so `place_windows` can avoid them.
pub async fn place_attic_ladders(
    ctx: &mut BuildCtx<'_>,
    room_plan: &mut RoomPlan,
    frame: &Frame,
    floor_plan: &FloorPlan,
    wall_segs: &WallSegments,
    gable_doorways: &[Point2D],
) -> Vec<(i32, i32)> {
    let editor: &Editor = &*ctx.editor;
    let rects = frame.footprint().rects();

    // Find all attic rooms (room index → rect index)
    let attic_rooms: Vec<(usize, usize)> = room_plan.rooms.iter().enumerate()
        .filter(|(_, r)| r.role == RoomRole::Attic)
        .map(|(i, r)| (i, r.rect_index))
        .collect();

    if attic_rooms.is_empty() { return Vec::new(); }

    // Which rect indices have stair access to the attic floor?
    // A stairwell reaches floor F+1, so if sw.floor == attic_floor - 1
    // and the stair top position is within the rect, that rect has access.
    let mut has_stair_access: HashSet<usize> = HashSet::new();
    for &(_, rect_idx) in &attic_rooms {
        let attic_floor = room_plan.rooms.iter()
            .find(|r| r.role == RoomRole::Attic && r.rect_index == rect_idx)
            .map(|r| r.floor)
            .unwrap();
        for sw in &floor_plan.stairwells {
            if sw.floor + 1 != attic_floor { continue; }
            if let Some(top) = sw.positions.last() {
                if rects[rect_idx].contains(*top) {
                    has_stair_access.insert(rect_idx);
                }
            }
        }
    }

    // Build connectivity graph between attic rects via gable doorways and interior doors
    let mut connected: HashMap<usize, HashSet<usize>> = HashMap::new();

    // Gable doorways: find which attic rects each doorway touches
    for &door_cell in gable_doorways {
        let mut touching: Vec<usize> = Vec::new();
        for &(_, rect_idx) in &attic_rooms {
            if rects[rect_idx].on_edge(door_cell) {
                touching.push(rect_idx);
            }
        }
        for i in 0..touching.len() {
            for j in (i+1)..touching.len() {
                connected.entry(touching[i]).or_default().insert(touching[j]);
                connected.entry(touching[j]).or_default().insert(touching[i]);
            }
        }
    }

    // Adjacent attic rects with shared boundaries are inherently connected
    // (no interior walls are placed on the attic floor)
    let attic_rect_set: HashSet<usize> = attic_rooms.iter().map(|&(_, ri)| ri).collect();
    let boundaries = find_boundaries(rects);
    for b in &boundaries {
        if attic_rect_set.contains(&b.rect_a) && attic_rect_set.contains(&b.rect_b) {
            // Both rects have attics at the same floor level
            if frame.floor_counts()[b.rect_a] == frame.floor_counts()[b.rect_b] {
                connected.entry(b.rect_a).or_default().insert(b.rect_b);
                connected.entry(b.rect_b).or_default().insert(b.rect_a);
            }
        }
    }

    // Flood fill to find which rects are reachable from a rect with stair access
    let mut accessible: HashSet<usize> = has_stair_access.clone();
    let mut changed = true;
    while changed {
        changed = false;
        let current = accessible.clone();
        for &rect_idx in &current {
            if let Some(neighbors) = connected.get(&rect_idx) {
                for &n in neighbors {
                    if accessible.insert(n) {
                        changed = true;
                    }
                }
            }
        }
    }

    let mut ladder_wall_cells: Vec<(i32, i32)> = Vec::new();

    // Collect window cells across all floors for intersection checks
    let window_cells: HashSet<(i32, i32)> = wall_segs.windows()
        .flat_map(|(seg, opening)| {
            let cells = walls::segment_cells(seg);
            (0..opening.width).filter_map(move |dx| {
                let idx = (opening.offset + dx) as usize;
                if idx < cells.len() { Some((cells[idx].x, cells[idx].y)) } else { None }
            })
        })
        .collect();

    // Place ladders in attic rooms that aren't accessible
    for &(room_idx, rect_idx) in attic_rooms.iter() {
        if accessible.contains(&rect_idx) { continue; }

        let attic_floor = room_plan.rooms[room_idx].floor;
        let rect = room_plan.rooms[room_idx].rect;
        let interior = room_plan.rooms[room_idx].interior;
        if interior.size.x <= 0 || interior.size.y <= 0 { continue; }

        // Try all 4 corners with both wall options each, pick one with no window
        // Each option: (interior cell, wall cell behind ladder, facing direction)
        let corners = [
            (interior.min(), Point2D::new(interior.min().x - 1, interior.min().y), "east"),
            (interior.min(), Point2D::new(interior.min().x, interior.min().y - 1), "south"),
            (Point2D::new(interior.max().x, interior.min().y), Point2D::new(interior.max().x + 1, interior.min().y), "west"),
            (Point2D::new(interior.max().x, interior.min().y), Point2D::new(interior.max().x, interior.min().y - 1), "south"),
            (Point2D::new(interior.min().x, interior.max().y), Point2D::new(interior.min().x - 1, interior.max().y), "east"),
            (Point2D::new(interior.min().x, interior.max().y), Point2D::new(interior.min().x, interior.max().y + 1), "north"),
            (interior.max(), Point2D::new(interior.max().x + 1, interior.max().y), "west"),
            (interior.max(), Point2D::new(interior.max().x, interior.max().y + 1), "north"),
        ];

        let chosen = corners.iter()
            .find(|(_, wall, _)| !window_cells.contains(&(wall.x, wall.y)))
            .unwrap_or(&corners[0]);

        let ladder_cell = (chosen.0.x, chosen.0.y);
        let wall_cell = (chosen.1.x, chosen.1.y);
        ladder_wall_cells.push(wall_cell);
        let facing = chosen.2;

        // Place ladder blocks from floor below up to attic floor level
        let attic_y = frame.floor_y(attic_floor);
        let below_y = frame.floor_y(attic_floor - 1);

        for y in below_y..attic_y {
            let mut ladder = Block::from_id("minecraft:ladder".into());
            ladder.state = Some(std::collections::HashMap::from([
                ("facing".to_string(), facing.to_string()),
            ]));
            editor.place_block_forced(&ladder, Point3D::new(ladder_cell.0, y, ladder_cell.1)).await;
        }

        // Ladder cell is walkable (player climbs through it) but no
        // furniture can land on it, so mark UnblockedReachable on both floors.
        room_plan.rooms[room_idx].constraints.set(ladder_cell, CellState::UnblockedReachable);

        for room in &mut room_plan.rooms {
            if room.floor == attic_floor - 1 && room.rect_index == rect_idx {
                let below_interior = room.interior;
                if below_interior.contains(Point2D::new(ladder_cell.0, ladder_cell.1)) {
                    room.constraints.set(ladder_cell, CellState::UnblockedReachable);
                }
            }
        }
    }

    ladder_wall_cells
}

/// Mark window cells in room constraint maps.
/// Call after `place_windows` so windows are in the wall segments.
pub fn mark_windows(room_plan: &mut RoomPlan, wall_segs: &WallSegments) {
    for room in &mut room_plan.rooms {
        let interior = room.interior;
        if interior.size.x <= 0 || interior.size.y <= 0 { continue; }

        for (seg, opening) in wall_segs.windows() {
            if seg.floor != room.floor { continue; }
            let cells = walls::segment_cells(seg);
            for dx in 0..opening.width {
                let idx = (opening.offset + dx) as usize;
                if idx >= cells.len() { continue; }
                let win_cell = cells[idx];
                if !room.rect.on_edge(win_cell) { continue; }
                let ic = nearest_interior_cell(win_cell, &interior);
                if interior.contains(ic) {
                    room.constraints.set_ceiling((ic.x, ic.y));
                    if room.constraints.is_open((ic.x, ic.y)) {
                        room.constraints.set((ic.x, ic.y), CellState::UnblockedReachable);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Invariant checks
// ---------------------------------------------------------------------------

/// Gather all wall cells for a given floor: exterior walls from the building
/// outline plus interior boundary walls from `find_boundaries`.
fn wall_cells_on_floor(frame: &Frame, floor: u32) -> HashSet<(i32, i32)> {
    let mut cells = HashSet::new();
    let outline = frame.outline_at_floor(floor);
    let n = outline.len();
    for i in 0..n {
        let start = outline[i];
        let end = outline[(i + 1) % n];
        for cell in walk_edge_cells(start, end) {
            cells.insert((cell.x, cell.y));
        }
    }
    for cell in concave_corner_cells(&outline) {
        cells.insert((cell.x, cell.y));
    }
    for b in find_boundaries(frame.footprint().rects()) {
        for cell in b.wall_cells {
            cells.insert((cell.x, cell.y));
        }
    }
    cells
}

/// Check structural invariants of a fully-generated building. Call this at
/// the end of the pipeline (after `furnish_rooms`) to catch regressions.
///
/// Checks:
///  1. **Wall-slot adjacency.** Every cell on a room's `interior.on_edge()`
///     has an adjacent wall block on the side that put it on the edge.
///     Violation means furniture placed there via a `Wall` constraint
///     would float (the phantom-wall-slot bug from the wing interior fix).
///  2. **BlockedReachable walkability.** Every `BlockedReachable` cell in
///     each room's constraint map has at least one walkable neighbor.
///     Violation means furniture fronts, stair approaches, or door
///     entrances are stranded (the connectivity bug from the table placement
///     fix).
///
/// Attic rooms are skipped for (1) since their walls come from the roof
/// module's gable geometry rather than `find_boundaries` + building outline.
pub fn check_building_invariants(
    frame: &Frame,
    room_plan: &RoomPlan,
    floor_plan: &FloorPlan,
) -> Result<(), String> {
    const NEIGHBORS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

    // Cache wall cells per floor
    let mut walls_by_floor: HashMap<u32, HashSet<(i32, i32)>> = HashMap::new();

    for room in &room_plan.rooms {
        let interior = room.interior;
        if interior.size.x <= 0 || interior.size.y <= 0 {
            continue;
        }

        // Invariant 1: every interior edge cell has a wall on the outside side.
        // Skip attic rooms — their walls come from the roof module, not from
        // outline + boundaries.
        if room.role != RoomRole::Attic {
            let wall_cells = walls_by_floor
                .entry(room.floor)
                .or_insert_with(|| wall_cells_on_floor(frame, room.floor));

            let imin = interior.min();
            let imax = interior.max();
            for cell in interior.iter() {
                let sides: [(bool, (i32, i32), &str); 4] = [
                    (cell.x == imin.x, (cell.x - 1, cell.y), "west"),
                    (cell.x == imax.x, (cell.x + 1, cell.y), "east"),
                    (cell.y == imin.y, (cell.x, cell.y - 1), "north"),
                    (cell.y == imax.y, (cell.x, cell.y + 1), "south"),
                ];
                for (on_side, adj, side_name) in sides {
                    if !on_side { continue; }
                    if !wall_cells.contains(&adj) {
                        return Err(format!(
                            "invariant (a): room {:?} floor {} interior edge cell ({},{}) \
                             has no wall on its {} side (expected wall at ({},{}))",
                            room.room_type, room.floor, cell.x, cell.y,
                            side_name, adj.0, adj.1,
                        ));
                    }
                }
            }
        }

        // Invariant 3: no furniture cell may coincide with a stair cell or
        // the air column directly above a stair (head-clearance for ascent).
        // - stair_cells_on_floor: physical stair blocks (Blocked) + landings
        //   (UR) on this floor.
        // - stair_air_above (filtered to this floor): cells one floor above any
        //   stair below; furniture there would head-collide during ascent.
        let this_floor_stair_cells = floor_plan.stair_cells_on_floor(room.floor);
        let this_floor_air_above: HashSet<(i32, i32)> = floor_plan.stair_air_above.iter()
            .filter(|(f, _, _)| *f == room.floor)
            .map(|(_, x, z)| (*x, *z))
            .collect();
        for furn in &room.furniture {
            for &(fx, fz) in &furn.cells {
                if this_floor_stair_cells.contains(&(fx, fz)) || this_floor_air_above.contains(&(fx, fz)) {
                    return Err(format!(
                        "invariant (c): room {:?} floor {} furniture '{}' overlaps stair cell ({},{})",
                        room.room_type, room.floor, furn.name, fx, fz,
                    ));
                }
            }
        }

        // Invariant 2: every BlockedReachable cell has a walkable neighbor.
        for ((cx, cz), state) in room.constraints.iter_ground() {
            if state != CellState::BlockedReachable { continue; }
            let has_walkable = NEIGHBORS.iter().any(|(dx, dz)| {
                room.constraints.is_walkable((cx + dx, cz + dz))
            });
            if !has_walkable {
                let neighbors: Vec<String> = NEIGHBORS.iter().map(|(dx, dz)| {
                    let n = (cx + dx, cz + dz);
                    format!("({},{})={:?}", n.0, n.1, room.constraints.get(n))
                }).collect();
                return Err(format!(
                    "invariant (b): room {:?} floor {} BlockedReachable cell ({},{}) \
                     has no walkable neighbor after furnishing. Neighbors: [{}]",
                    room.room_type, room.floor, cx, cz,
                    neighbors.join(", "),
                ));
            }
        }
    }

    Ok(())
}
