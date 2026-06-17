use std::collections::{HashMap, HashSet};
use log::info;
use crate::{generator::{districts::build_wall_gate, materials::{MaterialId, Placer}, nbts::{place_structure, Structure, StructureType}, BuildClaim}, geometry::{get_neighbours_in_set, get_edge, is_point_surrounded_by_points, Cardinal, Point2D, Point3D, CARDINALS_2D}, minecraft::BlockForm, noise::RNG};

use crate::editor::Editor;

pub const WALL_HEIGHT: i32 = 10; // optimal height of wall, will change based on smoothing and heightmap
pub const _WATER_CHECK: usize = 5;
pub const RANGE: i32 = 3;  // range for walkway flattening

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WallType { //used for both interal wall calculations and for choosing wall type to build
    Water,
    WaterWall,
    Standard,
    Palisade,
    StandardWithInner,
}

pub fn get_wall_points(
    inner_points: &HashSet<Point2D>,
    editor: &mut Editor,
) -> HashSet<Point2D> {
    let wall_points = get_edge(inner_points);

    for point in &wall_points {
        editor.world_mut().claim(*point, BuildClaim::Wall); // mark wall points as claimed
    }

    wall_points
}

/// Connected components of `region` under 4-connectivity.
pub(crate) fn connected_components(region: &HashSet<Point2D>) -> Vec<HashSet<Point2D>> {
    let mut remaining = region.clone();
    let mut components = Vec::new();

    while let Some(&start) = remaining.iter().next() {
        remaining.remove(&start);
        let mut component = HashSet::new();
        let mut stack = vec![start];
        while let Some(point) = stack.pop() {
            component.insert(point);
            for dir in CARDINALS_2D {
                let neighbour = point + dir;
                if remaining.remove(&neighbour) {
                    stack.push(neighbour);
                }
            }
        }
        components.push(component);
    }

    components
}

/// Moore-neighbour boundary tracing of a single connected `region`. Returns the
/// outer-boundary cells in order, walking the contour clockwise from the
/// top-left-most cell. The result is 8-connected (it takes diagonal steps at
/// convex corners — `densify_loop` closes those into a 4-connected ring).
///
/// Robust where the old greedy walk was not: it follows the contour by the
/// standard backtrack rule, so it never cuts across an inside corner and never
/// strands cells.
fn moore_trace(region: &HashSet<Point2D>) -> Vec<Point2D> {
    // The 8 Moore-neighbour offsets in clockwise order, starting due West.
    const CW: [Point2D; 8] = [
        Point2D { x: -1, y: 0 },  // W
        Point2D { x: -1, y: -1 }, // NW
        Point2D { x: 0, y: -1 },  // N
        Point2D { x: 1, y: -1 },  // NE
        Point2D { x: 1, y: 0 },   // E
        Point2D { x: 1, y: 1 },   // SE
        Point2D { x: 0, y: 1 },   // S
        Point2D { x: -1, y: 1 },  // SW
    ];
    fn cw_index(offset: Point2D) -> usize {
        CW.iter().position(|&c| c == offset).expect("offset must be a Moore neighbour")
    }

    // Top-most row, then left-most cell in it: a guaranteed convex corner whose
    // West neighbour is empty, so we can enter it from the West.
    let start = *region.iter().min_by_key(|p| (p.y, p.x)).unwrap();
    let mut contour = vec![start];
    if region.len() == 1 {
        return contour;
    }

    let mut current = start;
    // The (empty) cell we "came from"; West of the start is empty by construction.
    let mut backtrack = Point2D { x: start.x - 1, y: start.y };

    // Safety cap: a contour can revisit cells (spikes) but is bounded well under this.
    let max_steps = region.len() * 8 + 8;
    for _ in 0..max_steps {
        let start_idx = cw_index(backtrack - current);
        let mut prev_cell = backtrack; // last empty cell examined before a hit
        let mut next = None;
        for k in 1..=8 {
            let cand = current + CW[(start_idx + k) % 8];
            if region.contains(&cand) {
                next = Some((cand, prev_cell));
                break;
            }
            prev_cell = cand;
        }
        let (cell, new_backtrack) = match next {
            Some(v) => v,
            None => break, // isolated cell — unreachable for len > 1
        };
        if cell == start {
            break; // closed the loop
        }
        contour.push(cell);
        current = cell;
        backtrack = new_backtrack;
    }

    contour
}

/// Convert an 8-connected contour into a 4-connected ring by inserting the
/// "elbow" cell at every diagonal step, so the built wall has no diagonal seam
/// holes.
///
/// Prefers the elbow that lies *outside* the region, so the wall stays a single
/// outer ring and the cell one step inward is left free for the walkway — rather
/// than pulling the ring inward (which made the wall top 2 cells thick at corners
/// and stole the walkway cell). Falls back to an in-region elbow, then to the
/// horizontal one, for the rare bare diagonal staircase where both sides are open.
fn densify_loop(contour: &[Point2D], region: &HashSet<Point2D>) -> Vec<Point2D> {
    let n = contour.len();
    let mut dense = Vec::with_capacity(n);
    for i in 0..n {
        let cur = contour[i];
        dense.push(cur);
        let next = contour[(i + 1) % n];
        let d = next - cur;
        if d.x != 0 && d.y != 0 {
            let horizontal = Point2D { x: cur.x + d.x, y: cur.y };
            let vertical = Point2D { x: cur.x, y: cur.y + d.y };
            if !region.contains(&horizontal) {
                dense.push(horizontal);
            } else if !region.contains(&vertical) {
                dense.push(vertical);
            } else {
                dense.push(horizontal);
            }
        }
    }
    dense
}

/// Order the wall ring(s) for a filled urban `region`: one closed, 4-connected
/// loop per connected component. Replaces the old greedy traversal that could
/// discard whole arcs (holes) and leave diagonal seams.
pub fn trace_wall_loops(region: &HashSet<Point2D>) -> Vec<Vec<Point2D>> {
    // Drop degenerate specks — too small to be a meaningful walled section.
    const MIN_WALL_LOOP: usize = 12;

    let mut loops = Vec::new();
    for component in connected_components(region) {
        if component.len() < MIN_WALL_LOOP {
            continue;
        }
        let contour = moore_trace(&component);
        let dense = densify_loop(&contour, &component);
        if dense.len() >= 3 {
            loops.push(dense);
        }
    }
    loops
}

