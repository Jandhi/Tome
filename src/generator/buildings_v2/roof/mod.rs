#[cfg(test)]
mod test;

pub mod blocks;
pub mod gable;
pub mod heightmap;

use std::collections::BTreeMap;

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Point2D, Point3D, Rect2D};
use crate::minecraft::{Block, BlockForm};
use crate::noise::RNG;
use super::frame::Frame;
use super::pipeline::BuildCtx;
use blocks::place_roof_blocks;
use gable::{GablePitch, RidgeAxis, gable_heightmap, pick_ridge_axis, place_gable_walls};
use heightmap::RoofHeightmap;

/// Top-level roof style. Determines which roof algorithm runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoofStyle {
    Gable(GablePitch),
    Flat,
}

/// Visual style for flat-roof parapets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParapetStyle {
    /// Alternating full blocks and gaps (classic battlement).
    Crenellated,
    /// 2-block pillars at corners, 1-block walls between.
    CornerPillars,
    /// Thin wall blocks instead of full blocks.
    ThinWalls,
    /// Full blocks at intervals, top slabs filling the gaps between.
    SlabTopped,
}

/// Extend wing rects inward to the core's ridge line for roof heightmap generation.
/// Only extends wings whose ridge axis is perpendicular to the core's ridge axis.
fn extend_rects_for_roof(rects: &[Rect2D], axes: &[RidgeAxis]) -> Vec<Rect2D> {
    if rects.len() <= 1 {
        return rects.to_vec();
    }

    let core = &rects[0];
    let core_axis = axes[0];

    let mut result = rects.to_vec();

    for i in 1..rects.len() {
        let wing = &rects[i];
        let wing_axis = axes[i];

        // Only extend if perpendicular
        if wing_axis == core_axis {
            continue;
        }

        let mut new_min = wing.min();
        let mut new_max = wing.max();

        match core_axis {
            RidgeAxis::X => {
                // Core ridge along X, wing ridge along Z.
                // Wing needs to extend in Z to reach core's Z centerline.
                let core_mid_z = (core.min().y + core.max().y) / 2;

                if wing.min().y > core.max().y {
                    // Wing is south of core
                    new_min = Point2D::new(new_min.x, core_mid_z);
                } else if wing.max().y < core.min().y {
                    // Wing is north of core
                    new_max = Point2D::new(new_max.x, core_mid_z);
                }
            }
            RidgeAxis::Z => {
                // Core ridge along Z, wing ridge along X.
                // Wing needs to extend in X to reach core's X centerline.
                let core_mid_x = (core.min().x + core.max().x) / 2;

                if wing.min().x > core.max().x {
                    // Wing is east of core
                    new_min = Point2D::new(core_mid_x, new_min.y);
                } else if wing.max().x < core.min().x {
                    // Wing is west of core
                    new_max = Point2D::new(core_mid_x, new_max.y);
                }
            }
        }

        result[i] = Rect2D::from_points(new_min, new_max);
    }

    result
}

/// Check which gable ends of a rect should have their overhang suppressed.
/// Only suppress when the gable faces the middle of an adjacent same-height,
/// perpendicular-ridge rect (T-shape junction). Don't suppress at L-shape
/// junctions where the gable meets the end of the neighbor.
/// Returns (suppress_low, suppress_high) along the ridge axis.
fn gable_adjacency(
    rect_idx: usize,
    rects: &[Rect2D],
    axes: &[RidgeAxis],
    roof_ys: &[i32],
) -> (bool, bool) {
    let rect = &rects[rect_idx];
    let ridge_axis = axes[rect_idx];
    let roof_y = roof_ys[rect_idx];
    let min = rect.min();
    let max = rect.max();

    // Check if a same-height, perpendicular-ridge rect is adjacent at the probe
    // AND this rect connects to its middle (the neighbor extends past this rect
    // on both sides along the neighbor's ridge direction).
    let check = |probe: Point2D| -> bool {
        rects.iter().enumerate().any(|(j, r)| {
            if j == rect_idx || roof_ys[j] != roof_y || axes[j] == ridge_axis {
                return false;
            }
            if !r.contains(probe) {
                return false;
            }
            // Check if neighbor extends past this rect on both sides
            // along the neighbor's ridge axis (T-shape, not L-shape)
            match axes[j] {
                RidgeAxis::X => r.min().x < min.x && r.max().x > max.x,
                RidgeAxis::Z => r.min().y < min.y && r.max().y > max.y,
            }
        })
    };

    match ridge_axis {
        RidgeAxis::X => {
            let mid_z = (min.y + max.y) / 2;
            let lo = check(Point2D::new(min.x - 1, mid_z));
            let hi = check(Point2D::new(max.x + 1, mid_z));
            (lo, hi)
        }
        RidgeAxis::Z => {
            let mid_x = (min.x + max.x) / 2;
            let lo = check(Point2D::new(mid_x, min.y - 1));
            let hi = check(Point2D::new(mid_x, max.y + 1));
            (lo, hi)
        }
    }
}

