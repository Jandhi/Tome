//! Attic access. Works out which attic rects already reach a stair (directly or
//! via gable doorways / shared boundaries) and drops a corner ladder into any
//! that don't, marking the ladder column reachable on both floors.

use std::collections::{HashMap, HashSet};

use crate::editor::Editor;
use crate::geometry::{Point2D, Point3D, Rect2D};
use crate::minecraft::Block;

use super::super::footprint::find_boundaries;
use super::super::frame::Frame;
use super::super::floors::FloorPlan;
use super::super::pipeline::BuildCtx;
use super::super::walls::{self, WallSegments};
use super::constraints::CellState;
use super::plan::{RoomPlan, RoomRole};

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
    // Attic occupies the same XZ as the top regular floor of each rect — use the
    // jettied extent if jetty is set, ground extent otherwise.
    let rects: Vec<Rect2D> = (0..frame.rect_count())
        .map(|i| frame.rect_at_top(i).expect("rect must exist at its top floor"))
        .collect();

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
    let boundaries = find_boundaries(&rects);
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
