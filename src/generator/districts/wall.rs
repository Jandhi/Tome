use std::{collections::{HashMap, HashSet}, env, hash::Hash};
use std::path::Path;
use log::info;
use crate::{editor::World, generator::{buildings::walls::Wall, districts::{build_wall_gate, wall}, materials::{MaterialId, Placer}, nbts::{place_structure, Structure, StructureId}, BuildClaim}, geometry::{get_neighbours_in_set, get_outer_points, is_straight_point2d, Cardinal, Point2D, Point3D, CARDINALS_2D, EAST_2D, NORTH_2D}, minecraft::{Block, BlockForm, BlockID}, noise::RNG};

use crate::editor::Editor;

pub const WALL_HEIGHT: i32 = 10; // optimal height of wall, will change based on smoothing and heightmap
pub const WATER_CHECK: usize = 5;
pub const RANGE: i32 = 3;  // range for walkway flattening

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WallType { //used for both interal wall calculations and for choosing wall type to build
    Water,
    WaterWall,
    Standard,
    Palisade,
}

pub fn get_wall_points(
    inner_points: &HashSet<Point2D>,
    editor: &mut Editor,
) -> (HashSet<Point2D>) {
    let mut wall_points = get_outer_points(inner_points);

    // Collect points to remove to avoid mutating while iterating
    let mut to_remove = Vec::new();

    for point in &wall_points {
        editor.world().claim(*point, BuildClaim::Wall); // mark wall points as claimed
        //let neighbours = get_neighbours_in_set(*point, inner_points);
        //if neighbours.len() == 1 { // supposed to remove extra points
        //    to_remove.push(*point);
        //}
    }

    for point in to_remove {
        wall_points.remove(&point);
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
                reverse_check = false;
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

pub async fn build_wall(urban_points: &HashSet<Point2D>, editor: &mut Editor, rng : &mut RNG, material_placer: & Placer<'_>, material_id: &MaterialId, structures: & HashMap<StructureId, Structure>, wall_type: WallType) {
    let wall_points = get_wall_points(urban_points, editor);
    let ordered_wall_points = order_wall_points(&wall_points);

    for wall_point_list in ordered_wall_points {
        if wall_type == WallType::Standard {
            build_wall_standard(&wall_point_list, editor, rng, material_placer, material_id, structures, urban_points).await;
        } else if wall_type == WallType::Palisade {
            build_wall_palisade(&wall_point_list, editor, rng, material_placer, material_id, structures).await;
        }
    }
}

pub async fn build_wall_palisade(wall_points: &Vec<Point2D>, editor: &mut Editor, rng: &mut RNG, material_placer: &Placer<'_>, material_id: &MaterialId, structures: & HashMap<StructureId, Structure>) {
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
    build_wall_gate(&wall_points_with_world_height, editor, rng, material_placer, false, true, None, structures, 10).await;

}

pub async fn build_wall_standard(wall_points: &Vec<Point2D>, editor: &mut Editor, rng: &mut RNG, material_placer: &Placer<'_>, material_id: &MaterialId, structures: & HashMap<StructureId, Structure>, urban_points: &HashSet<Point2D>) {
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
            let state = HashMap::from([("facing".to_string(), previous_dir.turn_right().to_string())]);
            material_placer.place_block(editor, Point3D { x: point.x, y: point.y + 1, z: point.z }, material_id, BlockForm::Stairs, Some(&state), None).await;
        
            for dir in directions.iter() {
                let mut height_modifier = 0;

                if i > enhanced_wall_points.len() - 1 {
                    let prev_h = enhanced_wall_points[i - 1].0.y;
                    let next_h = enhanced_wall_points[i + 1].0.y;
                    let h = point.y;
                    if prev_h == h -1 && next_h == h - 1 {
                        height_modifier = -1;
                    }
                }
                if directions.contains(&dir.turn_right()) {
                    for new_pt in [
                        point.drop_y() + Point2D::from(*dir) + Point2D::from(dir.turn_right()),
                        point.drop_y() + Point2D::from(*dir) + Point2D::from(dir.turn_right()) * 2,
                        point.drop_y() + Point2D::from(*dir) * 2 + Point2D::from(dir.turn_right())
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
    //add gates
    build_wall_gate(&wall_points_with_height, editor, rng, material_placer, true, false, None, structures, 6).await

}


/// Adds height to wall points based on a heightmap, smoothing transitions.
/// Returns a Vec<Point3D> with smoothed wall heights.
pub fn add_wall_points_height(
    wall_points: &[Point2D],
    editor: &mut Editor,
) -> Vec<Point3D> {
    let mut current_height = editor.world().get_height_at(wall_points[0]);
    let mut target_height = current_height;
    let wall_height_2_3 = WALL_HEIGHT * 2 / 3;
    let mut height_wall_points = Vec::with_capacity(wall_points.len());

    for (i, point) in wall_points.iter().enumerate() {
        if i % 5 == 0 {
            // Wrap around if out of bounds
            let idx5 = if i + 5 >= wall_points.len() - 1 { 0 } else { i + 5 };
            target_height = editor.world().get_height_at(wall_points[idx5]);

            
            if (target_height > current_height + wall_height_2_3)
                || (target_height < current_height + wall_height_2_3)
            {
                let idx10 = if i + 10 >= wall_points.len() - 1 { 0 } else { i + 10 };
                target_height = editor.world().get_height_at(wall_points[idx10]);
            }
        }

        // Check for drastic height difference
        let point_height = editor.world().get_height_at(*point);
        if current_height < point_height - wall_height_2_3 {
            current_height = point_height;
            target_height = current_height;
        } else if current_height != target_height && (1 < i && i < wall_points.len() - 2) {
            if is_straight_point2d(
                wall_points[i - 2],
                wall_points[i + 2],
                4,
            ) {
                if current_height < target_height {
                    current_height += 1;
                } else if current_height > target_height {
                    current_height -= 1;
                }
            }
        }

        let new_point = Point3D {
            x: point.x,
            y: current_height + WALL_HEIGHT,
            z: point.y,
        };
        height_wall_points.push(new_point);
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
    let mut buildable = false;
    let mut long_water = true;

    for i in 0..enhanced_wall_points.len() {
        let point = &enhanced_wall_points[i].0;
        if editor.world().is_water(point.drop_y()) {
            enhanced_wall_points[i].2 = WallType::WaterWall;
            // TO DO, implement more complex logic for water walls
        } else {
            buildable = false;
            long_water = false;
        }
    }
    enhanced_wall_points
}

pub async fn fill_water(
    point: Point2D,
    editor: &mut Editor,
    material_placer: &Placer<'_>,
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
    material_placer: &Placer<'_>,
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
            //let state = HashMap::from([("facing".to_string(), previous_dir.turn_right().to_string())]);
            material_placer.place_block(editor, Point3D { x: point.x, y: height.round() as i32, z: point.y }, material_id, BlockForm::Slab, None, None).await;
            updated_walkway_heights.insert(point, height.round());
        } else if (frac_height > 0.25) && (frac_height <= 0.5) {
            let state = HashMap::from([("type".to_string(), "top".to_string())]);
            material_placer.place_block(editor, Point3D { x: point.x, y: height.round() as i32, z: point.y }, material_id, BlockForm::Slab, Some(&state), None).await;
            updated_walkway_heights.insert(point, height.round() + 0.49);
        } else if (frac_height > 0.5) && (frac_height <= 0.75) {
            //let state = HashMap::from([("facing".to_string(), previous_dir.turn_right().to_string())]);
            material_placer.place_block(editor, Point3D { x: point.x, y: height.round() as i32 - 1, z: point.y }, material_id, BlockForm::Slab, None, None).await;
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
