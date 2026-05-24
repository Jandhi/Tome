use std::collections::HashSet;

use crate::{editor::Editor, geometry::{average_to_neighbours_5_away_multi, Point2D, Point3D}};

pub async fn force_height(editor: &mut Editor, points: &HashSet<Point3D>, skip_water : bool) {
    for point in points {
        if editor.world().is_water(point.drop_y()) && skip_water {
            continue;
        }

        let height = editor.world().get_ocean_floor_height_at(point.drop_y());

        let surface_block = editor.world().get_block(point.with_y(height)).expect("Expected a block at the surface");

        if surface_block.id.is_water() && skip_water {
            continue;
        }

        if height < point.y {
            for y in height..=point.y {
                editor.place_block_forced(&surface_block, point.with_y(y)).await;
            }
        } else {
            for y in point.y..=height {
                editor.place_block_forced(&"air".into(), point.with_y(y)).await;
            }
        }
    }
    
    editor.world_mut().set_heights(points);
}

/// Smooths terrain over `points` using repeated wide-radius neighbour averaging
/// (same algorithm as road smoothing). `strength` in [0.0, 1.0] maps to 0–5 passes.
pub async fn smooth_terrain(points: &HashSet<Point2D>, strength: f32, editor: &mut Editor) {
    const MAX_ITERATIONS: usize = 5;
    let iterations = (strength.clamp(0.0, 1.0) * MAX_ITERATIONS as f32).round() as usize;
    if iterations == 0 {
        return;
    }

    let points_3d: HashSet<Point3D> = points
        .iter()
        .filter(|&&p| !editor.world().is_water(p))
        .map(|&p| {
            let y = editor.world().get_non_tree_height(p);
            Point3D::new(p.x, y, p.y)
        })
        .collect();

    let smoothed = average_to_neighbours_5_away_multi(&points_3d, iterations);
    force_height(editor, &smoothed, true).await;
}