use std::collections::{HashMap, HashSet, VecDeque};

use crate::{editor::Editor, geometry::{average_to_neighbours_5_away_multi, Point2D, Point3D, CARDINALS_2D}, minecraft::Block};

/// Flatten the urban interior toward a smoothed height field, feathering back to
/// natural terrain over `feather` cells near the city edge so the settlement
/// melts into the landscape instead of sitting on a mesa.
///
/// - `smooth_iters`: passes of wide (5-away) neighbour averaging used to build
///   the flattened target surface. Higher → flatter city, more earthworks.
/// - `feather`: width in cells of the flatten→natural transition band, measured
///   from the urban boundary inward.
/// - `skip_water`: forwarded to [`force_height`] (true leaves lakes alone).
pub async fn flatten_urban_area(
    editor: &mut Editor,
    urban: &HashSet<Point2D>,
    feather: i32,
    smooth_iters: usize,
    skip_water: bool,
) {
    if urban.is_empty() {
        return;
    }

    // Natural surface height at each urban cell (skipping tree canopy).
    let natural: HashSet<Point3D> = urban
        .iter()
        .map(|p| editor.world().add_non_tree_height(*p))
        .collect();

    // Smoothed target surface: repeated wide averaging flattens local relief
    // while keeping the broad terrain trend.
    let smoothed = average_to_neighbours_5_away_multi(&natural, smooth_iters);
    let smoothed_by_xz: HashMap<Point2D, i32> = smoothed.iter().map(|p| (p.drop_y(), p.y)).collect();
    let natural_by_xz: HashMap<Point2D, i32> = natural.iter().map(|p| (p.drop_y(), p.y)).collect();

    // Distance (in cells) from each urban cell to the nearest non-urban cell,
    // via multi-source BFS seeded on the boundary. Drives the feather.
    let dist = boundary_distance(urban);
    let feather = feather.max(1);

    let mut targets: HashSet<Point3D> = HashSet::new();
    for &p in urban {
        let natural_y = natural_by_xz[&p];
        let smoothed_y = *smoothed_by_xz.get(&p).unwrap_or(&natural_y);
        let d = *dist.get(&p).unwrap_or(&0);
        // t = 0 at the edge (natural terrain), 1 deep inside (fully smoothed).
        let t = (d as f64 / feather as f64).min(1.0);
        let target_y = (natural_y as f64 + (smoothed_y - natural_y) as f64 * t).round() as i32;
        targets.insert(Point3D::new(p.x, target_y, p.y));
    }

    force_height(editor, &targets, skip_water).await;
}

/// Multi-source BFS: distance (in cardinal steps) from each cell in `cells` to
/// the nearest cell *not* in `cells`. Boundary cells (those with an outside
/// cardinal neighbour) get distance 0.
fn boundary_distance(cells: &HashSet<Point2D>) -> HashMap<Point2D, i32> {
    let mut dist: HashMap<Point2D, i32> = HashMap::new();
    let mut queue: VecDeque<Point2D> = VecDeque::new();

    for &p in cells {
        if CARDINALS_2D.iter().any(|&d| !cells.contains(&(p + d))) {
            dist.insert(p, 0);
            queue.push_back(p);
        }
    }

    while let Some(p) = queue.pop_front() {
        let d = dist[&p];
        for dir in CARDINALS_2D {
            let n = p + dir;
            if cells.contains(&n) && !dist.contains_key(&n) {
                dist.insert(n, d + 1);
                queue.push_back(n);
            }
        }
    }

    dist
}

pub async fn force_height(editor: &mut Editor, points: &HashSet<Point3D>, skip_water : bool) {
    // Only the points we actually terraform may be written back to the
    // heightmap. Asserting heights for skipped water cells would make the map
    // claim land that was never built — downstream consumers (e.g. the wall's
    // ground fill) would then float above the real water surface.
    let mut updated: HashSet<Point3D> = HashSet::new();
    for point in points {
        let xz = point.drop_y();
        if editor.world().is_water(xz) && skip_water {
            continue;
        }
        // Never grade a built wall cell: a road corridor's width ring can reach
        // the solid wall beside a gate, and clearing air down to road level there
        // would punch a hole through the wall. Gate openings (claimed `Gate`) are
        // still graded so the road passes through them flush.
        if matches!(editor.world().get_claim(xz), Some(crate::generator::BuildClaim::Wall)) {
            continue;
        }
        updated.insert(*point);

        // Heightmap convention (see World::new / blend_terrain): the surface
        // value is the first air block; the top solid sits at `value - 1`.
        let terrain_y = editor.world().get_ocean_floor_height_at(xz);
        let target_y = point.y;
        if terrain_y == target_y {
            continue;
        }

        // Material choice mirrors foundation `blend_terrain`: dirt+grass for
        // normal ground, sand for sandy ground, snow re-capped on top. We sample
        // the ground block only to *detect* the surface type — the placed top is
        // a literal block, since the sampled cell isn't reliably solid.
        let surface = editor.world().get_ground_block(xz).clone();
        let is_snow = surface.id.as_str().contains("snow");
        let is_sandy = {
            let s = surface.id.as_str();
            s.contains("sand") || s.contains("sandstone")
        };
        let (fill, top) = if is_sandy {
            (Block::from_id("minecraft:sand".into()), Block::from_id("minecraft:sand".into()))
        } else {
            (Block::from_id("minecraft:dirt".into()), Block::from_id("minecraft:grass_block".into()))
        };

        if target_y > terrain_y {
            // Raise: subsurface fill from the old surface up to the new top.
            for y in terrain_y..target_y {
                editor.place_block_forced(&fill, point.with_y(y)).await;
            }
        } else {
            // Lower: clear down to the new surface.
            for y in target_y..=terrain_y {
                editor.place_block_forced(&"air".into(), point.with_y(y)).await;
            }
        }

        // Cap with the surface material at the new top (target_y - 1), re-laying
        // a snow layer above it if the original ground was snowy.
        editor.place_block_forced(&top, point.with_y(target_y - 1)).await;
        if is_snow {
            editor.place_block_forced(&surface, point.with_y(target_y)).await;
        }
    }

    editor.world_mut().set_heights(&updated);
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