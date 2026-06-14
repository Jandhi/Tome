//! Door-to-road connectors.
//!
//! For each building door that isn't already on or beside a road, A* a narrow
//! footpath from the door out to the nearest road cell and pave it with the road
//! material. Run *after* all roads and buildings are placed, so the goal network
//! is complete and the route can steer around every footprint.

use std::collections::HashSet;

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::MaterialId;
use crate::geometry::Point2D;
use crate::noise::RNG;

use super::building::build_path;
use super::path::PathPriority;
use super::routing::{get_path_with, RouteContext, RouteParams};

/// Don't bother routing a connector longer than this (Manhattan) — a door that
/// far from any road is almost certainly an interior/odd case.
const MAX_CONNECTOR: i32 = 40;

/// Pave a 1-wide path from every disconnected door to the nearest road.
///
/// `doors` are the ground cells just outside each door. A door is skipped when
/// it (or a cardinal neighbour) already sits on a road. `region` confines the
/// route to the urban area, `blocked` are cells the route must avoid (building /
/// wall footprints), and `road_cells` are both the goal and the stop condition.
pub async fn connect_doors_to_roads(
    editor: &Editor,
    data: &LoadedData,
    doors: &[Point2D],
    region: &HashSet<Point2D>,
    road_cells: &HashSet<Point2D>,
    blocked: &HashSet<Point2D>,
    material: MaterialId,
    rng: &mut RNG,
) -> usize {
    if road_cells.is_empty() {
        return 0;
    }

    let mut paved = 0usize;
    for &door in doors {
        // Already connected? On a road, or one cell from one.
        if road_cells.contains(&door)
            || door.neighbours().iter().any(|n| road_cells.contains(n))
        {
            continue;
        }

        // Nearest road cell.
        let Some(&goal) = road_cells.iter().min_by_key(|r| r.distance_squared(&door)) else {
            continue;
        };
        if door.distance_manhattan(&goal) > MAX_CONNECTOR {
            continue;
        }

        let start = editor.world().add_height(door);
        let end = editor.world().add_height(goal);

        // Route a Low-priority (width-1) path that stays in the urban area,
        // dodges building/wall footprints, and ends the moment it touches a road.
        let routed = {
            let ctx = RouteContext {
                region: Some(region),
                road_cells: None,
                road_height: None,
                goal_cells: Some(road_cells),
                wall_dist: None,
                blocked: Some(blocked),
            };
            get_path_with(
                editor,
                start,
                end,
                PathPriority::Low,
                material.clone(),
                RouteParams::default(),
                ctx,
                async |_| {},
            )
            .await
        };

        if let Some(path) = routed {
            build_path(editor, data, &path, rng).await;
            paved += 1;
        }
    }

    paved
}