/// True if `cell` sits on the outer face of the wall: it has a 4-neighbour that is
/// neither inside the city (`region`) nor part of the wall `ring`. Only these cells
/// get a parapet stair — the inner cells of a diagonal staircase (real boundary
/// cells now fronted by an outside filler cell) are fully enclosed by city + ring,
/// so they read as walkway instead, leaving a single clean outer stair line.
fn parapet_is_outer(cell: Point2D, region: &HashSet<Point2D>, ring: &HashSet<Point2D>) -> bool {
    CARDINALS_2D.iter().any(|&d| {
        let nb = cell + d;
        !region.contains(&nb) && !ring.contains(&nb)
    })
}

pub async fn build_wall(urban_points: &HashSet<Point2D>, editor: &mut Editor, rng : &mut RNG, material_placer: &mut Placer<'_>, material_id: &MaterialId, structures: & HashMap<StructureType, Structure>, wall_type: WallType) {
    let ordered_wall_points = trace_wall_loops(urban_points);
    let total_points: usize = ordered_wall_points.iter().map(|loop_| loop_.len()).sum();
    info!(
        "[Wall] Traced {} wall loop(s), {} wall points total",
        ordered_wall_points.len(),
        total_points
    );

    // Claim every cell of every ring up front so building placement steers clear of
    // the whole wall, not just the loop currently being built.
    for wall_loop in &ordered_wall_points {
        for &point in wall_loop {
            editor.world_mut().claim(point, BuildClaim::Wall);
        }
    }

    for wall_point_list in ordered_wall_points {
        if wall_type == WallType::Standard {
            build_wall_standard(&wall_point_list, editor, rng, material_placer, material_id, structures, urban_points).await;
        } else if wall_type == WallType::Palisade {
            build_wall_palisade(&wall_point_list, editor, rng, material_placer, material_id, structures).await;
        } else if wall_type == WallType::StandardWithInner {
            build_wall_standard_with_inner(&wall_point_list, editor, rng, material_placer, material_id, structures, urban_points).await;
        }
    }
}

pub async fn build_wall_palisade(wall_points: &Vec<Point2D>, editor: &mut Editor, rng: &mut RNG, material_placer: &mut Placer<'_>, material_id: &MaterialId, structures: & HashMap<StructureType, Structure>) {
    let wall_points_with_height = wall_points.iter()
        .map(|&point| {
            let height = rng.rand_i32_range(4, 7);
            let new_point = editor.world().add_height(point);
            (new_point, height)
        })
        .collect::<HashMap<_, _>>();

    let mut main_points = Vec::new();
    let mut top_points = Vec::new();
    let wall_points_with_world_height = wall_points.iter()
        .map(|&point| editor.world().add_height(point))
        .collect::<Vec<_>>();

    for (point, height) in wall_points_with_height {
        if editor.world().is_water(point.drop_y()) {
            continue; // Skip water points
        }
        for y in point.y..point.y + height {
            main_points.push(Point3D { x: point.x, y, z: point.z });
        }
        top_points.push(Point3D { x: point.x, y: point.y + height, z: point.z });
        
    }
    material_placer.place_blocks(
            editor, 
            main_points.into_iter(),
            material_id,
            BlockForm::Log,
        None,
        None).await;
    material_placer.place_blocks(
            editor, 
            top_points.into_iter(),
            material_id,
            BlockForm::Fence,
        None, None).await;


    //add gates
    build_wall_gate(&wall_points_with_world_height, editor, rng, material_placer, true, true, None, None, structures, 10).await;

}

pub async fn build_wall_standard(wall_points: &Vec<Point2D>, editor: &mut Editor, rng: &mut RNG, material_placer: &mut Placer<'_>, material_id: &MaterialId, structures: & HashMap<StructureType, Structure>, urban_points: &HashSet<Point2D>) {
    let wall_points_with_height = add_wall_points_height(wall_points, editor);
    let wall_set: HashSet<Point2D> = wall_points.iter().cloned().collect();
    let enhanced_wall_points = check_water(&mut add_wall_points_directionality(&wall_points_with_height, &wall_set, urban_points), editor);

    let mut walkway_points = Vec::<Point2D>::new();
    let mut walkway_heights: HashMap<Point2D, i32> = HashMap::new();

    let mut previous_dir = Cardinal::North; // Default direction

    for (i, (point, directions, wall_type)) in enhanced_wall_points.iter().enumerate() {
        if wall_type == &WallType::Water {
            continue;
        } else {
            if wall_type == &WallType::WaterWall {
                // If it's a water wall, we place blocks in the water
                fill_water(point.drop_y(), editor, material_placer, material_id).await;
            }
            // Outer-face cells carry the full-height wall (and a parapet stair below);
            // the inner cells of a diagonal staircase top out one slab lower so they
            // sit flush with the walkway behind them instead of jutting up as a block.
            let is_outer = parapet_is_outer(point.drop_y(), urban_points, &wall_set);
            let column_top = if is_outer { point.y } else { point.y - 1 };
            for y in editor.world().get_height_at(point.drop_y())..=column_top {
                let new_point = Point3D { x: point.x, y, z: point.z };
                material_placer.place_block(editor, new_point, material_id, BlockForm::Block, None, None).await;
            }
            if !is_outer {
                material_placer.place_block(editor, Point3D { x: point.x, y: point.y, z: point.z }, material_id, BlockForm::Slab, None, None).await;
            }
            if directions.len() > 0 {
                previous_dir = directions[0];
            }
            // Parapet cap: a stair lip only on outer-face cells. Inner staircase cells
            // were already dropped a slab above and read as walkway, so they get no cap.
            if is_outer {
                let state = HashMap::from([("facing".to_string(), previous_dir.rotate_right().to_string())]);
                material_placer.place_block(editor, Point3D { x: point.x, y: point.y + 1, z: point.z }, material_id, BlockForm::Stairs, Some(&state), None).await;
            }
        
            for dir in directions.iter() {
                let mut height_modifier = 0;

                if i != 0 && i != enhanced_wall_points.len() - 1 {
                    let prev_h = enhanced_wall_points[i - 1].0.y;
                    let next_h = enhanced_wall_points[i + 1].0.y;
                    let h = point.y;
                    if prev_h == h -1 && next_h == h - 1 {
                        height_modifier = -1;
                    }
                }
                if directions.contains(&dir.rotate_right()) {
                    for new_pt in [
                        point.drop_y() + Point2D::from(*dir) + Point2D::from(dir.rotate_right()),
                        point.drop_y() + Point2D::from(*dir) + Point2D::from(dir.rotate_right()) * 2,
                        point.drop_y() + Point2D::from(*dir) * 2 + Point2D::from(dir.rotate_right())
                    ] {
                        if wall_points.contains(&new_pt) {
                            break; // should this be continue?
                        }
                        if !walkway_points.contains(&new_pt) {
                            walkway_points.push(new_pt);
                            walkway_heights.insert(new_pt, point.y + height_modifier);
                            
                        }
                    }
                } 
                for x in 1..=3 {
                    let new_pt = point.drop_y() + Point2D::from(*dir) * x;
                    if wall_points.contains(&new_pt) {
                        break;
                    }
                    if !walkway_points.contains(&new_pt) {
                        walkway_points.push(new_pt);
                        walkway_heights.insert(new_pt, point.y + height_modifier);
                    }
                }
            }
        }
    }

    flatten_walkway(&walkway_points, &mut walkway_heights, editor, material_placer, material_id).await;
    // Claim every walkway cell as wall — building placement must steer around them.
    for p in &walkway_points {
        editor.world_mut().claim(*p, BuildClaim::Wall);
    }
    //add gates
    build_wall_gate(&wall_points_with_height, editor, rng, material_placer, true, false, None, None, structures, 6).await

}


