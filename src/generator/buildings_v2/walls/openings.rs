//! Door and window placement. First the planning passes that cut openings into
//! wall segments (`place_doors`, `place_terrace_doors`, `place_windows`), then
//! `place_openings`, which renders the planned openings as actual door, glass,
//! trapdoor, and arch blocks.

use std::collections::{HashMap, HashSet};

use crate::editor::Editor;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Placer};
use crate::geometry::{Cardinal, Point2D, Point3D, Rect2D};
use crate::minecraft::{Block, BlockForm};
use crate::noise::RNG;

use super::super::frame::Frame;
use super::super::pipeline::BuildCtx;
use super::segments::{
    DoorStyle, Opening, OpeningKind, WallSegments, WindowStyle, segment_cells,
};

/// What fills a window opening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowFill {
    Glass,
    Trapdoor,
    Open,
}

/// Distance from a point to the nearest edge of a rectangle.
fn distance_to_plot_edge(point: Point2D, plot_bounds: &Rect2D) -> i32 {
    let min = plot_bounds.min();
    let max = plot_bounds.max();
    let dx_min = (point.x - min.x).abs();
    let dx_max = (point.x - max.x).abs();
    let dz_min = (point.y - min.y).abs();
    let dz_max = (point.y - max.y).abs();
    dx_min.min(dx_max).min(dz_min).min(dz_max)
}

/// Midpoint of a wall segment in world coords.
fn segment_midpoint(seg: &super::segments::WallSegment) -> Point2D {
    Point2D::new(
        (seg.start.x + seg.end.x) / 2,
        (seg.start.y + seg.end.y) / 2,
    )
}

/// Place doors on ground-floor segments. Picks the segment closest to the plot
/// edge (likely road-facing). For large buildings, adds a second door on the
/// opposite side.
pub fn place_doors(wall_segs: &mut WallSegments, plot_bounds: &Rect2D, footprint_area: i32, boundary_cells: &HashSet<Point2D>, _rng: &mut RNG) {
    let door_style = if footprint_area > 150 {
        DoorStyle::Double
    } else {
        DoorStyle::Single
    };
    let door_width = match door_style {
        DoorStyle::Single => 1,
        DoorStyle::Double => 2,
        DoorStyle::Archway => 2,
    };
    let min_segment_len = door_width as i32 + 4; // 2 blocks margin each side

    // Helper: true if a segment overlaps interior boundary cells (where archways go)
    let is_on_boundary = |i: usize| {
        segment_cells(&wall_segs.segments[i]).iter().any(|c| boundary_cells.contains(c))
    };

    // Score ground-floor segments by distance to nearest plot edge (lower = better)
    // Skip segments that overlap with interior boundary cells (where dividing walls go).
    let mut scored: Vec<(usize, i32)> = wall_segs
        .segments
        .iter()
        .enumerate()
        .filter(|(i, s)| s.floor == 0 && s.length >= min_segment_len && !is_on_boundary(*i))
        .map(|(i, s)| {
            let mid = segment_midpoint(s);
            let dist = distance_to_plot_edge(mid, plot_bounds);
            (i, dist)
        })
        .collect();

    scored.sort_by_key(|&(_, dist)| dist);

    if scored.is_empty() {
        return;
    }

    // Place primary door on the closest segment
    let primary_idx = scored[0].0;
    let primary_offset = (wall_segs.segments[primary_idx].length as u32 - door_width) / 2;
    wall_segs.segments[primary_idx].openings.push(Opening {
        kind: OpeningKind::Door(door_style),
        offset: primary_offset,
        width: door_width,
        height: 2,
        y_offset: 0,
    });

    // For larger buildings, place a second door on the opposite wall.
    // Falls back to any non-adjacent facing if opposite isn't available.
    if footprint_area > 100 {
        let primary_facing = wall_segs.segments[primary_idx].facing;
        let opposite = -primary_facing;
        if let Some(&(idx, _)) = scored.iter()
            .find(|&&(i, _)| wall_segs.segments[i].facing == opposite)
            .or_else(|| scored.iter().find(|&&(i, _)| {
                wall_segs.segments[i].facing != primary_facing
            }))
        {
            let offset = (wall_segs.segments[idx].length as u32 - door_width) / 2;
            wall_segs.segments[idx].openings.push(Opening {
                kind: OpeningKind::Door(DoorStyle::Single),
                offset,
                width: 1,
                height: 2,
                y_offset: 0,
            });
        }
    }
}

/// Collect all interior boundary cells between adjacent rects into a HashSet.
/// Used to prevent side doors from overlapping with interior archway positions.
pub fn boundary_cell_set(rects: &[Rect2D]) -> HashSet<Point2D> {
    use super::super::footprint::find_boundaries;
    find_boundaries(rects)
        .into_iter()
        .flat_map(|b| b.wall_cells)
        .collect()
}

