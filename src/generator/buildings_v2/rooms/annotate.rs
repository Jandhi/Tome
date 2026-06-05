//! Post-placement constraint annotation: marks gable doorway and window cells
//! in room constraint maps once those openings exist in the wall/roof geometry.

use crate::geometry::Point2D;

use super::super::walls::{self, WallSegments};
use super::constraints::CellState;
use super::plan::{RoomPlan, RoomRole, nearest_interior_cell};

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