pub async fn build_wall_standard_with_inner(wall_points: &Vec<Point2D>, editor: &mut Editor, rng: &mut RNG, material_placer: &mut Placer<'_>, material_id: &MaterialId, structures: & HashMap<StructureType, Structure>, urban_points: &HashSet<Point2D>) {
    let wall_points_with_height = add_wall_points_height(wall_points, editor);
    let wall_set: HashSet<Point2D> = wall_points.iter().cloned().collect();
    let enhanced_wall_points = check_water(&mut add_wall_points_directionality(&wall_points_with_height, &wall_set, urban_points), editor);

    let mut walkway_points = Vec::<Point2D>::new();
    let mut walkway_heights: HashMap<Point2D, i32> = HashMap::new();

    let mut inner_wall_points = HashSet::<Point3D>::new();


    let mut previous_dir = Cardinal::North; // Default direction

    for (i, (point, directions, wall_type)) in enhanced_wall_points.iter().enumerate() {
        let mut fill_in = false;
        if wall_type == &WallType::Water {
            continue;
        } else {
            if i == 0 || i == enhanced_wall_points.len() - 1
                || enhanced_wall_points[i + 1].2 == WallType::Water
                || enhanced_wall_points[i - 1].2 == WallType::Water
                || point.y > enhanced_wall_points[i + 1].0.y + 4
                || point.y > enhanced_wall_points[i - 1].0.y + 4 {  
                fill_in = true; // Fill in the first and last points if they are StandardWithInner
            }
            if wall_type == &WallType::WaterWall {
                // If it's a water wall, we place blocks in the water
                fill_water(point.drop_y(), editor, material_placer, material_id).await;
            }
            // Outer-face cells carry the full-height wall (and a parapet stair below);
            // the inner cells of a diagonal staircase top out one slab lower so they
            // sit flush with the walkway behind them instead of jutting up as a block.
            let is_outer = parapet_is_outer(point.drop_y(), urban_points, &wall_set);
            let column_top = if is_outer { point.y } else { point.y - 1 };
            for y in editor.world().get_height_at(point.drop_y())..=column_top {
                let new_point = Point3D { x: point.x, y, z: point.z };
                material_placer.place_block(editor, new_point, material_id, BlockForm::Block, None, None).await;
            }
            if !is_outer {
                material_placer.place_block(editor, Point3D { x: point.x, y: point.y, z: point.z }, material_id, BlockForm::Slab, None, None).await;
            }
            if directions.len() > 0 {
                previous_dir = directions[0];
            }
            // Parapet cap: a stair lip only on outer-face cells. Inner staircase cells
            // were already dropped a slab above and read as walkway, so they get no cap.
            if is_outer {
                let state = HashMap::from([("facing".to_string(), previous_dir.rotate_right().to_string())]);
                material_placer.place_block(editor, Point3D { x: point.x, y: point.y + 1, z: point.z }, material_id, BlockForm::Stairs, Some(&state), None).await;
            }
        
            for dir in directions.iter() {
                let mut height_modifier = 0;

                if i != 0 && i != enhanced_wall_points.len() - 1 {
                    let prev_h = enhanced_wall_points[i - 1].0.y;
                    let next_h = enhanced_wall_points[i + 1].0.y;
                    let h = point.y;
                    if prev_h == h -1 && next_h == h - 1 {
                        height_modifier = -1;
                    }
                }
                if directions.contains(&dir.rotate_right()) {
                    for new_pt in [
                        point.drop_y() + Point2D::from(*dir) + Point2D::from(dir.rotate_right()),
                        point.drop_y() + Point2D::from(*dir) + Point2D::from(dir.rotate_right()) * 2,
                        point.drop_y() + Point2D::from(*dir) * 2 + Point2D::from(dir.rotate_right())
                    ] {
                        if wall_points.contains(&new_pt) {
                            break; // should this be continue?
                        }
                        if !walkway_points.contains(&new_pt) {
                            walkway_points.push(new_pt);
                            walkway_heights.insert(new_pt, point.y + height_modifier);
                            
                        }
                        if fill_in {
                            for y in editor.world().get_height_at(new_pt)..point.y {
                                material_placer.place_block(editor, new_pt.add_y(y), material_id, BlockForm::Block, None, None).await;
                            }
                            if editor.world().is_water(new_pt) {
                                fill_water(new_pt, editor, material_placer, material_id).await;
                            }
                        }
                    }
                    //inner wall
                    for new_pt in [
                        point.drop_y() + Point2D::from(*dir) * 2 + Point2D::from(dir.rotate_right()) * 2,
                        point.drop_y() + Point2D::from(*dir) + Point2D::from(dir.rotate_right()) * 3,
                        point.drop_y() + Point2D::from(*dir) * 2 + Point2D::from(dir.rotate_right()) * 2
                    ] {
                        if !wall_points.contains(&new_pt) && !walkway_points.contains(&new_pt) {
                            inner_wall_points.insert(new_pt.add_y(point.y));
                        }
                    }
                }
                for x in 1..=3 {
                    let new_pt = point.drop_y() + Point2D::from(*dir) * x;
                    if wall_points.contains(&new_pt) {
                        break;
                    }
                    if !walkway_points.contains(&new_pt) {
                        walkway_points.push(new_pt);
                        walkway_heights.insert(new_pt, point.y + height_modifier);
                        if x == 3 {
                            let inner_point = point.drop_y() + Point2D::from(*dir) * 4;
                            if !wall_points.contains(&inner_point) && !walkway_points.contains(&inner_point) {
                                inner_wall_points.insert(inner_point.add_y(point.y));
                            }
                        }
                    }
                    if fill_in {
                        for y in editor.world().get_height_at(new_pt)..point.y {
                            material_placer.place_block(editor, new_pt.add_y(y), material_id, BlockForm::Block, None, None).await;
                        }
                        if editor.world().is_water(new_pt) {
                            fill_water(new_pt, editor, material_placer, material_id).await;
                        }
                    }
                }
            }
        }
    }

    for (_i, point) in inner_wall_points.clone().iter().enumerate() {
        if !walkway_points.contains(&point.drop_y()) {
            for y in editor.world().get_height_at(point.drop_y())..=point.y {
                material_placer.place_block(editor, point.drop_y().add_y(y), material_id, BlockForm::Block, None, None).await;
            }
            if editor.world().is_water(point.drop_y()) {
                fill_water(point.drop_y(), editor, material_placer, material_id).await;
            }
        } else {
            inner_wall_points.remove(point); // check if correct or should be i - 1
        }
    }

    flatten_walkway(&walkway_points, &mut walkway_heights, editor, material_placer, material_id).await;
    // Claim every walkway and inner-wall cell as wall so building placement won't
    // overlap the wider wall structure (the core ring is already claimed by `get_wall_points`).
    for p in &walkway_points {
        editor.world_mut().claim(*p, BuildClaim::Wall);
    }
    for p in &inner_wall_points {
        editor.world_mut().claim(p.drop_y(), BuildClaim::Wall);
    }
    //add towers
    build_wall_towers(&walkway_points, &walkway_heights, editor, material_placer, material_id, structures, rng).await;
    //add gates
    build_wall_gate(&wall_points_with_height, editor, rng, material_placer, false, false, Some(&enhanced_wall_points), Some(&inner_wall_points), structures, 6).await

}