/// Place roofs on all rects of a building.
///
/// Returns gable doorways and per-rect heightmaps. Heightmap `i` is the gable
/// heightmap of `rects[i]` using the extended-roof bounds, suitable for asking
/// "what's the roof block y at this (x, z)?" for furnish-time clearance checks
/// inside attics. For flat roofs the heightmaps are trivial (height 0 everywhere).
pub async fn place_roof(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
    style: RoofStyle,
) -> (Vec<Point2D>, Vec<RoofHeightmap>) {
    match style {
        RoofStyle::Gable(pitch) => place_gable_roof(ctx, frame, pitch).await,
        RoofStyle::Flat => place_flat_roof(ctx, frame).await,
    }
}

async fn place_gable_roof(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
    pitch: GablePitch,
) -> (Vec<Point2D>, Vec<RoofHeightmap>) {
    let editor: &Editor = &*ctx.editor;
    let data = ctx.data;
    let palette = ctx.palette;
    let rng = &mut *ctx.rng;

    let mut gable_doorways = Vec::new();
    let rects = frame.footprint().rects();
    let overhang = 1;

    // Pick ridge axis per rect (store so we don't call RNG twice)
    let rect_axes: Vec<RidgeAxis> = (0..rects.len())
        .map(|i| pick_ridge_axis(&rects[i], rng))
        .collect();

    let roof_rects = extend_rects_for_roof(rects, &rect_axes);

    // Pre-compute roof_y per rect for adjacency checks
    let roof_ys: Vec<i32> = (0..rects.len()).map(|i| frame.roof_y(i)).collect();

    // Suppress gable overhang only where a same-height perpendicular-ridge rect is adjacent
    let gable_suppress: Vec<(bool, bool)> = (0..rects.len())
        .map(|i| gable_adjacency(i, rects, &rect_axes, &roof_ys))
        .collect();

    // Per-rect heightmaps for downstream consumers (chimney, lantern, furnish).
    let per_rect_heightmaps: Vec<RoofHeightmap> = (0..rects.len())
        .map(|i| gable_heightmap(&roof_rects[i], pitch, rect_axes[i], gable_suppress[i]))
        .collect();

    // Group rects by roof_y (BTreeMap keeps keys sorted)
    let mut groups: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
    for i in 0..rects.len() {
        groups.entry(frame.roof_y(i)).or_default().push(i);
    }

    for (&raw_roof_y, group_indices) in &groups {
        let roof_y = match pitch {
            GablePitch::Stairs => raw_roof_y - 1,
            _ => raw_roof_y,
        };
        let group_rects: Vec<&Rect2D> = group_indices.iter().map(|&i| &rects[i]).collect();

        // Rects from higher groups (wall precedence)
        let higher_rects: Vec<&Rect2D> = groups
            .iter()
            .filter(|(&ry, _)| ry > raw_roof_y)
            .flat_map(|(_, indices)| indices.iter().map(|&i| &rects[i]))
            .collect();

        // Combined bounding box using extended rects for heightmap + overhang
        let group_roof_rects: Vec<&Rect2D> = group_indices.iter().map(|&i| &roof_rects[i]).collect();
        let combined_min_x = group_roof_rects.iter().map(|r| r.min().x).min().unwrap() - overhang;
        let combined_min_z = group_roof_rects.iter().map(|r| r.min().y).min().unwrap() - overhang;
        let combined_max_x = group_roof_rects.iter().map(|r| r.max().x).max().unwrap() + overhang;
        let combined_max_z = group_roof_rects.iter().map(|r| r.max().y).max().unwrap() + overhang;
        let width = (combined_max_x - combined_min_x + 1) as usize;
        let depth = (combined_max_z - combined_min_z + 1) as usize;

        let mut combined_hm = RoofHeightmap::new(combined_min_x, combined_min_z, width, depth);

        // Use extended rects for heightmap generation
        for &i in group_indices {
            let sub_hm = gable_heightmap(&roof_rects[i], pitch, rect_axes[i], gable_suppress[i]);
            combined_hm.merge_max(&sub_hm);
        }

        // Place gable wall triangles using original rects
        let all_rects_with_roof_y: Vec<(&Rect2D, i32, RidgeAxis)> = (0..rects.len())
            .map(|i| (&rects[i], frame.roof_y(i), rect_axes[i]))
            .collect();
        for &i in group_indices {
            let doors = place_gable_walls(
                editor, &rects[i], rect_axes[i], pitch, roof_y, &higher_rects,
                &all_rects_with_roof_y, data, palette, rng,
            ).await;
            gable_doorways.extend(doors);
        }

        // Place roof surface and fill (group_rects uses original rects for overhang detection)
        place_roof_blocks(
            editor, &combined_hm, roof_y, pitch, &group_rects, &higher_rects,
            data, palette, rng,
        ).await;
    }

    // For steep (Double) roofs, place hanging lanterns with chains in attic spaces
    if matches!(pitch, GablePitch::Double) {
        for i in 0..rects.len() {
            let roof_y_val = frame.roof_y(i);
            let attic_floor = frame.floor_counts()[i];
            let attic_floor_y = frame.floor_y(attic_floor);
            place_attic_lantern(editor, &rects[i], &per_rect_heightmaps[i], roof_y_val, attic_floor_y).await;
        }
    }

    // Place chimney on one of the tallest rects
    if let Some((&max_roof_y, tallest_indices)) = groups.last_key_value() {
        let roof_y = match pitch {
            GablePitch::Stairs => max_roof_y - 1,
            _ => max_roof_y,
        };
        // Pick a tallest rect
        let rect_idx = tallest_indices[rng.rand_i32_range(0, tallest_indices.len() as i32) as usize];
        let rect = &rects[rect_idx];
        place_chimney(editor, rect, &per_rect_heightmaps[rect_idx], roof_y, pitch, data, palette, rng).await;
    }

    (gable_doorways, per_rect_heightmaps)
}

