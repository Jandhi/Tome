//! Furnishing for [`RegionType::Plaza`](super::RegionType::Plaza) — a large open
//! space ringed by buildings: the town's civic square. Unlike a nook, a plaza is
//! *built*: we pave the ground (road material with a border accent), drop a
//! centrepiece structure (well / fountain / monument) on the most-interior cell,
//! ring it with lamp posts and benches, and tuck a little greenery in the
//! corners.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::editor::Editor;
use crate::generator::materials::{Material, MaterialId, Placer};
use crate::generator::terrain::Forest;
use crate::geometry::{Point2D, Point3D, CARDINALS_2D};
use crate::minecraft::BlockForm;
use crate::noise::RNG;

use super::props::{
    chebyshev, inward_dir, is_building, is_path, place_bench, place_lantern_post, place_planter,
    place_tree, put, put_forced,
};
use super::Region;

const FILL_MATERIAL: &str = "cobblestone";
const BORDER_MATERIAL: &str = "stone_bricks";

/// Which centrepiece a plaza gets, chosen by the open room available.
#[derive(Debug, Clone, Copy)]
enum Centerpiece {
    /// 3×3 covered well: rim wall, water, corner posts, slab roof, hung lantern.
    Well,
    /// 5×5 walled basin with a central spouting pillar.
    Fountain,
    /// Stepped plinth with a pillar and a lantern on top — a dry landmark.
    Monument,
}

/// Largest odd square (half-side `radius`: 0=1×1, 1=3×3, 2=5×5) fully inside the
/// region when centred at `c`.
fn max_square_radius(cells: &HashSet<Point2D>, c: Point2D, limit: i32) -> i32 {
    let mut radius = 0;
    while radius < limit {
        let r = radius + 1;
        let fits = (-r..=r).all(|dx| {
            (-r..=r).all(|dz| cells.contains(&Point2D::new(c.x + dx, c.y + dz)))
        });
        if !fits {
            break;
        }
        radius = r;
    }
    radius
}

/// The most-interior region cell (max distance from the perimeter), with the
/// largest odd-square half-radius that fits there.
fn centre_cell(region: &Region, cells: &HashSet<Point2D>) -> (Point2D, i32) {
    let mut dist: HashMap<Point2D, i32> = HashMap::new();
    let mut queue: VecDeque<Point2D> = VecDeque::new();
    for &c in &region.cells {
        if CARDINALS_2D.iter().any(|d| !cells.contains(&(c + *d))) {
            dist.insert(c, 0);
            queue.push_back(c);
        }
    }
    while let Some(c) = queue.pop_front() {
        let dc = dist[&c];
        for d in CARDINALS_2D {
            let n = c + d;
            if cells.contains(&n) && !dist.contains_key(&n) {
                dist.insert(n, dc + 1);
                queue.push_back(n);
            }
        }
    }
    let centre = *region
        .cells
        .iter()
        .max_by_key(|c| dist.get(c).copied().unwrap_or(0))
        .expect("region has cells");
    (centre, max_square_radius(cells, centre, 2))
}

