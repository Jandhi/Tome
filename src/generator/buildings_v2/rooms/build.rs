//! Room construction: partitions the frame into rooms, places interior and
//! phantom walls (leaving archway gaps), and seeds each room's constraint map
//! with stair, door, and ceiling cells. Non-attic room types are assigned here;
//! attic types are deferred until the roof and ladders are in place.

use std::collections::{HashMap, HashSet};

use crate::editor::Editor;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Placer};
use crate::geometry::{Point2D, Point3D, Rect2D};
use crate::minecraft::BlockForm;

use super::super::footprint::merge::{concave_corner_cells, walk_edge_cells};
use super::super::footprint::{SizeClass, find_boundaries, phantom_wall_cells};
use super::super::floors::{FloorPlan, StairKind};
use super::super::frame::Frame;
use super::super::pipeline::BuildCtx;
use super::super::walls::{self, WallSegments};
use super::super::RoomType;
use super::assign::{RoomBudget, pick_room_type, wing_ranks};
use super::constraints::{CellState, ConstraintMap};
use super::plan::{Room, RoomPlan, RoomRole, compute_room_interior, nearest_interior_cell};

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
        // The room is one cell inward from the door. `seg.facing` is already the
        // wall's INWARD normal, so add it directly (negating lands outside).
        let inward: Point2D = seg.facing.into();
        let interior_cell = door_cell + inward;

        for (i, rect) in rects.iter().enumerate() {
            if rect.contains(interior_cell) {
                return Some(i);
            }
        }
    }
    None
}

/// Find an archway position that doesn't conflict with stairwells.
/// Tries the center first. If blocked, picks the wall corner (index 0 or last)
/// that is furthest from any stair cell, then searches inward from that end.
///
/// Only the two cells *perpendicular* to the wall (one in each adjacent room)
/// are checked — those are what a player steps through to use the door. A stair
/// running *alongside* the wall doesn't block the doorway, so cells parallel to
/// the wall are ignored; this leaves far more slots usable and avoids burying a
/// door in a stair just because the stair sits beside the gap.
fn find_archway_pos(
    interior_cells: &[&Point2D],
    stair_cells: &HashSet<(i32, i32)>,
) -> usize {
    let len = interior_cells.len();
    if len == 0 {
        return 0;
    }
    let center = len / 2;

    // Perpendicular approach offsets, derived from the wall's run. The wall is
    // colinear; if x is constant it runs along z (approaches are east/west),
    // otherwise it runs along x (approaches are north/south). A single-cell wall
    // gives no orientation, so check all four neighbours to stay safe.
    let perp: &[(i32, i32)] = if len < 2 {
        &[(1, 0), (-1, 0), (0, 1), (0, -1)]
    } else if interior_cells[0].x == interior_cells[len - 1].x {
        &[(1, 0), (-1, 0)]
    } else {
        &[(0, 1), (0, -1)]
    };

    // How many of a slot's approach cells a stair sits on (0 = fully clear).
    let blocked_sides = |idx: usize| -> usize {
        let cell = interior_cells[idx];
        perp.iter()
            .filter(|(dx, dz)| stair_cells.contains(&(cell.x + dx, cell.y + dz)))
            .count()
    };

    if blocked_sides(center) == 0 {
        return center;
    }

    // Distance from a slot to the nearest stair cell (larger = further away).
    let min_stair_dist = |idx: usize| -> i32 {
        let cell = interior_cells[idx];
        stair_cells.iter()
            .map(|&(sx, sz)| (cell.x - sx).abs() + (cell.y - sz).abs())
            .min()
            .unwrap_or(i32::MAX)
    };

    let start_dist = min_stair_dist(0);
    let end_dist = min_stair_dist(len - 1);

    // Search from the corner furthest from stairs toward the other end.
    let order: Vec<usize> = if end_dist >= start_dist {
        (0..len).rev().collect()
    } else {
        (0..len).collect()
    };

    for &idx in &order {
        if blocked_sides(idx) == 0 {
            return idx;
        }
    }

    // No fully clear slot exists: fall back to the least-blocked one (one open
    // side beats none), tie-broken by greatest distance from any stair — so the
    // door ends up beside a stair at worst, never buried in it.
    order
        .iter()
        .copied()
        .min_by_key(|&idx| (blocked_sides(idx), -min_stair_dist(idx)))
        .unwrap_or(center)
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

    // Per-floor extents (grown on jettied upper floors). Interior walls must be
    // placed from these — not the ground rects — so partition/phantom walls land
    // where `compute_room_interior` shrinks the (grown) rooms. When jetty is off
    // `rect_at(i, floor)` equals the ground rect, so this is a no-op.
    let floor_rects_at = |floor: u32| -> Vec<Rect2D> {
        (0..frame.rect_count())
            .map(|i| frame.rect_at(i, floor).unwrap_or(rects[i]))
            .collect()
    };

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
                    StairKind::Straight => sw.positions.len().saturating_sub(1),
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
        let floor_rects = floor_rects_at(floor);
        let boundaries = find_boundaries(&floor_rects);

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


        // Phantom walls plug the partial-shared edge gap (see
        // `phantom_wall_cells`). No doorway carved — these are corner-pillar
        // continuations of the adjacent boundary wall, not their own room
        // boundary.
        let active_rects: Vec<Rect2D> = active.iter().map(|&i| floor_rects[i]).collect();
        let base_y = frame.floor_y(floor);
        let height = frame.wall_height();
        for cell in phantom_wall_cells(&active_rects) {
            if perimeter.contains(&(cell.x, cell.y)) { continue; }
            for ry in 0..height {
                placer.place_block(
                    editor,
                    Point3D::new(cell.x, base_y + ry as i32, cell.y),
                    BlockForm::Block,
                    None,
                    None,
                ).await;
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

        // Per-floor extents: for jettied buildings the rect on floor ≥ 1 is grown
        // by 1 on each side, and the room's interior must shrink from that grown
        // extent (otherwise walls land one cell outside the room and the wall-
        // adjacency invariant fails). Attic floors reuse the top regular extent.
        let extent_at_floor = |i: usize, f: u32| -> Rect2D {
            if f < frame.floor_counts()[i] {
                frame.rect_at(i, f).unwrap_or(rects[i])
            } else {
                frame.rect_at_top(i).unwrap_or(rects[i])
            }
        };
        let floor_rects: Vec<Rect2D> = (0..frame.rect_count())
            .map(|i| extent_at_floor(i, floor))
            .collect();
        let rect = floor_rects[rect_idx];
        let interior = compute_room_interior(&floor_rects, rect_idx);
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