/// Place doors from upper floors onto the flat roof of a shorter adjacent rect.
/// For each boundary between rects with different floor counts, the shorter
/// rect's roof aligns with a floor of the taller rect. A door is placed in the
/// taller rect's wall at that floor, giving access to the rooftop terrace.
///
/// Returns the 2D positions of the door cells on the roof surface (for parapet
/// exclusion and room/cell-state marking).
pub fn place_terrace_doors(
    wall_segs: &mut WallSegments,
    frame: &Frame,
) -> Vec<Point2D> {
    use super::super::footprint::find_boundaries;
    let rects = frame.footprint().rects();
    let boundaries = find_boundaries(rects);
    let mut terrace_door_cells = Vec::new();

    for boundary in &boundaries {
        let fc_a = frame.floor_counts()[boundary.rect_a];
        let fc_b = frame.floor_counts()[boundary.rect_b];
        if fc_a == fc_b { continue; }

        // The shorter rect's roof is accessed from the taller rect's floor
        // at index == shorter's floor count.
        let (short_fc, _tall_fc) = if fc_a < fc_b { (fc_a, fc_b) } else { (fc_b, fc_a) };
        let door_floor = short_fc; // floor index on the taller rect

        // Find wall segments on that floor whose cells overlap with boundary cells
        let boundary_set: HashSet<Point2D> = boundary.wall_cells.iter().copied().collect();

        for seg_idx in 0..wall_segs.segments.len() {
            let seg = &wall_segs.segments[seg_idx];
            if seg.floor != door_floor { continue; }

            let cells = segment_cells(seg);
            // Find which cell indices overlap with the boundary
            let overlapping: Vec<usize> = cells.iter().enumerate()
                .filter(|(_, c)| boundary_set.contains(c))
                .map(|(i, _)| i)
                .collect();

            if overlapping.is_empty() { continue; }

            // Place a single door centered on the overlap
            let mid_idx = overlapping[overlapping.len() / 2];
            let offset = mid_idx as u32;

            // Check segment is long enough and door doesn't overlap existing openings
            if offset == 0 || offset >= seg.length as u32 - 1 { continue; }

            let overlaps_existing = seg.openings.iter().any(|o| {
                let o_start = o.offset.saturating_sub(1);
                let o_end = o.offset + o.width + 1;
                offset + 1 > o_start && offset < o_end
            });
            if overlaps_existing { continue; }

            wall_segs.segments[seg_idx].openings.push(Opening {
                kind: OpeningKind::Door(DoorStyle::Single),
                offset,
                width: 1,
                height: 2,
                y_offset: 0,
            });

            // The door cell on the roof surface
            terrace_door_cells.push(cells[mid_idx]);
        }
    }

    terrace_door_cells
}

/// Place windows on all wall segments. Even spacing, denser on upper floors.
/// Skips positions that overlap with existing doors.
pub fn place_windows(
    wall_segs: &mut WallSegments,
    interior_walls: &HashSet<(i32, i32)>,
    _rng: &mut RNG,
) {
    let corner_margin: u32 = 1;

    for seg_idx in 0..wall_segs.segments.len() {
        let seg = &wall_segs.segments[seg_idx];
        let seg_len = seg.length as u32;

        if seg_len < corner_margin * 2 + 1 {
            continue; // too short for any window
        }

        let cells = segment_cells(seg);
        let available = seg_len - corner_margin * 2;

        // Window style based on segment length
        let (style, win_width, win_height) = if available >= 8 {
            (WindowStyle::Arched, 2u32, 2u32)
        } else if available >= 5 {
            (WindowStyle::Decorated, 1, 2)
        } else {
            (WindowStyle::Small, 1u32, 1u32)
        };

        let spacing = if matches!(style, WindowStyle::Arched) { 5u32 } else { 3u32 };
        let count = available / spacing;
        if count == 0 {
            continue;
        }

        // Evenly distribute
        let stride = available / (count + 1);
        let existing_openings = seg.openings.clone();

        for i in 1..=count {
            let offset = corner_margin + stride * i - win_width / 2;

            // Check overlap with existing doors
            let overlaps = existing_openings.iter().any(|o| {
                let o_start = o.offset.saturating_sub(1);
                let o_end = o.offset + o.width + 1;
                offset + win_width > o_start && offset < o_end
            });
            if overlaps { continue; }

            // Check overlap with interior walls
            let hits_interior = (0..win_width).any(|dx| {
                let idx = (offset + dx) as usize;
                idx < cells.len() && interior_walls.contains(&(cells[idx].x, cells[idx].y))
            });
            if hits_interior { continue; }

            wall_segs.segments[seg_idx].openings.push(Opening {
                kind: OpeningKind::Window(style),
                offset,
                width: win_width,
                height: win_height,
                y_offset: 1,
            });
        }
    }
}

