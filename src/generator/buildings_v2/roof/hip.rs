use std::collections::HashMap;

use crate::editor::Editor;
use crate::generator::materials::{Material, MaterialId, Palette};
use crate::geometry::{Cardinal, Point3D};
use crate::minecraft::Block;
use crate::noise::RNG;

use super::super::footprint::Footprint;
use super::{Roof, RoofConfig, RoofMaterials, RoofPitch};

/// Place a hip roof on a footprint (supports non-rectangular shapes).
/// Slopes on all sides, meeting at a peak or ridge.
pub async fn place_hip_roof(
    roof: &Roof,
    footprint: &Footprint,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    let config = match &roof.config {
        RoofConfig::Hip(c) => c,
        _ => return, // Not a hip roof
    };

    let (bounds_min, bounds_max) = footprint.bounds().unwrap();
    let overhang = config.overhang;
    let pitch = config.pitch;

    // Calculate roof dimensions with overhang (bounding box for iteration)
    let roof_min_x = bounds_min.x - overhang;
    let roof_max_x = bounds_max.x + overhang;
    let roof_min_z = bounds_min.y - overhang; // Point2D.y is Z
    let roof_max_z = bounds_max.y + overhang;

    let roof_mats = RoofMaterials::from_palette(palette, materials, rng);

    // For each position, check if it's within the footprint (+ overhang)
    for z in roof_min_z..=roof_max_z {
        for x in roof_min_x..=roof_max_x {
            let point = crate::geometry::Point2D::new(x, z);

            // Skip positions outside the footprint's overhang range
            if !footprint.is_within_distance(point, overhang) {
                continue;
            }

            // Calculate distance to nearest edge of the actual footprint
            let edge_dist = footprint.distance_to_edge(point);

            // If inside the footprint, raise by 1 to create consistent slope from overhang
            // Overhang positions stay at distance 0, interior positions start at 1
            let min_dist = if footprint.contains(point) {
                edge_dist + 1
            } else {
                // In overhang area - treat as distance 0 (edge)
                0
            };

            // Determine facing toward the closest edge (outward)
            let facing = find_closest_edge_facing(footprint, point);

            match pitch {
                RoofPitch::Shallow => {
                    place_shallow_hip_block(
                        editor, &roof_mats, roof.base_y, x, z, min_dist,
                    ).await;
                }
                RoofPitch::Medium => {
                    place_medium_hip_block(
                        editor, &roof_mats, roof.base_y, x, z, min_dist, facing,
                    ).await;
                }
                RoofPitch::Steep => {
                    place_steep_hip_block(
                        editor, &roof_mats, roof.base_y, x, z, min_dist, facing,
                    ).await;
                }
            }
        }
    }
}

/// Find which edge of the footprint is closest and return the facing direction.
/// Stairs face TOWARD the closest edge (outward/downslope direction).
fn find_closest_edge_facing(footprint: &Footprint, point: crate::geometry::Point2D) -> Cardinal {
    let mut min_dist = i32::MAX;
    let mut best_facing = Cardinal::North;

    for (start, end) in footprint.edges() {
        let dx = end.x - start.x;
        let dy = end.y - start.y;

        // Calculate distance to this edge
        let dist = point_to_edge_distance(point, start, end);

        if dist < min_dist {
            min_dist = dist;
            // Determine facing toward the edge (outward)
            // Stairs face the direction of the slope (toward the edge)
            if dx == 0 {
                // Vertical edge (runs along Z) - edge has constant x
                let edge_x = start.x;
                if edge_x < point.x {
                    // Edge is to the west, face west (toward edge)
                    best_facing = Cardinal::West;
                } else {
                    // Edge is to the east, face east (toward edge)
                    best_facing = Cardinal::East;
                }
            } else if dy == 0 {
                // Horizontal edge (runs along X) - edge has constant z
                let edge_z = start.y;
                if edge_z < point.y {
                    // Edge is to the north (smaller z), face north
                    best_facing = Cardinal::North;
                } else {
                    // Edge is to the south (larger z), face south
                    best_facing = Cardinal::South;
                }
            }
        }
    }

    best_facing
}

