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
/// Groups rects by roof_y, generates per-rect gable heightmaps, merges with max per group,
/// then places blocks. Lower roofs skip positions inside higher-floor rects.
pub async fn place_roof(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
    pitch: GablePitch,
) -> Vec<Point2D> {
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
            let hm = gable_heightmap(&roof_rects[i], pitch, rect_axes[i], gable_suppress[i]);
            place_attic_lantern(editor, &rects[i], &hm, roof_y_val, attic_floor_y).await;
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
        let hm = gable_heightmap(&roof_rects[rect_idx], pitch, rect_axes[rect_idx], gable_suppress[rect_idx]);
        place_chimney(editor, rect, &hm, roof_y, pitch, data, palette, rng).await;
    }

    gable_doorways
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