/// Place opening blocks (doors and windows) for all wall segments.
/// Doors use PrimaryWood with Door form and correct facing.
/// Windows use PrimaryWood with Fence form for now.
pub async fn place_openings(
    ctx: &mut BuildCtx<'_>,
    wall_segs: &WallSegments,
    window_fill: WindowFill,
) {
    let editor: &Editor = &*ctx.editor;
    let data = ctx.data;
    let palette = ctx.palette;
    let rng = &mut *ctx.rng;

    let wood_material = palette
        .get_material(MaterialRole::PrimaryWood)
        .expect("No primary wood material")
        .clone();
    let wall_material = palette
        .get_material(MaterialRole::PrimaryWall)
        .unwrap_or_else(|| palette.get_material(MaterialRole::PrimaryStone).expect("No wall or stone material"))
        .clone();

    let mut placer_rng = rng.derive();
    let mut placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng),
        wood_material,
    );
    let mut wall_placer_rng = rng.derive();
    let mut wall_placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut wall_placer_rng),
        wall_material,
    );

    for seg in &wall_segs.segments {
        let cells = segment_cells(seg);

        for opening in &seg.openings {
            match &opening.kind {
                OpeningKind::Door(_style) => {
                    let facing_str = seg.facing.to_string();
                    for dx in 0..opening.width {
                        let idx = (opening.offset + dx) as usize;
                        if idx >= cells.len() { continue; }
                        let cell = cells[idx];

                        let hinge = if dx == 0 { "right" } else { "left" };

                        // Lower half
                        let lower_state = HashMap::from([
                            ("facing".to_string(), facing_str.clone()),
                            ("half".to_string(), "lower".to_string()),
                            ("hinge".to_string(), hinge.to_string()),
                        ]);
                        let y = seg.base_y + opening.y_offset as i32;
                        placer.place_block(
                            editor,
                            Point3D::new(cell.x, y, cell.y),
                            BlockForm::Door,
                            Some(&lower_state),
                            None,
                        ).await;

                        // Upper half
                        let upper_state = HashMap::from([
                            ("facing".to_string(), facing_str.clone()),
                            ("half".to_string(), "upper".to_string()),
                            ("hinge".to_string(), hinge.to_string()),
                        ]);
                        placer.place_block(
                            editor,
                            Point3D::new(cell.x, y + 1, cell.y),
                            BlockForm::Door,
                            Some(&upper_state),
                            None,
                        ).await;
                    }
                }
                OpeningKind::Window(style) => {
                    let is_arched = matches!(style, WindowStyle::Decorated | WindowStyle::Arched);
                    let top_dy = opening.height.saturating_sub(1);

                    for dx in 0..opening.width {
                        let idx = (opening.offset + dx) as usize;
                        if idx >= cells.len() { continue; }
                        let cell = cells[idx];
                        for dy in 0..opening.height {
                            let y = seg.base_y + opening.y_offset as i32 + dy as i32;
                            let pos = Point3D::new(cell.x, y, cell.y);

                            if is_arched && dy == top_dy {
                                let stair_facing = match style {
                                    WindowStyle::Arched => {
                                        let walk_dir = match seg.facing {
                                            Cardinal::South => Cardinal::East,
                                            Cardinal::North => Cardinal::West,
                                            Cardinal::East => Cardinal::North,
                                            Cardinal::West => Cardinal::South,
                                        };
                                        if dx == 0 { -walk_dir } else { walk_dir }
                                    }
                                    _ => seg.facing,
                                };
                                let state = HashMap::from([
                                    ("facing".to_string(), stair_facing.to_string()),
                                    ("half".to_string(), "top".to_string()),
                                ]);
                                wall_placer.place_block_forced(
                                    editor, pos, BlockForm::Stairs,
                                    Some(&state), None,
                                ).await;
                                continue;
                            }

                            match window_fill {
                                WindowFill::Glass => {
                                    editor.place_block_forced(
                                        &Block::from_id("minecraft:glass_pane".into()),
                                        pos,
                                    ).await;
                                }
                                WindowFill::Trapdoor => {
                                    let facing_str = (-seg.facing).to_string();
                                    let state = HashMap::from([
                                        ("facing".to_string(), facing_str),
                                        ("open".to_string(), "true".to_string()),
                                        ("half".to_string(), "bottom".to_string()),
                                    ]);
                                    placer.place_block_forced(
                                        editor, pos, BlockForm::Trapdoor,
                                        Some(&state), None,
                                    ).await;
                                }
                                WindowFill::Open => {
                                    editor.place_block_forced(
                                        &"air".into(),
                                        pos,
                                    ).await;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
