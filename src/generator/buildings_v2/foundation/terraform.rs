use std::collections::HashSet;

use crate::editor::Editor;
use crate::generator::buildings_v2::footprint::Footprint;
use crate::generator::buildings_v2::pipeline::BuildCtx;
use crate::geometry::{Point2D, Point3D};
use crate::minecraft::Block;

/// How many blocks outward from the footprint edge to blend terrain.
const BLEND_RADIUS: i32 = 5;

/// Blend surrounding terrain upward to meet the building's `base_y`.
///
/// For each column within `BLEND_RADIUS` blocks of the footprint perimeter,
/// lerp between `base_y` (at the wall) and the natural terrain height (at
/// max radius). Fills with dirt, topped with the biome surface block.
pub async fn blend_terrain(ctx: &mut BuildCtx<'_>, footprint: &Footprint, base_y: i32) {
    let footprint_set: HashSet<Point2D> = footprint.filled_points().into_iter().collect();
    let world_bounds = ctx.editor.world().world_rect_2d();

    // For each ring distance, collect exterior points and lerp their height.
    for dist in 1..=BLEND_RADIUS {
        let ring = ring_at_distance(&footprint_set, dist, &world_bounds);

        for point in ring {
            let terrain_y = ctx.editor.world().get_ocean_floor_height_at(point);

            // Only raise terrain, never lower it. If terrain is already at or
            // above the lerped target there's nothing to do.
            let t = dist as f64 / BLEND_RADIUS as f64;
            let target_y = lerp_i32(base_y, terrain_y, t);

            if target_y <= terrain_y {
                continue;
            }

            let surface = ctx.editor.world().get_ground_block(point).clone();
            let is_snow = surface.id.as_str().contains("snow");
            let is_sandy = {
                let s = surface.id.as_str();
                s.contains("sand") || s.contains("sandstone")
            };

            let (fill, top) = if is_sandy {
                (
                    Block::from_id("minecraft:sand".into()),
                    Block::from_id("minecraft:sand".into()),
                )
            } else {
                (
                    Block::from_id("minecraft:dirt".into()),
                    Block::from_id("minecraft:grass_block".into()),
                )
            };

            // Convert the old surface to fill material since it's being buried.
            ctx.editor.place_block(&fill, point.add_y(terrain_y - 1)).await;

            // Fill column.
            for y in terrain_y..target_y {
                ctx.editor.place_block(&fill, point.add_y(y)).await;
            }

            // Top block becomes the appropriate surface.
            ctx.editor.place_block(&top, point.add_y(target_y - 1)).await;
            if is_snow {
                ctx.editor.place_block(&surface, point.add_y(target_y)).await;
            }

            // Heightmap = top of solid ground (above grass), excluding snow.
            ctx.editor
                .world_mut()
                .set_heights(&[Point3D::new(point.x, target_y, point.y)].into_iter().collect());
        }
    }
}

/// Collect all points that are exactly `dist` blocks outside the footprint
/// (measured by Chebyshev / chessboard distance).
fn ring_at_distance(
    footprint_set: &HashSet<Point2D>,
    dist: i32,
    world_bounds: &crate::geometry::Rect2D,
) -> Vec<Point2D> {
    // Expand footprint by `dist` and subtract expansion by `dist - 1`.
    let expanded = expand(footprint_set, dist);
    let inner = if dist == 1 {
        footprint_set.clone()
    } else {
        expand(footprint_set, dist - 1)
    };

    expanded
        .difference(&inner)
        .copied()
        .filter(|p| {
            !footprint_set.contains(p) && world_bounds.contains(*p)
        })
        .collect()
}

/// Expand a point set by `radius` in all 4+diagonal directions (Chebyshev).
fn expand(points: &HashSet<Point2D>, radius: i32) -> HashSet<Point2D> {
    let mut result = HashSet::new();
    for &p in points {
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                result.insert(Point2D::new(p.x + dx, p.y + dz));
            }
        }
    }
    result
}

fn lerp_i32(a: i32, b: i32, t: f64) -> i32 {
    (a as f64 + (b as f64 - a as f64) * t).round() as i32
}