/// Flat roof: places a solid slab layer at roof_y for each rect, with a
/// 1-block-high parapet wall around the perimeter using PrimaryStone.
async fn place_flat_roof(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
) -> (Vec<Point2D>, Vec<RoofHeightmap>) {
    let editor: &Editor = &*ctx.editor;
    let data = ctx.data;
    let palette = ctx.palette;
    let rng = &mut *ctx.rng;

    let rects = frame.footprint().rects();

    let mut placer_rng = rng.derive();
    let roof_material = palette
        .get_material(MaterialRole::PrimaryRoof)
        .unwrap_or_else(|| palette.get_material(MaterialRole::PrimaryStone).expect("No roof or stone material"))
        .clone();
    let mut roof_placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng),
        roof_material,
    );

    let mut parapet_rng = rng.derive();
    let parapet_material = palette
        .get_material(MaterialRole::PrimaryStone)
        .expect("No primary stone material")
        .clone();
    let mut parapet_placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut parapet_rng),
        parapet_material,
    );

    // Pick a parapet style for this building
    let parapet_style = match rng.rand_i32_range(0, 4) {
        0 => ParapetStyle::Crenellated,
        1 => ParapetStyle::CornerPillars,
        2 => ParapetStyle::ThinWalls,
        _ => ParapetStyle::SlabTopped,
    };

    // Map each point to the roof_y of the tallest rect containing it.
    let mut point_roof_y: std::collections::HashMap<Point2D, i32> = std::collections::HashMap::new();
    for i in 0..rects.len() {
        let ry = frame.roof_y(i);
        for point in rects[i].iter() {
            let entry = point_roof_y.entry(point).or_insert(ry);
            if ry > *entry { *entry = ry; }
        }
    }

    // Collect parapet cells per rect (with their roof_y) before placing,
    // so we can detect corners for CornerPillars style.
    let mut parapet_cells: Vec<(Point2D, i32)> = Vec::new();

    // Place roof blocks and identify parapet cells per rect
    for i in 0..rects.len() {
        let rect = &rects[i];
        let roof_y = frame.roof_y(i);

        for point in rect.iter() {
            // Roof surface: full block replaces where the ceiling would be
            roof_placer
                .place_block(editor, point.add_y(roof_y - 2), BlockForm::Block, None, None)
                .await;

            // Parapet: place on cells at the footprint border OR where a
            // neighbor cell belongs to a lower rect.
            let neighbors = [
                Point2D::new(point.x - 1, point.y),
                Point2D::new(point.x + 1, point.y),
                Point2D::new(point.x, point.y - 1),
                Point2D::new(point.x, point.y + 1),
                Point2D::new(point.x - 1, point.y - 1),
                Point2D::new(point.x + 1, point.y - 1),
                Point2D::new(point.x - 1, point.y + 1),
                Point2D::new(point.x + 1, point.y + 1),
            ];
            let needs_parapet = neighbors.iter().any(|n| {
                match point_roof_y.get(n) {
                    None => true,
                    Some(&ny) => ny < roof_y,
                }
            });

            if needs_parapet {
                parapet_cells.push((point, roof_y));
            }
        }
    }

    // Build a set for fast corner detection
    let parapet_set: std::collections::HashSet<Point2D> =
        parapet_cells.iter().map(|(p, _)| *p).collect();

    // A parapet cell is a corner if it has parapet neighbors on two
    // perpendicular cardinal axes (L-shaped or more).
    let is_corner = |p: Point2D| -> bool {
        let has_x = parapet_set.contains(&Point2D::new(p.x - 1, p.y))
                 || parapet_set.contains(&Point2D::new(p.x + 1, p.y));
        let has_z = parapet_set.contains(&Point2D::new(p.x, p.y - 1))
                 || parapet_set.contains(&Point2D::new(p.x, p.y + 1));
        has_x && has_z
    };

    // Place parapet blocks according to style
    use std::collections::HashMap;
    for &(point, roof_y) in &parapet_cells {
        let checkerboard = (point.x + point.y) % 2 == 0;
        match parapet_style {
            ParapetStyle::Crenellated => {
                // Base block always, slab on top of alternating cells
                parapet_placer
                    .place_block(editor, point.add_y(roof_y - 1), BlockForm::Block, None, None)
                    .await;
                if checkerboard {
                    parapet_placer
                        .place_block(editor, point.add_y(roof_y), BlockForm::Slab, None, None)
                        .await;
                }
            }
            ParapetStyle::CornerPillars => {
                // Always 1 block; corners get a slab on top
                parapet_placer
                    .place_block(editor, point.add_y(roof_y - 1), BlockForm::Block, None, None)
                    .await;
                if is_corner(point) {
                    parapet_placer
                        .place_block(editor, point.add_y(roof_y), BlockForm::Slab, None, None)
                        .await;
                }
            }
            ParapetStyle::ThinWalls => {
                // Thin wall blocks; corners get full blocks for connection
                if is_corner(point) {
                    parapet_placer
                        .place_block(editor, point.add_y(roof_y - 1), BlockForm::Block, None, None)
                        .await;
                } else {
                    parapet_placer
                        .place_block(editor, point.add_y(roof_y - 1), BlockForm::Wall, None, None)
                        .await;
                }
            }
            ParapetStyle::SlabTopped => {
                // Full blocks at intervals, slabs between
                if checkerboard {
                    parapet_placer
                        .place_block(editor, point.add_y(roof_y - 1), BlockForm::Block, None, None)
                        .await;
                } else {
                    let slab_state = HashMap::from([("type".to_string(), "top".to_string())]);
                    parapet_placer
                        .place_block(editor, point.add_y(roof_y - 1), BlockForm::Slab, Some(&slab_state), None)
                        .await;
                }
            }
        }
    }

    // Return trivial heightmaps (height 0)
    let per_rect_heightmaps: Vec<RoofHeightmap> = rects.iter().map(|rect| {
        let min = rect.min();
        let max = rect.max();
        let width = (max.x - min.x + 1) as usize;
        let depth = (max.y - min.y + 1) as usize;
        RoofHeightmap::new(min.x, min.y, width, depth)
    }).collect();

    (Vec::new(), per_rect_heightmaps)
}

