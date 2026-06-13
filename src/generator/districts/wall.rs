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

fn find_wall_neighbour(point: Point2D, wall_points: &HashSet<Point2D>, ordered_set: &HashSet<Point2D>) -> Option<Point2D> {
    // checking neighbours in a specific order to ensure consistent ordering
    let directions = [
        Point2D { x: -1, y: 0 },
        Point2D { x: 0, y: -1 },
        Point2D { x: -1, y: -1 },
        Point2D { x: -1, y: 1 },
        Point2D { x: 1, y: -1 },
        Point2D { x: 1, y: 0 },
        Point2D { x: 0, y: 1 },
        Point2D { x: 1, y: 1 },
    ];

    // Check all neighbours of the point in the wall points
    for direction in directions.iter() {
        let neighbour = point + *direction;
        if !ordered_set.contains(&neighbour) && wall_points.contains(&neighbour) {
            return Some(neighbour);
        }
    }
    None
}

pub fn order_wall_points(
    wall_points: & HashSet<Point2D>,
) -> Vec<Vec<Point2D>> {
    let mut list_of_ordered_vec = Vec::new();

    let mut wall_point_list = wall_points.iter().cloned().collect::<Vec<_>>();
    let mut ordered_vec = Vec::new();
    let mut ordered_set = HashSet::new();
    // No wall points (e.g. a degenerate / fully-built urban region) — nothing to order.
    if wall_point_list.is_empty() {
        return list_of_ordered_vec;
    }
    let mut current_point = wall_point_list.remove(0);

    ordered_vec.push(current_point);
    ordered_set.insert(current_point);

    let mut reverse_check = false;

    while wall_point_list.len() > 0 {
        let next_wall_point = find_wall_neighbour(current_point, wall_points, &ordered_set);
        if next_wall_point.is_none() {
            // If no next point is found, we need to reverse the direction
            if reverse_check {
                info!("Failed to find a neighbour");
                if ordered_vec.len() > 20 {
                    // Killing small wall structures, shouldnt really need to be here since those small urban sections shouldnt happen
                    list_of_ordered_vec.push(ordered_vec.clone());
                }
                ordered_vec.clear();
                current_point = wall_point_list.remove(0);
                ordered_vec.push(current_point);
                ordered_set.insert(current_point);
                break; // If we already reversed, we are done
            } else {
                info!("Reversing wall");
                reverse_check = true;
                // Reverse the order of the ordered_vec
                ordered_vec.reverse();
                current_point = ordered_vec.first().cloned().unwrap();
                continue; 
            }
        
        } else {
            wall_point_list.retain(|p| *p != next_wall_point.unwrap());
            ordered_vec.push(current_point);
            ordered_set.insert(current_point);
            current_point = next_wall_point.unwrap();
        }
    }

    list_of_ordered_vec.push(ordered_vec);
    list_of_ordered_vec
}

