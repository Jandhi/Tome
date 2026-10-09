//! Gable roofs: ridge-axis selection, wing extension and T-junction overhang
//! suppression, the per-group surface/wall placement, and the chimney + attic
//! lantern detailing that rides on top.

use std::collections::BTreeMap;

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Point2D, Point3D, Rect2D};
use crate::minecraft::{Block, BlockForm};
use crate::noise::RNG;

use super::super::frame::Frame;
use super::super::pipeline::BuildCtx;
use super::blocks::{self, place_roof_blocks};
use super::gable::{GablePitch, RidgeAxis, gable_heightmap, pick_ridge_axis, place_gable_walls};
use super::heightmap::RoofHeightmap;
use super::top_floor_rects;

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

/// Clear the overhang cells past rect `i`'s two gable ends (the row beyond
/// each end, corners included) that fall inside another rect of the same roof
/// height. The roof over those cells belongs to that rect: where it is lower
/// (a narrower wing meeting the core end to end, or a wing's eave running
/// along the core's gable) the overhang would float one column past the gable
/// wall with an open pocket under it, and where it is higher the overhang is
/// buried anyway. A T-junction wing's extended end lies entirely inside the
/// core, so this also stops its roof at the core's ridge line. Eave overhang
/// cells inside another rect never show (they sit below that rect's eave) and
/// are left alone.
pub(super) fn clear_covered_gable_overhang(
    hm: &mut RoofHeightmap,
    i: usize,
    roof_rect: &Rect2D,
    axis: RidgeAxis,
    rects: &[Rect2D],
    roof_ys: &[i32],
) {
    let (min, max) = (roof_rect.min(), roof_rect.max());
    let end_cells: Vec<Point2D> = match axis {
        RidgeAxis::X => (min.y - 1..=max.y + 1)
            .flat_map(|z| [Point2D::new(min.x - 1, z), Point2D::new(max.x + 1, z)])
            .collect(),
        RidgeAxis::Z => (min.x - 1..=max.x + 1)
            .flat_map(|x| [Point2D::new(x, min.y - 1), Point2D::new(x, max.y + 1)])
            .collect(),
    };
    for p in end_cells {
        let covered = rects.iter().enumerate().any(|(j, r)| j != i && roof_ys[j] == roof_ys[i] && r.contains(p));
        if covered {
            hm.set(p.x, p.y, f32::NEG_INFINITY);
        }
    }
}

pub(super) async fn place_gable_roof(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
    pitch: GablePitch,
) -> (Vec<Point2D>, Vec<RoofHeightmap>) {
    let editor: &Editor = &*ctx.editor;
    let data = ctx.data;
    let palette = ctx.palette;
    let rng = &mut *ctx.rng;

    let mut gable_doorways = Vec::new();
    let rects = top_floor_rects(frame);
    let rects = &rects[..];
    let overhang = 1;

    // Pick ridge axis per rect (store so we don't call RNG twice)
    let rect_axes: Vec<RidgeAxis> = (0..rects.len())
        .map(|i| pick_ridge_axis(&rects[i], rng))
        .collect();

    let roof_rects = extend_rects_for_roof(rects, &rect_axes);

    // Pre-compute roof_y per rect for adjacency checks
    let roof_ys: Vec<i32> = (0..rects.len()).map(|i| frame.roof_y(i)).collect();

    // Per-rect heightmaps: merged into each height group's roof below, and
    // handed to downstream consumers (chimney, lantern, furnish).
    let per_rect_heightmaps: Vec<RoofHeightmap> = (0..rects.len())
        .map(|i| {
            let mut hm = gable_heightmap(&roof_rects[i], pitch, rect_axes[i]);
            clear_covered_gable_overhang(&mut hm, i, &roof_rects[i], rect_axes[i], rects, &roof_ys);
            hm
        })
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

        for &i in group_indices {
            combined_hm.merge_max(&per_rect_heightmaps[i]);
        }

        // Place gable wall triangles using original rects
        let all_rects_with_roof_y: Vec<(&Rect2D, i32, RidgeAxis)> = (0..rects.len())
            .map(|i| (&rects[i], frame.roof_y(i), rect_axes[i]))
            .collect();
        for &i in group_indices {
            let doors = place_gable_walls(
                editor, &rects[i], rect_axes[i], pitch, roof_y, frame.knee_for_rect(i) as i32, &higher_rects,
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

    // Chimney height above the roof surface depends on pitch.
    // Slab pitch needs +1 because its surface_y sits one block lower than
    // the adjacent up-slope slab, so shelves at surface_y+1 would overwrite it.
    let chimney_height: i32 = match pitch {
        GablePitch::Slab => 2,
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