/// Place a ladder from the top floor up to the flat roof.
/// Picks a wall-adjacent cell that doesn't conflict with stairs.
/// Marks the ladder cell as UnblockedReachable on the top floor.
/// Returns the wall cell behind the ladder (if any) so callers can exclude
/// it from window placement.
pub async fn place_roof_ladder(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
    floor_plan: &super::floors::FloorPlan,
    room_plan: &mut super::rooms::RoomPlan,
) -> Option<(i32, i32)> {
    use super::rooms::CellState;
    let editor: &Editor = &*ctx.editor;
    let rects = frame.footprint().rects();

    let tallest_rect_idx = (0..rects.len())
        .max_by_key(|&i| frame.floor_counts()[i])
        .unwrap_or(0);
    let tallest_rect = &rects[tallest_rect_idx];
    let roof_y = frame.roof_y(tallest_rect_idx);
    let top_floor = frame.floor_counts()[tallest_rect_idx] - 1;
    let top_floor_y = frame.floor_y(top_floor);

    // Cells to avoid: stair blocks on the top floor + stair air above
    let stair_cells = floor_plan.stair_cells_on_floor(top_floor);
    let stair_avoid: std::collections::HashSet<(i32, i32)> = stair_cells.iter().copied()
        .chain(floor_plan.stair_air_above.iter()
            .filter(|(f, _, _)| *f == top_floor)
            .map(|(_, x, z)| (*x, *z)))
        .chain(floor_plan.stair_tops.iter()
            .filter(|(f, _, _)| *f == top_floor)
            .map(|(_, x, z)| (*x, *z)))
        .chain(floor_plan.stair_bottoms.iter()
            .filter(|(f, _, _)| *f == top_floor)
            .map(|(_, x, z)| (*x, *z)))
        .collect();

    // Parapet cells: cells on the edge of this rect at this roof level
    let footprint_set: std::collections::HashSet<Point2D> =
        frame.footprint().filled_points().into_iter().collect();
    let parapet_set: std::collections::HashSet<Point2D> = tallest_rect.iter()
        .filter(|p| {
            [Point2D::new(p.x-1,p.y), Point2D::new(p.x+1,p.y),
             Point2D::new(p.x,p.y-1), Point2D::new(p.x,p.y+1),
             Point2D::new(p.x-1,p.y-1), Point2D::new(p.x+1,p.y-1),
             Point2D::new(p.x-1,p.y+1), Point2D::new(p.x+1,p.y+1)]
                .iter().any(|n| !footprint_set.contains(n))
        })
        .collect();

    // Candidates: interior cells adjacent to a parapet wall, not on stairs.
    // Ladder faces toward the wall (inward-facing, back against the wall).
    // Returns (ladder_pos, wall_pos, facing).
    let interior_set: std::collections::HashSet<Point2D> = tallest_rect.iter()
        .filter(|p| !parapet_set.contains(p))
        .collect();
    let mut candidates: Vec<(Point2D, Point2D, &str)> = interior_set.iter()
        .filter(|p| !stair_avoid.contains(&(p.x, p.y)))
        .filter_map(|&p| {
            if parapet_set.contains(&Point2D::new(p.x + 1, p.y)) { Some((p, Point2D::new(p.x + 1, p.y), "west")) }
            else if parapet_set.contains(&Point2D::new(p.x - 1, p.y)) { Some((p, Point2D::new(p.x - 1, p.y), "east")) }
            else if parapet_set.contains(&Point2D::new(p.x, p.y + 1)) { Some((p, Point2D::new(p.x, p.y + 1), "north")) }
            else if parapet_set.contains(&Point2D::new(p.x, p.y - 1)) { Some((p, Point2D::new(p.x, p.y - 1), "south")) }
            else { None }
        })
        .collect();

    // Prefer ladder positions away from the building's corners so the ladder
    // hugs the middle of an exterior wall instead of cutting in at a far edge.
    let corners = [
        tallest_rect.min(),
        Point2D::new(tallest_rect.max().x, tallest_rect.min().y),
        Point2D::new(tallest_rect.min().x, tallest_rect.max().y),
        tallest_rect.max(),
    ];
    candidates.sort_by_key(|(pos, _, _)| {
        let min_corner_dist = corners.iter()
            .map(|c| (pos.x - c.x).abs() + (pos.y - c.y).abs())
            .min()
            .unwrap_or(0);
        std::cmp::Reverse(min_corner_dist)
    });

    let (ladder_pos, wall_pos, facing) = if let Some(&(pos, wall, facing)) = candidates.first() {
        (pos, wall, facing)
    } else {
        return None;
    };

    // Place ladder from top floor up through the roof slab
    for y in top_floor_y..(roof_y - 1) {
        let mut ladder = Block::from_id("minecraft:ladder".into());
        ladder.state = Some(std::collections::HashMap::from([
            ("facing".to_string(), facing.to_string()),
        ]));
        editor.place_block_forced(&ladder, Point3D::new(ladder_pos.x, y, ladder_pos.y)).await;
    }

    // Mark ladder cell as UnblockedReachable on the top floor
    for room in &mut room_plan.rooms {
        if room.floor == top_floor && room.rect_index == tallest_rect_idx {
            if room.interior.contains(ladder_pos) {
                room.constraints.set((ladder_pos.x, ladder_pos.y), CellState::UnblockedReachable);
            }
        }
    }

    Some((wall_pos.x, wall_pos.y))
}