pub async fn build_wall(urban_points: &HashSet<Point2D>, editor: &mut Editor, rng : &mut RNG, material_placer: &mut Placer<'_>, material_id: &MaterialId, structures: & HashMap<StructureType, Structure>, wall_type: WallType) {
    let wall_points = get_wall_points(urban_points, editor);
    info!("[Wall] Found {} wall points", wall_points.len());
    let ordered_wall_points = order_wall_points(&wall_points);

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
    let enhanced_wall_points = check_water(&mut add_wall_points_directionality(&wall_points_with_height, &HashSet::from_iter(wall_points.iter().cloned()), urban_points), editor);

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
            for y in editor.world().get_height_at(point.drop_y())..=point.y {
                let new_point = Point3D { x: point.x, y, z: point.z };
                material_placer.place_block(editor, new_point, material_id, BlockForm::Block, None, None).await;
            }
            if directions.len() > 0 {
                previous_dir = directions[0];
            }
            let state = HashMap::from([("facing".to_string(), previous_dir.rotate_right().to_string())]);
            material_placer.place_block(editor, Point3D { x: point.x, y: point.y + 1, z: point.z }, material_id, BlockForm::Stairs, Some(&state), None).await;
        
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
    let enhanced_wall_points = check_water(&mut add_wall_points_directionality(&wall_points_with_height, &HashSet::from_iter(wall_points.iter().cloned()), urban_points), editor);

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
            for y in editor.world().get_height_at(point.drop_y())..=point.y {
                let new_point = Point3D { x: point.x, y, z: point.z };
                material_placer.place_block(editor, new_point, material_id, BlockForm::Block, None, None).await;
            }
            if directions.len() > 0 {
                previous_dir = directions[0];
            }
            let state = HashMap::from([("facing".to_string(), previous_dir.rotate_right().to_string())]);
            material_placer.place_block(editor, Point3D { x: point.x, y: point.y + 1, z: point.z }, material_id, BlockForm::Stairs, Some(&state), None).await;
        
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


/// Adds height to wall points based on a heightmap, smoothing transitions.
/// Returns a Vec<Point3D> where `.y` is the wall *top* height at each point.
///
/// The base height tracks the terrain but is rate-limited to MAX_STEP per point in
/// *both* directions, so adjacent wall columns can never differ in top height by
/// more than MAX_STEP. This is what prevents the tall single-block spikes: a sharp
/// terrain change is spread over several points instead of jumping in one step. The
/// only exception is the gap guard, which pulls the base up on a genuine cliff so
/// the wall always covers the ground — and only by as much as that requires.
pub fn add_wall_points_height(
    wall_points: &[Point2D],
    editor: &mut Editor,
) -> Vec<Point3D> {
    // Max change in wall-top height between adjacent points, in either direction.
    const MAX_STEP: i32 = 1;
    // Minimum wall thickness that must remain above the terrain. If the rate-limited
    // base drops so far below a rising cliff that less than this would be left, the
    // base is forced up just enough to keep it.
    const MIN_CLEARANCE: i32 = 3;

    let mut current_height = editor.world().get_height_at(wall_points[0]);
    let mut height_wall_points = Vec::with_capacity(wall_points.len());

    for point in wall_points {
        let target_height = editor.world().get_height_at(*point);

        // Step the base toward the terrain, capped to MAX_STEP per point so a steep
        // change is smeared across several points rather than producing a spike.
        let delta = (target_height - current_height).clamp(-MAX_STEP, MAX_STEP);
        current_height += delta;

        // Gap guard: on a cliff the rate-limited base can fall far enough below the
        // terrain that the wall would no longer cover it — pull it up just enough to
        // keep MIN_CLEARANCE of wall above the ground.
        if current_height + WALL_HEIGHT < target_height + MIN_CLEARANCE {
            current_height = target_height + MIN_CLEARANCE - WALL_HEIGHT;
        }

        height_wall_points.push(Point3D {
            x: point.x,
            y: current_height + WALL_HEIGHT,
            z: point.y,
        });
    }

    height_wall_points
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
    updated_walkway_heights

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

    // Mirror the private consts inside `add_wall_points_height`.
    const MAX_STEP: i32 = 1;
    const MIN_CLEARANCE: i32 = 3;

    /// Run `add_wall_points_height` over a 1-D terrain profile laid along x at
    /// z=1, returning the wall-top height at each point.
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

    /// Assert the three correctness invariants over any terrain profile:
    /// (1) the wall always covers the ground, (2) the top never drops by more
    /// than MAX_STEP (no downward spikes), and (3) any upward jump bigger than
    /// MAX_STEP is forced by the gap guard — it lands exactly at the minimum
    /// clearance, never gratuitously.
    fn assert_invariants(terrain: &[i32]) {
        let tops = tops_for(terrain);
        assert_eq!(tops.len(), terrain.len());
        for i in 0..terrain.len() {
            assert!(
                tops[i] >= terrain[i] + MIN_CLEARANCE,
                "point {i}: top {} does not cover terrain {} + clearance {MIN_CLEARANCE} (profile {terrain:?})",
                tops[i], terrain[i],
            );
            if i > 0 {
                assert!(
                    tops[i] >= tops[i - 1] - MAX_STEP,
                    "point {i}: downward spike {} -> {} (profile {terrain:?})",
                    tops[i - 1], tops[i],
                );
                if tops[i] > tops[i - 1] + MAX_STEP {
                    assert_eq!(
                        tops[i], terrain[i] + MIN_CLEARANCE,
                        "point {i}: upward spike not pinned to gap guard (profile {terrain:?})",
                        );
                }
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
        assert_invariants(&[60, 61, 62, 63, 64, 65, 66, 67]); // gentle rise
        assert_invariants(&[70, 69, 68, 67, 66, 65, 64, 63]); // gentle fall
        assert_invariants(&[64, 65, 66, 65, 64, 65, 66, 65]); // rolling
    }

    #[test]
    fn cliffs_hold_invariants() {
        assert_invariants(&[64, 64, 64, 90, 90, 90]); // up-cliff (+26)
        assert_invariants(&[90, 90, 90, 64, 64, 64]); // down-cliff (-26)
        assert_invariants(&[64, 90, 64, 90, 64, 90]); // alternating spikes
        assert_invariants(&[64, 64, 100, 64, 64]); // single tall spike
    }

    #[test]
    fn up_cliff_does_not_overshoot() {
        // A sharp rise is rate-limited until the gap guard must intervene; once
        // it does, the top hugs terrain + clearance rather than overshooting.
        let terrain = [64, 64, 64, 64, 90, 90, 90, 90];
        let tops = tops_for(&terrain);
        for (i, &t) in tops.iter().enumerate() {
            assert!(t <= terrain[i].max(64) + WALL_HEIGHT,
                "point {i}: top {t} overshoots above wall height over terrain {}", terrain[i]);
        }
    }

    #[test]
    fn down_cliff_stays_tall_then_recovers() {
        // The base lags a steep drop by one block per point, so the wall is tall
        // over the low ground and steps back down toward the terrain gradually.
        // Recovery is 1 block/point, so the flat run must be longer than the drop
        // (here a drop of 6 over 8 flat points) for the wall to fully settle.
        let terrain = [70, 70, 64, 64, 64, 64, 64, 64, 64, 64];
        let tops = tops_for(&terrain);
        // Right after the drop the wall is much taller than its resting height.
        assert!(tops[2] > 64 + WALL_HEIGHT, "wall should stay tall right after the drop");
        // Given enough flat run it settles back to the resting height.
        assert_eq!(*tops.last().unwrap(), 64 + WALL_HEIGHT);
        // And it only ever steps down by MAX_STEP at a time.
        for i in 1..tops.len() {
            assert!(tops[i] >= tops[i - 1] - MAX_STEP);
        }
    }
}