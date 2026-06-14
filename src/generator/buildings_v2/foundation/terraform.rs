use std::collections::HashSet;

use crate::editor::Editor;
use crate::generator::BuildClaim;
use crate::generator::buildings_v2::footprint::Footprint;
use crate::generator::buildings_v2::pipeline::BuildCtx;
use crate::geometry::{Point2D, Point3D};

/// How many blocks outward from the footprint edge to blend terrain.
const BLEND_RADIUS: i32 = 5;

/// Over how many cells the blend tapers to zero as it approaches the town wall
/// or a gate: a cell `d` cells from the wall keeps only `d / WALL_FALLOFF` of its
/// raise, reaching 0 displacement at the wall itself so the terraform never
/// disturbs it and meets it flush.
const WALL_FALLOFF: i32 = 4;

/// Chebyshev distance from `p` to the nearest Wall/Gate-claimed cell within
/// `WALL_FALLOFF`, or `None` if none is that close (no taper needed).
fn wall_proximity(editor: &Editor, p: Point2D) -> Option<i32> {
    let mut best: Option<i32> = None;
    for dx in -WALL_FALLOFF..=WALL_FALLOFF {
        for dz in -WALL_FALLOFF..=WALL_FALLOFF {
            let c = Point2D::new(p.x + dx, p.y + dz);
            if matches!(editor.world().get_claim(c), Some(BuildClaim::Wall | BuildClaim::Gate)) {
                let d = dx.abs().max(dz.abs());
                best = Some(best.map_or(d, |b| b.min(d)));
            }
        }
    }
    best
}

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
            // Don't raise terrain onto already-paved roads (the lerped fill would
            // bury the pavement) or onto another building/structure (the fill
            // would cut earth and grass into a neighbouring house). `PathPlanned`
            // is fine to blend through: those cells are reserved but not yet
            // built, and a later pave pass repaves at the new (raised) height.
            if matches!(
                ctx.editor.world().get_claim(point),
                Some(BuildClaim::Path(_) | BuildClaim::Building(_) | BuildClaim::Structure(_))
            ) {
                continue;
            }

            let terrain_y = ctx.editor.world().get_ocean_floor_height_at(point);

            // Only raise terrain, never lower it. If terrain is already at or
            // above the lerped target there's nothing to do.
            let t = dist as f64 / BLEND_RADIUS as f64;
            let mut target_y = lerp_i32(base_y, terrain_y, t);

            // Taper the raise toward natural terrain near the wall/gate so the
            // foundation blend never lifts the ground into them — 0 raise at the
            // wall, ramping back to full over WALL_FALLOFF cells.
            if let Some(d) = wall_proximity(ctx.editor, point) {
                let w = (d as f64 / WALL_FALLOFF as f64).clamp(0.0, 1.0);
                target_y = lerp_i32(terrain_y, target_y, w);
            }

            if target_y <= terrain_y {
                continue;
            }

            // Keep the blended cap in the natural surface material; gravity
            // surfaces (sand/gravel) get a solid sandstone/stone body so the cap
            // rests on a base and can't fall. (See `terraform_layers`.)
            let surface = ctx.editor.world().get_ground_block(point).clone();
            let is_snow = surface.id.as_str().contains("snow");
            let (fill, top) = crate::generator::terrain::terraform_layers(&surface);

            // Convert the old surface to fill material since it's being buried.
            ctx.editor.place_block(&fill, point.add_y(terrain_y - 1)).await;

            // Fill column.
            for y in terrain_y..target_y {
                ctx.editor.place_block(&fill, point.add_y(y)).await;
            }

            // Top block becomes the appropriate surface. Forced, because the
            // fill loop above just placed dirt at `target_y - 1`, and a normal
            // place_block skips an equal-density block — leaving a dirt top.
            ctx.editor.place_block_forced(&top, point.add_y(target_y - 1)).await;
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
