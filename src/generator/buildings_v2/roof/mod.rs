#[cfg(test)]
mod test;

pub mod blocks;
pub mod gable;
pub mod heightmap;

use std::collections::HashMap;

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::Palette;
use crate::geometry::{Point2D, Rect2D};
use crate::noise::RNG;
use super::frame::Frame;
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

/// Place roofs on all rects of a building.
/// Groups rects by roof_y, generates per-rect gable heightmaps, merges with max per group,
/// then places blocks. Lower roofs skip positions inside higher-floor rects.
pub async fn place_roof(
    editor: &Editor,
    frame: &Frame,
    pitch: GablePitch,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
) {
    let rects = frame.footprint().rects();
    let overhang = 1;

    // Pick ridge axis per rect (store so we don't call RNG twice)
    let rect_axes: Vec<RidgeAxis> = (0..rects.len())
        .map(|i| pick_ridge_axis(&rects[i], rng))
        .collect();

    let roof_rects = extend_rects_for_roof(rects, &rect_axes);

    // Group rects by roof_y
    let mut groups: HashMap<i32, Vec<usize>> = HashMap::new();
    for i in 0..rects.len() {
        groups.entry(frame.roof_y(i)).or_default().push(i);
    }

    let mut sorted_roof_ys: Vec<i32> = groups.keys().cloned().collect();
    sorted_roof_ys.sort();

    for &raw_roof_y in &sorted_roof_ys {
        let roof_y = match pitch {
            GablePitch::Stairs => raw_roof_y - 1,
            _ => raw_roof_y,
        };
        let group_indices = &groups[&raw_roof_y];
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
            let sub_hm = gable_heightmap(&roof_rects[i], pitch, rect_axes[i]);
            combined_hm.merge_max(&sub_hm);
        }

        // Place gable wall triangles using original rects
        for &i in group_indices {
            place_gable_walls(
                editor, &rects[i], rect_axes[i], pitch, roof_y, &higher_rects,
                data, palette, rng,
            ).await;
        }

        // Place roof surface and fill (group_rects uses original rects for overhang detection)
        place_roof_blocks(
            editor, &combined_hm, roof_y, pitch, &group_rects, &higher_rects,
            data, palette, rng,
        ).await;
    }
}