/// Place a hanging lantern with chains in the center of an attic rect.
/// Chains go from the roof surface down, with a lantern at the bottom.
async fn place_attic_lantern(
    editor: &Editor,
    rect: &Rect2D,
    hm: &RoofHeightmap,
    roof_y: i32,
    attic_floor_y: i32,
) {
    let center = rect.midpoint();
    let h = hm.get(center.x, center.y);
    if h == f32::NEG_INFINITY || h <= 0.0 { return; }

    // The roof surface at the center
    let roof_surface_y = roof_y + h.floor() as i32;

    // Lantern hangs 1 block above the attic floor
    let lantern_y = attic_floor_y + 2;
    if lantern_y >= roof_surface_y { return; }

    let chain = Block::from_id("minecraft:iron_chain".into());
    use std::collections::HashMap;
    let lantern = Block::new("minecraft:lantern".into(), Some(HashMap::from([("hanging".into(), "true".into())])), None);

    // Chains from just below the roof surface down to above the lantern
    for y in (lantern_y + 1)..(roof_surface_y - 1) {
        editor.place_block_forced(&chain, Point3D::new(center.x, y, center.y)).await;
    }

    // Lantern at the bottom
    editor.place_block_forced(&lantern, Point3D::new(center.x, lantern_y, center.y)).await;
}