/// Furnish one plaza region in place.
pub async fn furnish_plaza(
    editor: &Editor,
    region: &Region,
    rng: &mut RNG,
    forest: &Forest,
    materials: &HashMap<MaterialId, Material>,
) {
    let world = editor.world();
    let cells: HashSet<Point2D> = region.cells.iter().copied().collect();
    let height_at = |c: Point2D| world.get_ocean_floor_height_at(c);

    // Classify cells: perimeter (touches non-region), entrance (touches road),
    // and seat cells (against a building, clear of road).
    let mut seat_cells: Vec<Point2D> = Vec::new();
    let mut decor_cells: Vec<Point2D> = Vec::new();
    let mut border_cells: Vec<Point2D> = Vec::new();
    for &c in &region.cells {
        let mut on_perimeter = false;
        let mut touches_building = false;
        let mut touches_path = false;
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
        if on_perimeter && !touches_path {
            border_cells.push(c);
        }
        if touches_building && !touches_path {
            seat_cells.push(c);
        }
        if on_perimeter && !touches_path {
            decor_cells.push(c);
        }
    }
    let border_set: HashSet<Point2D> = border_cells.iter().copied().collect();

    // Flatten the plaza to one level — the median surface height — so the paving
    // reads as a level square instead of stepping with the terrain. Everything
    // afterward sits on this single level.
    let mut heights: Vec<i32> = region.cells.iter().map(|&c| height_at(c)).collect();
    heights.sort_unstable();
    let target_h = heights[heights.len() / 2];
    let target_top = target_h - 1; // the paved surface y

    // --- Flatten + pave: road material edge to edge, border accent on the ring. ---
    {
        let fill = MaterialId::new(FILL_MATERIAL.to_string());
        let border = MaterialId::new(BORDER_MATERIAL.to_string());
        let mut placer = Placer::new(materials, rng);
        for &c in &region.cells {
            let base = height_at(c) - 1; // current surface y
            // Cut anything above the new surface.
            for y in (target_top + 1)..=base {
                put_forced(editor, c.x, y, c.y, "minecraft:air").await;
            }
            // Fill dips up to just under the new surface.
            for y in (base + 1)..target_top {
                put_forced(editor, c.x, y, c.y, "minecraft:dirt").await;
            }
            let mat = if border_set.contains(&c) { &border } else { &fill };
            placer
                .place_block_forced(editor, Point3D::new(c.x, target_top, c.y), mat, BlockForm::Block, None, None)
                .await;
        }
    }

    let mut used: HashSet<Point2D> = HashSet::new();

    // --- Centrepiece on the most-interior cell. ---
    let (centre, radius) = centre_cell(region, &cells);
    let piece = match radius {
        r if r >= 2 => *rng.choose(&[Centerpiece::Fountain, Centerpiece::Well, Centerpiece::Monument]),
        1 => *rng.choose(&[Centerpiece::Well, Centerpiece::Monument]),
        _ => Centerpiece::Monument,
    };
    match piece {
        Centerpiece::Well => build_well(editor, centre, target_h).await,
        Centerpiece::Fountain => build_fountain(editor, centre, target_h).await,
        Centerpiece::Monument => build_monument(editor, centre, target_h, radius >= 1).await,
    }
    // Reserve the footprint (+1 margin) so nothing crowds the centrepiece.
    let margin = radius + 1;
    for dx in -margin..=margin {
        for dz in -margin..=margin {
            used.insert(Point2D::new(centre.x + dx, centre.y + dz));
        }
    }

    // --- Lamp posts around the ring, spaced out. ---
    rng.shuffle(&mut border_cells);
    let mut lamps: Vec<Point2D> = Vec::new();
    let lamp_target = (region.area / 40).max(2);
    for &c in &border_cells {
        if lamps.len() >= lamp_target {
            break;
        }
        if used.contains(&c) {
            continue;
        }
        if lamps.iter().any(|l| chebyshev(*l, c) < 5) {
            continue;
        }
        place_lantern_post(editor, c, target_h).await;
        used.insert(c);
        lamps.push(c);
    }

    // --- Benches against the buildings, facing inward. ---
    rng.shuffle(&mut seat_cells);
    let bench_target = (region.area / 30).clamp(2, 6);
    let mut benches: Vec<Point2D> = Vec::new();
    for &c in &seat_cells {
        if benches.len() >= bench_target {
            break;
        }
        if used.contains(&c) {
            continue;
        }
        if benches.iter().any(|b| chebyshev(*b, c) < 3) {
            continue;
        }
        if let Some(inward) = inward_dir(world, c, &cells) {
            place_bench(editor, c, target_h, inward).await;
            used.insert(c);
            benches.push(c);
        }
    }

    // --- Corner greenery: a couple of trees and planters on the ring. ---
    rng.shuffle(&mut decor_cells);
    let mut trees = 0;
    let mut planters = 0;
    for &c in &decor_cells {
        if used.contains(&c) {
            continue;
        }
        if trees < 3 {
            let biome = world.get_surface_biome_at(c);
            if place_tree(editor, forest, &biome, c, target_h, rng).await {
                used.insert(c);
                trees += 1;
                continue;
            }
        }
        if planters < 4 {
            place_planter(editor, c, target_h).await;
            used.insert(c);
            planters += 1;
        }
    }
}