/// How far the wall top may rise above its resting `WALL_HEIGHT` to keep a climb
/// walkable (pre-climbing into a rise at 1/cell instead of jumping it).
pub const MAX_WALL_RAISE: i32 = 14;
/// How far the wall top may drop below its resting `WALL_HEIGHT` for the same reason
/// (easing down toward a steep section from the high side). Together these give the
/// height a two-sided budget, so a sharp change is split into a rise on one side and
/// a dip on the other and absorbed over twice as many cells.
pub const MAX_WALL_DROP: i32 = 8;

/// Relax `values` in place to the smallest 1-Lipschitz function that dominates them
/// (no cell more than 1 below a neighbour), treating the slice as a closed ring.
fn relax_lipschitz_above(values: &mut [i32]) {
    let n = values.len();
    loop {
        let mut changed = false;
        for i in 0..n {
            let bound = values[(i + n - 1) % n] - 1;
            if bound > values[i] {
                values[i] = bound;
                changed = true;
            }
        }
        for i in (0..n).rev() {
            let bound = values[(i + 1) % n] - 1;
            if bound > values[i] {
                values[i] = bound;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Relax `values` in place to the largest 1-Lipschitz function dominated by them
/// (no cell more than 1 above a neighbour), treating the slice as a closed ring.
fn relax_lipschitz_below(values: &mut [i32]) {
    let n = values.len();
    loop {
        let mut changed = false;
        for i in 0..n {
            let bound = values[(i + n - 1) % n] + 1;
            if bound < values[i] {
                values[i] = bound;
                changed = true;
            }
        }
        for i in (0..n).rev() {
            let bound = values[(i + 1) % n] + 1;
            if bound < values[i] {
                values[i] = bound;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Computes the wall-top height at each ring point so the walkway is *walkable*:
/// adjacent tops differ by at most 1 wherever the height budget allows, so every
/// step can be bridged by a single stair/slab. Returns a Vec<Point3D> where `.y` is
/// the wall top at each point.
///
/// The desired top is `WALL_HEIGHT` above the terrain. To absorb a sharp change we
/// take the midpoint of two 1-Lipschitz envelopes of that desired profile — the
/// smallest one that dominates it (`above`, which pre-climbs *before* a rise) and the
/// largest one it dominates (`below`, which eases *after* it). Their average is the
/// smoothest 1-Lipschitz curve through the desired profile: it equals the desired
/// height on flat/gentle ground and ramps symmetrically through steep spots, rising
/// on one side and dipping on the other. It is then clamped to a two-sided budget
/// (`-MAX_WALL_DROP .. +MAX_WALL_RAISE` around resting), which only bites — leaving a
/// residual step — where terrain is genuinely too steep for a continuous walkway.
///
/// Ring points are treated as a closed loop (the wall is a cycle), so the constraint
/// is enforced around the wrap as well.
pub fn add_wall_points_height(
    wall_points: &[Point2D],
    editor: &mut Editor,
) -> Vec<Point3D> {
    let n = wall_points.len();

    let desired: Vec<i32> = wall_points
        .iter()
        .map(|p| editor.world().get_height_at(*p) + WALL_HEIGHT)
        .collect();

    let mut above = desired.clone();
    relax_lipschitz_above(&mut above);
    let mut below = desired.clone();
    relax_lipschitz_below(&mut below);

    let tops: Vec<i32> = (0..n)
        .map(|i| {
            // Midpoint of the two envelopes: the smoothest walkable curve. `above`
            // and `below` are each 1-Lipschitz and within 1 of each other per step,
            // so their floored average is 1-Lipschitz too.
            let mid = (above[i] + below[i]).div_euclid(2);
            mid.clamp(desired[i] - MAX_WALL_DROP, desired[i] + MAX_WALL_RAISE)
        })
        .collect();

    wall_points
        .iter()
        .zip(tops)
        .map(|(point, y)| Point3D { x: point.x, y, z: point.y })
        .collect()
}

/// Adds directionality to wall points to know which way to build walkways.
/// Returns a Vec of (Point3D, Vec<Cardinal>, Option<&'static str>).
pub fn add_wall_points_directionality(
    wall_points: &[Point3D],
    wall_set: &HashSet<Point2D>,
    inner_points: &HashSet<Point2D>,
) -> Vec<(Point3D, Vec<Cardinal>, WallType)> {
    let mut enhanced_wall_points = Vec::with_capacity(wall_points.len());
    for &point in wall_points {
        let mut directions = Vec::new();
        let neighbours = get_neighbours_in_set(point.drop_y(), inner_points);
        for neighbour in neighbours {
            if !wall_set.contains(&neighbour) {
                if let Some(dir) = Cardinal::from_point_2d(neighbour - point.drop_y()) {
                    directions.push(dir);
                }
            }
        }
        enhanced_wall_points.push((point, directions, WallType::Standard));
    }
    enhanced_wall_points
}



/// Checks water along wall points and marks them as "water_wall" if needed.
/// Modifies the third tuple element in-place.
pub fn check_water(
    wall_points: &mut Vec<(Point3D, Vec<Cardinal>, WallType)>,
    editor: &mut Editor,
) -> Vec<(Point3D, Vec<Cardinal>, WallType)> {
    let mut enhanced_wall_points = wall_points.clone();

    for i in 0..enhanced_wall_points.len() {
        let point = &enhanced_wall_points[i].0;
        if editor.world().is_water(point.drop_y()) {
            enhanced_wall_points[i].2 = WallType::WaterWall;
            // TO DO, implement more complex logic for water walls
        }
    }
    enhanced_wall_points
}

pub async fn fill_water(
    point: Point2D,
    editor: &mut Editor,
    material_placer: &mut Placer<'_>,
    material_id: &MaterialId,
) {
    let mut water_points = Vec::new();
    let mut height = editor.world().get_height_at(point) - 1;
    while editor.world().is_water_3d(point.add_y(height)) && height > 0 {
        water_points.push(Point3D { x: point.x, y: height, z: point.y });
        height -= 1;
    }
    //To do, fix so this places mossy stuff
    material_placer.place_blocks(
        editor,
        water_points.into_iter(),
        material_id,
        BlockForm::Block, 
        None,
        None,
    ).await;
}

pub async fn flatten_walkway(
    walkway_points: &Vec<Point2D>,
    walkway_heights: &mut HashMap<Point2D, i32>,
    editor: &mut Editor,
    material_placer: &mut Placer<'_>,
    material_id: &MaterialId,
) -> HashMap<Point2D, f64> {

    let mut updated_walkway_heights: HashMap<Point2D, f64> = walkway_points.iter()
        .map(|&point| {
            let height = average_neighbour_height(point, walkway_heights);
            (point, height)
        })
        .collect();

    // place slabs
    for (&point, &height) in updated_walkway_heights.clone().iter() {
        let frac_height = height % 1.0;
        if (frac_height <= 0.25) || (frac_height > 0.75){
            //let state = HashMap::from([("facing".to_string(), previous_dir.rotate_right().to_string())]);
            material_placer.place_block(editor, Point3D { x: point.x, y: height.round() as i32, z: point.y }, material_id, BlockForm::Slab, None, None).await;
            updated_walkway_heights.insert(point, height.round());
        } else if (frac_height > 0.25) && (frac_height <= 0.5) {
            let state = HashMap::from([("type".to_string(), "top".to_string())]);
            material_placer.place_block(editor, Point3D { x: point.x, y: height.round() as i32, z: point.y }, material_id, BlockForm::Slab, Some(&state), None).await;
            updated_walkway_heights.insert(point, height.round() + 0.49);
        } else if (frac_height > 0.5) && (frac_height <= 0.75) {
            //let state = HashMap::from([("facing".to_string(), previous_dir.rotate_right().to_string())]);
            let state = HashMap::from([("type".to_string(), "top".to_string())]);
            material_placer.place_block(editor, Point3D { x: point.x, y: height.round() as i32 - 1, z: point.y }, material_id, BlockForm::Slab, Some(&state), None).await;
            updated_walkway_heights.insert(point, height.round() - 0.51);
        }
    }
    // add stairs
    for (&point, &height) in updated_walkway_heights.clone().iter() {
        for direction in CARDINALS_2D {
            let neighbour = point + Point2D::from(direction);
            if !updated_walkway_heights.contains_key(&neighbour) {
                continue; // Skip if neighbour is not in walkway heights
            }
            else if height % 1.0 == 0.0 { // bottom slab
                if updated_walkway_heights.get(&neighbour).unwrap() - height >= 1.0 {
                    let state = HashMap::from([("facing".to_string(), Cardinal::from_point_2d(direction).expect("Expected cardinal direction").to_string())]);
                    material_placer.place_block(editor, Point3D { x: point.x, y: height.round() as i32, z: point.y }, material_id, BlockForm::Stairs, Some(&state), None).await;
                }
            } else if updated_walkway_heights.get(&neighbour).unwrap() - height <= -1.0 {
                let state = HashMap::from([("facing".to_string(), Cardinal::from_point_2d(direction).expect("Expected cardinal direction").opposite().to_string())]);
                material_placer.place_block(editor, Point3D { x: point.x, y: height.round() as i32 + 1, z: point.y }, material_id, BlockForm::Stairs, Some(&state), None).await;
            }
        }
    }

    // Fill solid blocks beneath each walkway cell so the shelf reads as one
    // continuous surface. Each cell's slab used to float on air; where adjacent
    // cells differed in height by more than a single stair could bridge, the side
    // face was left open — the see-through / fall-through holes on steep terrain.
    // Dropping each column down to its lowest orthogonal neighbour closes those
    // faces, and the minimum-one-block support also seals the slab layer itself.
    for (point, bottom, top) in walkway_support_columns(&updated_walkway_heights) {
        for y in bottom..=top {
            material_placer.place_block(editor, Point3D { x: point.x, y, z: point.y }, material_id, BlockForm::Block, None, None).await;
        }
    }

    // Ladder fallback: where two adjacent walkway cells still differ in height by
    // more than a single stair can bridge — a residual step on terrain too steep even
    // for the two-sided height budget — hang a ladder up the taller cell's face from
    // the lower cell's airspace, so the walkway stays traversable instead of
    // dead-ending at a blank wall. Only the lower cell of the pair places it, climbing
    // toward the higher neighbour; the ladder backs onto that neighbour's fill column.
    for (&point, &height) in updated_walkway_heights.clone().iter() {
        for direction in CARDINALS_2D {
            let neighbour = point + Point2D::from(direction);
            let Some(&neighbour_height) = updated_walkway_heights.get(&neighbour) else {
                continue;
            };
            let low = height.floor() as i32;
            let high = neighbour_height.floor() as i32;
            if high - low < 2 {
                continue; // bridgeable by slab/stair, or this is the higher cell
            }
            // `facing` points away from the supporting block; the higher neighbour
            // (the support) is in `direction`, so the ladder faces the opposite way.
            let facing = Cardinal::from_point_2d(direction)
                .expect("cardinal direction")
                .opposite()
                .to_string();
            let Some(ladder) = crate::minecraft::string_to_block(&format!("ladder[facing={facing}]"))
            else {
                continue;
            };
            for y in (low + 1)..high {
                editor.place_block(&ladder, Point3D { x: point.x, y, z: point.y }).await;
            }
        }
    }

    updated_walkway_heights

}

/// Plan A support fill: for every walkway cell, the inclusive vertical span
/// `[bottom, top]` of solid blocks that must sit beneath its slab cap so the
/// walkway is a continuous surface with no open vertical faces between cells.
///
/// `top` is the highest *full* block under the cap (one below the slab, so it
/// never collides with the cell's own slab regardless of slab type). `bottom`
/// drops to the lowest orthogonal walkway neighbour's `top`, so the face between
/// two cells of differing height is always backed by wall instead of air — that
/// open face was the source of the walkway holes. A cell with no lower neighbour
/// still gets its own single support block (`bottom == top`), sealing the layer.
fn walkway_support_columns(heights: &HashMap<Point2D, f64>) -> Vec<(Point2D, i32, i32)> {
    let support_top = |h: f64| h.floor() as i32 - 1;
    let mut columns = Vec::with_capacity(heights.len());
    for (&point, &height) in heights {
        let top = support_top(height);
        let mut bottom = top;
        for direction in CARDINALS_2D {
            let neighbour = point + Point2D::from(direction);
            if let Some(&neighbour_height) = heights.get(&neighbour) {
                bottom = bottom.min(support_top(neighbour_height));
            }
        }
        columns.push((point, bottom, top));
    }
    columns
}

pub fn average_neighbour_height(
    point: Point2D,
    walkway_heights: &HashMap<Point2D, i32>,
) -> f64 {
    let neighbours: Vec<Point2D> = (-RANGE..=RANGE).flat_map(|x| {
        (-RANGE..=RANGE).map(move |z| Point2D { x: x as i32, y: z as i32 })
    }).collect();
    let mut total_height = 0.0;
    let mut total_weight = 0.0;

    for neighbour in neighbours {
        if !walkway_heights.contains_key(&Point2D::new(point.x + neighbour.x, point.y + neighbour.y)) {
            continue; // Skipping if neighbour is not in walkway heights
        } else if (walkway_heights.get(&Point2D::new(point.x + neighbour.x, point.y + neighbour.y)).unwrap() - 
            walkway_heights.get(&Point2D::new(point.x, point.y)).unwrap()).abs() >= 4 {
            continue;// skipping extremes
        }
        let distance = neighbour.x.abs() + neighbour.y.abs();
        let weight = 0.8_f64.powi(distance);
        total_height += *walkway_heights.get(&Point2D::new(point.x + neighbour.x, point.y + neighbour.y)).unwrap() as f64 * weight;
        total_weight += weight;
    }

    //this was floor division in the python code, is changing this correct?
    total_height / total_weight

}

pub async fn build_wall_towers(
    walkway_points: &Vec<Point2D>,
    walkway_heights: &HashMap<Point2D, i32>,
    editor: &mut Editor,
    material_placer: &mut Placer<'_>,
    material_id: &MaterialId,
    structures: & HashMap<StructureType, Structure>,
    rng: &mut RNG,
) {
    let distance_to_next_tower = 80;
    let mut tower_possible = rng.rand_i32_range(0, distance_to_next_tower / 2);
    let tower = structures.get(&"basic_tower".into()).expect("Structure not found");
    let walkway_set: HashSet<Point2D> = walkway_points.iter().cloned().collect();

    for point in walkway_points {
        if tower_possible == 0 {
            if is_point_surrounded_by_points(*point, &walkway_set) {
                // Build tower at this point
                tower_possible = distance_to_next_tower;
                let neighbours = ((point.x - 2)..=(point.x + 2))
                    .flat_map(|x| {
                        ((point.y - 2)..=(point.y + 2))
                            .map(move |y| Point2D { x, y })
                    })
                    .collect::<Vec<Point2D>>();
                let point_height = walkway_heights.get(point).expect("Should have height for walkway point"); // Default height if not found
                for neighbour in &neighbours {
                    for height in point_height-1..=point_height+5 {
                        if height == point_height + 5 || !walkway_set.contains(neighbour) {
                            material_placer.place_block(editor, neighbour.add_y(height), material_id, BlockForm::Block, None, None).await;
                        }
                    }
                }
                // Claim the tower's 5x5 base so building placement keeps clear of it.
                for neighbour in &neighbours {
                    editor.world_mut().claim(*neighbour, BuildClaim::Wall);
                }
                info!("Placing tower at: {:?}", point.add_y(point_height+6));
                place_structure(editor, None, &tower, point.add_y(point_height+6), Cardinal::North, None, None, false, false).await.expect("Failed to place tower");
            }
        } else {
                tower_possible -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::World;
    use crate::geometry::Rect3D;

    /// Run `add_wall_points_height` over a terrain profile laid along x at z=1,
    /// returning the wall-top height at each point. The profile is treated as a
    /// closed loop (the wall is a ring), so test profiles are written cyclic — the
    /// last and first entries are adjacent and should match terrain-wise.
    fn tops_for(terrain: &[i32]) -> Vec<i32> {
        let n = terrain.len() as i32;
        let build_area =
            Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(n + 2, 320, 4));
        let world = World::synthetic(build_area, 64);
        let mut editor = world.get_offline_editor();

        let mut heights = HashSet::new();
        let mut wall_points = Vec::new();
        for (i, &h) in terrain.iter().enumerate() {
            wall_points.push(Point2D::new(i as i32, 1));
            heights.insert(Point3D::new(i as i32, h, 1));
        }
        editor.world_mut().set_heights(&heights);

        add_wall_points_height(&wall_points, &mut editor)
            .iter()
            .map(|p| p.y)
            .collect()
    }

    /// Assert the wall-top invariants over a cyclic terrain profile:
    /// (1) bounded height — the top stays within the two-sided budget around its
    ///     resting height (`WALL_HEIGHT - MAX_WALL_DROP .. WALL_HEIGHT + MAX_WALL_RAISE`
    ///     above terrain), so the wall always covers the ground and never towers off;
    /// (2) walkable where affordable — a step larger than 1 may only appear where a
    ///     cell has hit a budget boundary (terrain genuinely too steep). Anywhere the
    ///     budget has slack, adjacent tops differ by at most 1, bridgeable by a single
    ///     stair/slab.
    fn assert_invariants(terrain: &[i32]) {
        let tops = tops_for(terrain);
        let n = terrain.len();
        assert_eq!(tops.len(), n);
        let at_budget = |i: usize| {
            tops[i] == terrain[i] + WALL_HEIGHT + MAX_WALL_RAISE
                || tops[i] == terrain[i] + WALL_HEIGHT - MAX_WALL_DROP
        };
        for i in 0..n {
            assert!(
                tops[i] >= terrain[i] + WALL_HEIGHT - MAX_WALL_DROP,
                "point {i}: top {} below the drop budget over terrain {} (profile {terrain:?})",
                tops[i], terrain[i],
            );
            assert!(
                tops[i] <= terrain[i] + WALL_HEIGHT + MAX_WALL_RAISE,
                "point {i}: top {} above the raise budget over terrain {} (profile {terrain:?})",
                tops[i], terrain[i],
            );
        }
        for i in 0..n {
            let j = (i + 1) % n;
            if (tops[i] - tops[j]).abs() > 1 {
                assert!(
                    at_budget(i) || at_budget(j),
                    "unwalkable step {} -> {} between {i} and {j} with budget to spare (profile {terrain:?})",
                    tops[i], tops[j],
                );
            }
        }
    }

    #[test]
    fn flat_terrain_is_uniform() {
        let tops = tops_for(&[64; 10]);
        assert!(tops.iter().all(|&t| t == 64 + WALL_HEIGHT));
    }

    #[test]
    fn smooth_profiles_hold_invariants() {
        assert_invariants(&[64; 8]);
        assert_invariants(&[64, 65, 66, 67, 66, 65, 64, 63]); // gentle hill (cyclic)
        assert_invariants(&[64, 65, 64, 65, 64, 65, 64, 65]); // rolling
    }

    #[test]
    fn cliffs_hold_invariants() {
        assert_invariants(&[64, 64, 64, 90, 90, 90, 64, 64, 64]); // up- then down-cliff
        assert_invariants(&[64, 90, 64, 90, 64, 90, 64, 90]); // alternating spikes
        assert_invariants(&[64, 64, 100, 100, 64, 64]); // tall plateau
    }

    #[test]
    fn moderate_change_is_fully_walkable() {
        // A rise of 8 fits inside the combined two-sided budget (drop + raise = 14),
        // so given room to ramp it is split into a dip on the high side and a climb on
        // the low side and spread into 1-per-cell steps — no unclimbable step anywhere.
        let terrain = [64, 64, 64, 64, 64, 64, 72, 72, 72, 72, 72, 72];
        let tops = tops_for(&terrain);
        let n = tops.len();
        for i in 0..n {
            let j = (i + 1) % n;
            assert!(
                (tops[i] - tops[j]).abs() <= 1,
                "step {} -> {} at {i}->{j} should be walkable (tops {tops:?})",
                tops[i], tops[j],
            );
        }
    }

    #[test]
    fn steep_change_splits_budget_both_ways() {
        // A +20 plateau outruns even the combined budget. The wall both dips below
        // resting on the high side and rises above it on the low side to shrink the
        // gap as far as the budget allows, leaving a residual step at each edge.
        let terrain = [64, 64, 64, 64, 84, 84, 84, 84, 64, 64];
        let tops = tops_for(&terrain);
        let n = tops.len();
        // Low-side approach rises above resting height (uses the raise budget) ...
        assert!(
            tops[3] > 64 + WALL_HEIGHT,
            "low approach should rise above resting (tops {tops:?})",
        );
        // ... and the high plateau dips below resting height (uses the drop budget).
        assert!(
            tops[4] < 84 + WALL_HEIGHT,
            "high side should dip below resting (tops {tops:?})",
        );
        let big_steps = (0..n)
            .filter(|&i| (tops[i] - tops[(i + 1) % n]).abs() > 1)
            .count();
        assert_eq!(big_steps, 2, "a plateau leaves one residual step up and one down (tops {tops:?})");
        assert_invariants(&terrain);
    }

    // ---- wall-ring tracing ----

    /// A solid axis-aligned rectangle of cells [x0,x1) × [z0,z1).
    fn filled_rect(x0: i32, z0: i32, x1: i32, z1: i32) -> HashSet<Point2D> {
        let mut set = HashSet::new();
        for x in x0..x1 {
            for z in z0..z1 {
                set.insert(Point2D::new(x, z));
            }
        }
        set
    }

    /// Every consecutive pair (including the wrap) is 4-adjacent — i.e. the ring
    /// has no diagonal seam.
    fn assert_four_connected_ring(loop_: &[Point2D]) {
        let n = loop_.len();
        for i in 0..n {
            let a = loop_[i];
            let b = loop_[(i + 1) % n];
            let d = (a.x - b.x).abs() + (a.y - b.y).abs();
            assert_eq!(d, 1, "non-4-connected step {a:?} -> {b:?}");
        }
    }

    #[test]
    fn rectangle_traces_one_closed_4connected_loop() {
        let region = filled_rect(0, 0, 10, 6);
        let loops = trace_wall_loops(&region);
        assert_eq!(loops.len(), 1, "a solid rectangle is one ring");
        let ring = &loops[0];
        assert_four_connected_ring(ring);
        // The boundary of a 10×6 rectangle is its perimeter cells: 2*(10+6) - 4 = 28.
        let unique: HashSet<_> = ring.iter().cloned().collect();
        assert_eq!(unique.len(), 28, "ring should be exactly the perimeter cells");
        // Every traced cell is inside the region (no spurious outside cells here).
        assert!(ring.iter().all(|p| region.contains(p)));
    }

    #[test]
    fn no_boundary_cell_is_dropped() {
        // The old greedy walk discarded arcs on a dead end; assert every edge cell
        // of the region appears in some traced loop.
        let region = filled_rect(0, 0, 14, 9);
        let edge = get_edge(&region);
        let traced: HashSet<Point2D> =
            trace_wall_loops(&region).into_iter().flatten().collect();
        for e in &edge {
            assert!(traced.contains(e), "edge cell {e:?} was dropped from the wall ring");
        }
    }

    #[test]
    fn diagonal_corner_is_seam_filled() {
        // An L / staircase region forces a diagonal step in the raw contour; the
        // densified ring must still be fully 4-connected (Bug 3).
        let mut region = filled_rect(0, 0, 8, 8);
        // Carve a stepped notch out of a corner to create diagonal boundary runs.
        region.remove(&Point2D::new(7, 0));
        region.remove(&Point2D::new(7, 1));
        region.remove(&Point2D::new(6, 0));
        let loops = trace_wall_loops(&region);
        assert_eq!(loops.len(), 1);
        assert_four_connected_ring(&loops[0]);
    }

    #[test]
    fn separate_blobs_become_separate_loops() {
        let mut region = filled_rect(0, 0, 8, 8);
        region.extend(filled_rect(20, 20, 28, 28));
        let loops = trace_wall_loops(&region);
        assert_eq!(loops.len(), 2, "two disjoint blobs -> two rings");
        for ring in &loops {
            assert_four_connected_ring(ring);
        }
    }

    #[test]
    fn tiny_specks_are_dropped() {
        // A lone 2×2 blob is below MIN_WALL_LOOP and should not yield a ring.
        let region = filled_rect(0, 0, 2, 2);
        assert!(trace_wall_loops(&region).is_empty());
    }

    // ---- walkway support fill (Plan A) ----

    /// `walkway_support_columns` mirror of the cap → support-block mapping used in
    /// the assertions below: the highest full block under a cell's slab cap.
    fn support_top(h: f64) -> i32 {
        h.floor() as i32 - 1
    }

    /// Build a height map laid along x at z=0 from a surface profile.
    fn heights_along_x(surfaces: &[f64]) -> HashMap<Point2D, f64> {
        surfaces
            .iter()
            .enumerate()
            .map(|(i, &h)| (Point2D::new(i as i32, 0), h))
            .collect()
    }

    /// The core invariant: between any two orthogonally adjacent walkway cells the
    /// taller one's fill must reach down to at least the shorter one's support top,
    /// so the connecting vertical face is solid (no walkway hole). Also every cell
    /// keeps at least one support block.
    fn assert_no_open_faces(heights: &HashMap<Point2D, f64>) {
        let cols: HashMap<Point2D, (i32, i32)> = walkway_support_columns(heights)
            .into_iter()
            .map(|(p, bottom, top)| (p, (bottom, top)))
            .collect();
        for (&point, &(bottom, top)) in &cols {
            assert!(bottom <= top, "cell {point:?} has empty support span");
            for direction in CARDINALS_2D {
                let neighbour = point + Point2D::from(direction);
                let Some(&(_nb_bottom, nb_top)) = cols.get(&neighbour) else {
                    continue;
                };
                if top >= nb_top {
                    // This cell is the taller (or equal) one: its fill must cover
                    // the neighbour's top so the shared face has no air gap.
                    assert!(
                        bottom <= nb_top,
                        "open face between {point:?} (fill {bottom}..={top}) and \
                         {neighbour:?} (top {nb_top})",
                    );
                }
            }
        }
    }

    #[test]
    fn flat_walkway_each_cell_self_supported() {
        let heights = heights_along_x(&[70.0; 6]);
        for (_p, bottom, top) in walkway_support_columns(&heights) {
            assert_eq!(bottom, top, "flat run needs only a single support block");
            assert_eq!(top, support_top(70.0));
        }
        assert_no_open_faces(&heights);
    }

    #[test]
    fn gentle_slope_has_no_open_faces() {
        assert_no_open_faces(&heights_along_x(&[70.0, 71.0, 72.0, 73.0, 74.0]));
        // half-step slabs (top-slab fractional surfaces) interleaved
        assert_no_open_faces(&heights_along_x(&[70.0, 70.49, 71.0, 71.49, 72.0]));
    }

    #[test]
    fn cliff_face_is_filled_solid() {
        // A gap-guard cliff: a sudden ~18-block jump mid-run. The fill must bridge
        // the whole face rather than leaving it open (the screenshot holes).
        let heights = heights_along_x(&[70.0, 71.0, 72.0, 90.0, 91.0, 92.0]);
        assert_no_open_faces(&heights);
        // The tall cell at the cliff must drop its fill all the way to the low side.
        let cols: HashMap<Point2D, (i32, i32)> = walkway_support_columns(&heights)
            .into_iter()
            .map(|(p, b, t)| (p, (b, t)))
            .collect();
        let (bottom, _top) = cols[&Point2D::new(3, 0)];
        assert!(
            bottom <= support_top(72.0),
            "cliff cell fill {bottom} should reach the low neighbour's top {}",
            support_top(72.0),
        );
    }

    #[test]
    fn two_dimensional_walkway_band_has_no_open_faces() {
        // A 3-wide band that steps up along its length — exercises both axes.
        let mut heights = HashMap::new();
        for x in 0..8 {
            let surface = 64.0 + x as f64; // 1 per cell along x
            for z in 0..3 {
                heights.insert(Point2D::new(x, z), surface);
            }
        }
        assert_no_open_faces(&heights);
    }
}