/// Place a chimney on the roof of a rect.
/// Finds a position that's not on the edge and not on the ridge,
/// builds a column of roof blocks, tops with a campfire,
/// and surrounds the campfire with dark oak shelves.
async fn place_chimney(
    editor: &Editor,
    rect: &Rect2D,
    hm: &RoofHeightmap,
    roof_y: i32,
    pitch: GablePitch,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
) {
    let min = rect.min();
    let max = rect.max();

    // Chimney height above the roof surface depends on pitch
    let chimney_height: i32 = match pitch {
        GablePitch::Slab => 1,
        GablePitch::Stairs => 2,
        GablePitch::Double => 3,
    };

    // Find candidates: inside the rect (not edge), not on the ridge, not at the eave
    let mut candidates: Vec<(i32, i32, i32)> = Vec::new(); // (x, z, surface_y)
    for x in (min.x + 1)..max.x {
        for z in (min.y + 1)..max.y {
            let h = hm.get(x, z);
            if h == f32::NEG_INFINITY || h <= 0.0 {
                continue;
            }
            // Not on the ridge
            if blocks::is_ridge(hm, x, z) {
                continue;
            }
            let h_adj = if matches!(pitch, GablePitch::Slab) { h - 0.5 } else { h };
            let y_floor = roof_y + h_adj.floor() as i32;
            candidates.push((x, z, y_floor));
        }
    }

    if candidates.is_empty() {
        return;
    }

    // Pick a random candidate
    let idx = rng.rand_i32_range(0, candidates.len() as i32) as usize;
    let (cx, cz, surface_y) = candidates[idx];

    // Roof material for the chimney column
    let roof_material_id = palette
        .get_material(MaterialRole::PrimaryRoof)
        .expect("No primary roof material")
        .clone();
    let mut placer_rng = rng.derive();
    let mut placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng),
        roof_material_id,
    );

    // Build chimney column: full roof blocks going upward from the surface
    for dy in 0..chimney_height {
        placer.place_block(
            editor,
            Point3D::new(cx, surface_y + dy, cz),
            BlockForm::Block,
            None,
            None,
        ).await;
    }

    let top_y = surface_y + chimney_height;

    // Place campfire on top
    editor.place_block_forced(
        &Block::from_id("minecraft:campfire".into()),
        Point3D::new(cx, top_y, cz),
    ).await;

    // Surround the campfire with dark oak shelves facing outward
    use std::collections::HashMap;
    for (dx, dz, facing) in [
        (1, 0, "east"), (-1, 0, "west"), (0, 1, "south"), (0, -1, "north"),
    ] {
        let mut shelf = Block::from_id("minecraft:dark_oak_shelf".into());
        shelf.state = Some(HashMap::from([
            ("facing".to_string(), facing.to_string()),
        ]));
        editor.place_block_forced(&shelf, Point3D::new(cx + dx, top_y, cz + dz)).await;
    }
}
