use std::collections::{HashMap, HashSet};
use log::info;

use crate::{editor::World, generator::{BuildClaim, materials::MaterialPlacer}, noise::RNG, geometry::{get_neighbours_in_set, get_outer_points, Point2D, Point3D,}};

use crate::editor::Editor;

fn get_wall_points(
    inner_points: &HashSet<Point2D>,
    editor: &mut Editor,
) -> (HashSet<Point2D>) {
    let mut wall_points = get_outer_points(inner_points);

    // Collect points to remove to avoid mutating while iterating
    let mut to_remove = Vec::new();

    for point in &wall_points {
        editor.world().claim(*point, BuildClaim::Wall); // mark wall points as claimed
        let neighbours = get_neighbours_in_set(*point, inner_points);
        if neighbours.len() == 1 { // supposed to remove extra points
            to_remove.push(*point);
        }
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

pub async fn build_wall(urban_points: &HashSet<Point2D>, editor: &mut Editor, rng : &RNG, MaterialPlacer: & MaterialPlacer<'_>){
    let mut wall_points = get_wall_points(urban_points, editor);
    let ordered_wall_points = order_wall_points(&wall_points);

    for wall_point_list in ordered_wall_points {
        continue
    }
}