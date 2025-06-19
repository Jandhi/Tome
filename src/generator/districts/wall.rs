use std::{collections::{HashMap, HashSet}, env};
use std::path::Path;
use log::info;
use crate::{editor::World, generator::{districts::wall, materials::{MaterialPlacer, Placer}, nbts::place_nbt_without_palette, BuildClaim}, geometry::{get_neighbours_in_set, get_outer_points, is_straight_not_diagonal_point2d, Point2D, Point3D, EAST_2D, NORTH_2D}, minecraft::{Block, BlockForm, BlockID}, noise::RNG};

use crate::editor::Editor;

pub fn get_wall_points(
    inner_points: &HashSet<Point2D>,
    editor: &mut Editor,
) -> (HashSet<Point2D>) {
    let mut wall_points = get_outer_points(inner_points);

    // Collect points to remove to avoid mutating while iterating
    let mut to_remove = Vec::new();

    for point in &wall_points {
        editor.world_mut().claim(*point, BuildClaim::Wall); // mark wall points as claimed
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

pub async fn build_wall(urban_points: &HashSet<Point2D>, editor: &mut Editor, rng : &mut RNG, material_placer: &mut MaterialPlacer<'_>){
    let wall_points = get_wall_points(urban_points, editor);
    let ordered_wall_points = order_wall_points(&wall_points);

    for wall_point_list in ordered_wall_points {
        build_wall_palisade(&wall_point_list, editor, rng, material_placer).await;
    }
}

pub async fn build_wall_palisade(wall_points: &Vec<Point2D>, editor: &mut Editor, rng: &mut RNG, material_placer: &mut MaterialPlacer<'_>) {
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
            BlockForm::Log, None, None).await;
    material_placer.place_blocks(
            editor, 
            top_points.into_iter(),
            BlockForm::Fence, None, None).await;


    //add gates
    build_wall_gate(&wall_points_with_world_height, editor, rng, material_placer, false, true, None).await

}

pub async fn build_wall_gate(
    wall_points: &Vec<Point3D>,
    editor: &mut Editor,
    rng: &mut RNG,
    material_placer: &MaterialPlacer<'_>,
    is_thin: bool,
    is_palisade: bool,
    inner_wall_set: Option<&HashSet<Point2D>>,
) {
    let distance_to_next_gate = 60;
    let gate_size = 7; // eventually this should depend on some type of gate we place
    let mut gate_possible = 0;
    let gate_height = 10; // height of the gate
    let palisade_gate = env::current_dir().expect("Should get current dir")
            .join("data").join("structures").join("city_wall").join("basic_palisade_gate.nbt");
    let thin_gate = env::current_dir().expect("Should get current dir")
            .join("data").join("structures").join("city_wall").join("basic_thin_gate.nbt");
    let wide_gate = env::current_dir().expect("Should get current dir")
            .join("data").join("structures").join("city_wall").join("basic_wide_gate.nbt");

    let air = Block {
            id: BlockID::Air,
            data: None,
            state: None,
    };
     let bedrock = Block {
            id: BlockID::Bedrock,
            data: None,
            state: None,
    };

    for (i, point) in wall_points.iter().enumerate() {
        if gate_possible == 0 {
            if is_gate_possible(*point, wall_points, gate_size, i) {
                if is_palisade {
                    let middle_point = Point3D::new(wall_points[i+2].x, editor.world().get_height_at(wall_points[i+2].drop_y()) - 1, wall_points[i+2].z);
                    let direction: Point2D;
                    let neighbours: Vec<Point2D>;
                    if point.x == wall_points[i + 6].x {
                        direction = EAST_2D;
                    } else {
                        direction = NORTH_2D;
                    }
                    if direction == NORTH_2D {
                        neighbours = ((middle_point.x - 2)..=(middle_point.x + 2))
                            .flat_map(|x| {
                                ((middle_point.z - 1)..=(middle_point.z + 1))
                                    .map(move |z| Point2D { x, y: z })
                            })
                            .collect::<Vec<Point2D>>();
                    } else {
                        neighbours = ((middle_point.x - 1)..=(middle_point.x + 1))
                            .flat_map(|x| {
                                ((middle_point.z - 2)..=(middle_point.z + 2))
                                    .map(move |z| Point2D { x, y: z })
                            })
                            .collect::<Vec<Point2D>>();
                    }
                    let height = middle_point.y + 1;
                    for height in height..height + gate_height {
                        for neighbour in neighbours.iter() {
                            editor.place_block(
                                &air,
                                neighbour.add_y(height)
                            ).await;
                        }

                    }
                    println!("Placing palisade gate at: {:?}", middle_point);
                    
                    place_nbt_without_palette(Path::new(&palisade_gate), middle_point.into(), editor)
                    .await
                    .expect("Failed to place gate");

                    editor.place_block(&bedrock, Point3D { x: middle_point.x, y: middle_point.y + 5, z: middle_point.z }).await;




                } else if is_thin {
                    continue; // thin gates are not implemented yet
                } else {
                    continue; // thin gates are not implemented yet
                }
                gate_possible = distance_to_next_gate;
            }


            
        } else {
            gate_possible -= 1;
        }
    }
}

pub fn is_gate_possible(
    point: Point3D,
    wall_list: &Vec<Point3D>,
    gate_size: i32,
    index: usize,
) -> bool {
    // Check if the point is a valid gate position
    if index + gate_size as usize > wall_list.len() {
        return false; // Not enough points to form a gate, doesnt loop
    }
    println!("Checking gate at index: {}, point: {:?}", index, point);
    println!("{:?}", is_straight_not_diagonal_point2d(
        Point2D { x: point.x, y: point.z },
        Point2D { x: wall_list[index + gate_size as usize - 1].x, y: wall_list[index + gate_size as usize - 1].z },
        gate_size - 1,
    ));
    println!("{:?}", (point.y - wall_list[index + gate_size as usize - 1].y).abs());
    // Check if the point is straight and not diagonal
    if is_straight_not_diagonal_point2d(
        Point2D { x: point.x, y: point.z },
        Point2D { x: wall_list[index + gate_size as usize - 1].x, y: wall_list[index + gate_size as usize - 1].z },
        gate_size - 1,
    ) && (point.y - wall_list[index + gate_size as usize - 1].y).abs() <= 1 {
        return true;
    }

    false
}