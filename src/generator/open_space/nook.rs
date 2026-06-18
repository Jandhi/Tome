//! Furnishing for [`RegionType::Nook`](super::RegionType::Nook) — a small open
//! space ringed by buildings. We treat it as an intimate shared garden: a
//! biome-appropriate small tree as a centrepiece, a few benches backed against
//! the surrounding buildings facing inward, planters tucked into the corners,
//! and a lantern so it isn't dark at night.

use std::collections::HashSet;

use crate::editor::Editor;
use crate::geometry::{Point2D, CARDINALS_2D};
use crate::noise::RNG;

use super::props::{
    inward_dir, is_building, is_path, place_bench, place_lantern_post, place_planter, place_tree,
};
use super::theme::Theme;
use super::Region;

/// Below this area a nook is too cramped for a centrepiece tree — planters only.
const TREE_MIN_AREA: usize = 12;

/// Furnish one nook region in place.
pub async fn furnish_nook(editor: &Editor, region: &Region, rng: &mut RNG, theme: &Theme) {
    let world = editor.world();
    let cells: HashSet<Point2D> = region.cells.iter().copied().collect();
    let height_at = |c: Point2D| world.get_ocean_floor_height_at(c);

    // Cells against a building (bench backing) and perimeter cells clear of any
    // road (planters / lantern), shuffled for variety.
    let mut seat_cells: Vec<Point2D> = Vec::new();
    let mut decor_cells: Vec<Point2D> = Vec::new();
    for &c in &region.cells {
        let mut touches_building = false;
        let mut touches_path = false;
        let mut on_perimeter = false;
        for d in CARDINALS_2D {
            let n = c + d;
            if !cells.contains(&n) {
                on_perimeter = true;
            }
            let claim = world.get_claim(n);
            if is_building(claim.as_ref()) {
                touches_building = true;
            }
            if is_path(claim.as_ref()) {
                touches_path = true;
            }
        }
        if touches_building && !touches_path {
            seat_cells.push(c);
        }
        if on_perimeter && !touches_path {
            decor_cells.push(c);
        }
    }
    rng.shuffle(&mut seat_cells);
    rng.shuffle(&mut decor_cells);

    // How much goes in, by size.
    let area = region.area;
    let (n_benches, n_planters) = if area < 19 {
        (0, 2)
    } else if area < 35 {
        (rng.rand_i32_range(1, 3) as usize, 3)
    } else {
        (rng.rand_i32_range(2, 4) as usize, 3)
    };

    let mut used: HashSet<Point2D> = HashSet::new();

    // Centrepiece: a biome-appropriate small tree nearest the centroid.
    if area >= TREE_MIN_AREA {
        let sum = region.cells.iter().fold(Point2D::ZERO, |a, p| a + *p);
        let centroid = sum / region.cells.len().max(1) as i32;
        if let Some(&center) = region
            .cells
            .iter()
            .min_by_key(|c| c.distance_squared(&centroid))
        {
            let biome = world.get_surface_biome_at(center);
            place_tree(editor, theme, &biome, center, height_at(center), rng).await;
            // Keep the centrepiece cell and its neighbours clear of furniture.
            used.insert(center);
            for d in CARDINALS_2D {
                used.insert(center + d);
            }
        }
    }

    // Benches backed against the buildings, seats facing inward.
    let mut placed = 0;
    for &c in &seat_cells {
        if placed >= n_benches {
            break;
        }
        if used.contains(&c) {
            continue;
        }
        if let Some(inward) = inward_dir(world, c, &cells) {
            place_bench(editor, c, height_at(c), inward, theme.wood).await;
            used.insert(c);
            placed += 1;
        }
    }

    // Planters in the corners.
    let mut placed = 0;
    for &c in &decor_cells {
        if placed >= n_planters {
            break;
        }
        if used.contains(&c) {
            continue;
        }
        place_planter(editor, c, height_at(c), theme.wood).await;
        used.insert(c);
        placed += 1;
    }

    // One lantern on a fence post so the nook reads at night.
    for &c in &decor_cells {
        if used.contains(&c) {
            continue;
        }
        place_lantern_post(editor, c, height_at(c), theme.wood).await;
        break;
    }
}