/// 3×3 covered well centred at `c`; `h` is the first air cell above the paving.
async fn build_well(editor: &Editor, c: Point2D, h: i32) {
    // Rim wall at the base; water in the middle, sunk to ground level and
    // forced so it shows through the paving we just laid.
    for dx in -1..=1 {
        for dz in -1..=1 {
            let (x, z) = (c.x + dx, c.y + dz);
            if dx == 0 && dz == 0 {
                // Submerged chain links: waterlogged so the shaft still reads as
                // water, with the chain continuing down into it.
                put_forced(editor, x, h - 2, z, "minecraft:chain[axis=y,waterlogged=true]").await;
                put_forced(editor, x, h - 1, z, "minecraft:chain[axis=y,waterlogged=true]").await;
            } else {
                put(editor, x, h, z, "minecraft:cobblestone_wall").await;
            }
        }
    }
    // Corner posts up to the roof.
    for &(dx, dz) in &[(-1, -1), (1, -1), (-1, 1), (1, 1)] {
        put(editor, c.x + dx, h + 1, c.y + dz, "minecraft:oak_fence").await;
        put(editor, c.x + dx, h + 2, c.y + dz, "minecraft:oak_fence").await;
    }
    // Chain hung from under the roof, all the way down to the water surface.
    for y in h..=h + 2 {
        put(editor, c.x, y, c.y, "minecraft:chain[axis=y]").await;
    }
    // 3×3 bottom-slab roof.
    for dx in -1..=1 {
        for dz in -1..=1 {
            put(editor, c.x + dx, h + 3, c.y + dz, "minecraft:spruce_slab[type=bottom]").await;
        }
    }
}

/// 5×5 walled basin with a central spouting pillar, centred at `c`.
async fn build_fountain(editor: &Editor, c: Point2D, h: i32) {
    for dx in -2..=2 {
        for dz in -2..=2 {
            let (x, z) = (c.x + dx, c.y + dz);
            let cheb = dx.abs().max(dz.abs());
            match cheb {
                2 => put(editor, x, h, z, "minecraft:stone_brick_wall").await, // basin wall
                1 => put(editor, x, h, z, "minecraft:water").await,            // water ring
                _ => {
                    // Central pillar with a water spout on top.
                    put(editor, x, h, z, "minecraft:chiseled_stone_bricks").await;
                    put(editor, x, h + 1, z, "minecraft:stone_bricks").await;
                    put(editor, x, h + 2, z, "minecraft:water").await;
                }
            }
        }
    }
}

/// Stepped plinth + pillar + lantern. `wide` builds a 3×3 base, else a 1×1.
async fn build_monument(editor: &Editor, c: Point2D, h: i32, wide: bool) {
    if wide {
        for dx in -1..=1 {
            for dz in -1..=1 {
                put(editor, c.x + dx, h, c.y + dz, "minecraft:stone_bricks").await;
            }
        }
    } else {
        put(editor, c.x, h, c.y, "minecraft:stone_bricks").await;
    }
    let base = if wide { h + 1 } else { h };
    put(editor, c.x, base, c.y, "minecraft:chiseled_stone_bricks").await;
    put(editor, c.x, base + 1, c.y, "minecraft:stone_bricks").await;
    put(editor, c.x, base + 2, c.y, "minecraft:chiseled_stone_bricks").await;
    put(editor, c.x, base + 3, c.y, "minecraft:lantern").await;
}
