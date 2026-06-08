//! Structural invariant checks for a fully-generated building. Run at the end
//! of the pipeline to catch wall-slot, stair-overlap, and connectivity
//! regressions before the blueprint is trusted.

use std::collections::{HashMap, HashSet};

use crate::geometry::Rect2D;

use super::super::footprint::merge::{concave_corner_cells, walk_edge_cells};
use super::super::footprint::{find_boundaries, phantom_wall_cells};
use super::super::floors::FloorPlan;
use super::super::frame::Frame;
use super::constraints::CellState;
use super::plan::{RoomPlan, RoomRole};

/// Gather all wall cells for a given floor: exterior walls from the building
/// outline plus interior boundary walls from `find_boundaries`.
pub(super) fn wall_cells_on_floor(frame: &Frame, floor: u32) -> HashSet<(i32, i32)> {
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
    // Interior boundary + phantom walls from the per-floor (grown on jettied
    // upper floors) extents, matching where `build_rooms` actually places them
    // and where `compute_room_interior` shrinks the rooms. A no-op when jetty
    // is off, since `rect_at(i, floor)` then equals the ground rect.
    let all_rects = frame.footprint().rects();
    let floor_rects: Vec<Rect2D> = (0..frame.rect_count())
        .map(|i| frame.rect_at(i, floor).unwrap_or(all_rects[i]))
        .collect();
    for b in find_boundaries(&floor_rects) {
        for cell in b.wall_cells {
            cells.insert((cell.x, cell.y));
        }
    }
    let active: Vec<Rect2D> = frame.active_rects(floor).iter().map(|&i| floor_rects[i]).collect();
    for cell in phantom_wall_cells(&active) {
        cells.insert((cell.x, cell.y));
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