/// Calculate distance from point to an edge segment.
fn point_to_edge_distance(point: crate::geometry::Point2D, seg_start: crate::geometry::Point2D, seg_end: crate::geometry::Point2D) -> i32 {
    let dx = seg_end.x - seg_start.x;
    let dy = seg_end.y - seg_start.y;

    if dx == 0 {
        // Vertical segment
        let min_y = seg_start.y.min(seg_end.y);
        let max_y = seg_start.y.max(seg_end.y);
        if point.y >= min_y && point.y <= max_y {
            return (point.x - seg_start.x).abs();
        }
        let dist_to_start = (point.x - seg_start.x).abs().max((point.y - seg_start.y).abs());
        let dist_to_end = (point.x - seg_end.x).abs().max((point.y - seg_end.y).abs());
        return dist_to_start.min(dist_to_end);
    }

    if dy == 0 {
        // Horizontal segment
        let min_x = seg_start.x.min(seg_end.x);
        let max_x = seg_start.x.max(seg_end.x);
        if point.x >= min_x && point.x <= max_x {
            return (point.y - seg_start.y).abs();
        }
        let dist_to_start = (point.x - seg_start.x).abs().max((point.y - seg_start.y).abs());
        let dist_to_end = (point.x - seg_end.x).abs().max((point.y - seg_end.y).abs());
        return dist_to_start.min(dist_to_end);
    }

    // Non-axis-aligned (rare in our case)
    i32::MAX
}

async fn place_shallow_hip_block(
    editor: &Editor,
    mats: &RoofMaterials,
    base_y: i32,
    x: i32,
    z: i32,
    min_dist: i32,
) {
    let y_offset = min_dist / 2;
    let is_top_slab_row = min_dist % 2 == 1;
    let y = base_y + y_offset;
    let pos = Point3D::new(x, y, z);

    if min_dist == 0 {
        // Edge: just a bottom slab
        let mut state = HashMap::new();
        state.insert("type".to_string(), "bottom".to_string());
        let slab = Block::new(mats.slab.clone(), Some(state), None);
        editor.place_block(&slab, pos).await;
    } else if is_top_slab_row {
        // Top slab rows become full blocks
        let block = Block::from(mats.solid.clone());
        editor.place_block(&block, pos).await;
    } else {
        // Bottom slab rows: place bottom slab + top slab below
        let mut state = HashMap::new();
        state.insert("type".to_string(), "bottom".to_string());
        let slab = Block::new(mats.slab.clone(), Some(state), None);
        editor.place_block(&slab, pos).await;

        // Place top slab one block below
        let pos_below = Point3D::new(x, y - 1, z);
        let mut state_below = HashMap::new();
        state_below.insert("type".to_string(), "top".to_string());
        let slab_below = Block::new(mats.slab.clone(), Some(state_below), None);
        editor.place_block(&slab_below, pos_below).await;
    }
}

async fn place_medium_hip_block(
    editor: &Editor,
    mats: &RoofMaterials,
    base_y: i32,
    x: i32,
    z: i32,
    min_dist: i32,
    facing: Cardinal,
) {
    // Stairs, +1 y per distance
    let y_offset = min_dist;
    let y = base_y + y_offset;
    let pos = Point3D::new(x, y, z);

    let mut state = HashMap::new();
    state.insert("facing".to_string(), facing.to_string());
    let stair = Block::new(mats.stairs.clone(), Some(state), None);
    editor.place_block(&stair, pos).await;
}

async fn place_steep_hip_block(
    editor: &Editor,
    mats: &RoofMaterials,
    base_y: i32,
    x: i32,
    z: i32,
    min_dist: i32,
    facing: Cardinal,
) {
    // Block + stair per distance unit, +2 y per distance
    let y_offset = min_dist * 2;
    let y = base_y + y_offset;

    // Place solid block
    let pos_block = Point3D::new(x, y, z);
    let solid = Block::from(mats.solid.clone());
    editor.place_block(&solid, pos_block).await;

    // Place stair above
    let pos_stair = Point3D::new(x, y + 1, z);
    let mut state = HashMap::new();
    state.insert("facing".to_string(), facing.to_string());
    let stair = Block::new(mats.stairs.clone(), Some(state), None);
    editor.place_block(&stair, pos_stair).await;
}
