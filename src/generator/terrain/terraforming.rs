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
/// - `skip_water`: forwarded to [`force_height`]. `true` leaves lakes alone;
///   `false` terraforms through them and drains all standing water/lava so the
///   city is left with no liquid.
pub async fn flatten_urban_area(
    editor: &mut Editor,
    urban: &HashSet<Point2D>,
    feather: i32,
    smooth_iters: usize,
    skip_water: bool,
) {
    feathered_flatten(editor, urban, feather, smooth_iters, skip_water).await;
}

/// Flatten `region` toward a smoothed height field, feathering back to natural
/// terrain over `feather` cells measured inward from the region boundary, so the
/// flattened area melts into the surrounding landscape instead of ending in a
/// step. Shared by the urban flatten and rural production-area smoothing.
///
/// - `smooth_iters`: passes of wide (5-away) neighbour averaging.
/// - `feather`: width (cells) of the flatten→natural transition band.
/// - `skip_water`: forwarded to [`force_height`] (true leaves lakes alone).
pub async fn feathered_flatten(
    editor: &mut Editor,
    region: &HashSet<Point2D>,
    feather: i32,
    smooth_iters: usize,
    skip_water: bool,
) {
    if region.is_empty() {
        return;
    }

    // Natural surface height at each region cell (skipping tree canopy).
    let natural: HashSet<Point3D> = region
        .iter()
        .map(|p| editor.world().add_non_tree_height(*p))
        .collect();

    // Smoothed target surface: repeated wide averaging flattens local relief
    // while keeping the broad terrain trend.
    let smoothed = average_to_neighbours_5_away_multi(&natural, smooth_iters);
    let smoothed_by_xz: HashMap<Point2D, i32> = smoothed.iter().map(|p| (p.drop_y(), p.y)).collect();
    let natural_by_xz: HashMap<Point2D, i32> = natural.iter().map(|p| (p.drop_y(), p.y)).collect();

    // Distance (in cells) from each region cell to the nearest cell outside the
    // region, via multi-source BFS seeded on the boundary. Drives the feather.
    let dist = boundary_distance(region);
    let feather = feather.max(1);

    let mut targets: HashSet<Point3D> = HashSet::new();
    for &p in region {
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

/// Choose the (subsurface fill, surface cap) blocks for terraforming a column,
/// given the natural surface block. Grassy ground gets a dirt body + grass cap.
/// Gravity surfaces (sand, red sand, gravel) get a SOLID rock body (sandstone /
/// red sandstone / stone) under the loose cap, so the single top layer rests on
/// a base and can't fall — and it matches real desert geology (sand on
/// sandstone). Everything else (stone, terracotta, …) is stable and is filled
/// and capped with itself. Snow folds into the grassy case; callers re-lay the
/// snow layer on top.
pub fn terraform_layers(surface: &Block) -> (Block, Block) {
    let s = surface.id.as_str();
    if s.contains("snow")
        || s.contains("grass_block")
        || s.contains("dirt")
        || s.contains("podzol")
        || s.contains("mycelium")
    {
        (Block::from_id("minecraft:dirt".into()), Block::from_id("minecraft:grass_block".into()))
    } else if s.contains("red_sand") && !s.contains("sandstone") {
        (Block::from_id("minecraft:red_sandstone".into()), surface.clone())
    } else if s.contains("sand") && !s.contains("sandstone") {
        (Block::from_id("minecraft:sandstone".into()), surface.clone())
    } else if s.contains("gravel") {
        (Block::from_id("minecraft:stone".into()), surface.clone())
    } else {
        (surface.clone(), surface.clone())
    }
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
        // Heightmap convention (see World::new / blend_terrain): the surface
        // value is the first air block; the top solid sits at `value - 1`.
        let terrain_y = editor.world().get_ocean_floor_height_at(xz);
        let target_y = point.y;

        // Keep the terraformed cap in the natural surface material: grass stays
        // grass, sand stays sand, stone stays stone. Gravity surfaces (sand/
        // gravel) get a SOLID subsurface (sandstone/stone) so the cap has a base
        // and can't fall. Snow is re-laid on top.
        //
        // Sample the real surface block at `terrain_y - 1` (the top solid), NOT
        // `get_ground_block` — that reads `ground_block_map` at the first-air
        // heightmap value, i.e. the block ABOVE the surface (air), which would
        // mis-detect every surface and cap with air (a 1-deep hole).
        let surface = editor
            .world()
            .get_block(point.with_y(terrain_y - 1))
            .unwrap_or_else(|| Block::from_id("minecraft:dirt".into()));
        let (fill, top) = terraform_layers(&surface);
        let is_snow = surface.id.as_str().contains("snow");

        let mut changed = terrain_y != target_y;
        if terrain_y != target_y {
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

            // Cap with the surface material at the new top (target_y - 1),
            // re-laying a snow layer above it if the original ground was snowy.
            editor.place_block_forced(&top, point.with_y(target_y - 1)).await;
            if is_snow {
                editor.place_block_forced(&surface, point.with_y(target_y)).await;
            }
        }

        // When we're terraforming through liquid (skip_water = false, e.g. the
        // city flatten), REPLACE any standing water/lava in the column with solid
        // ground rather than draining it to air. An air pocket just lets the
        // neighbouring lake flow straight back in — only a solid block keeps the
        // city dry. Fill the whole liquid stack (submerged below grade, and any
        // standing above it), then re-cap and report the true new surface so the
        // heightmap matches.
        let mut surface_air = target_y;
        if !skip_water {
            let is_liquid_at = |editor: &Editor, y: i32| {
                editor
                    .world()
                    .get_block(point.with_y(y))
                    .is_some_and(|b| b.id.is_liquid())
            };

            // Submerged liquid just below the graded surface → solid fill.
            let mut y = target_y - 1;
            while is_liquid_at(editor, y) {
                editor.place_block_forced(&fill, point.with_y(y)).await;
                changed = true;
                y -= 1;
            }
            // Liquid at/above the graded surface → solid fill, tracking the top.
            let mut y = target_y;
            while is_liquid_at(editor, y) {
                editor.place_block_forced(&fill, point.with_y(y)).await;
                changed = true;
                y += 1;
            }
            if y > target_y {
                // Liquid rose above grade; cap the raised solid and lift the surface.
                editor.place_block_forced(&top, point.with_y(y - 1)).await;
                surface_air = y;
            }
        }

        // Only the cells we actually touched may be written back to the
        // heightmap; asserting heights for untouched (e.g. skipped) cells would
        // make the map claim land that was never built.
        if changed {
            updated.insert(point.with_y(surface_air));
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