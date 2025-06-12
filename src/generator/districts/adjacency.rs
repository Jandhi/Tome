use std::collections::HashMap;

use crate::geometry::{Point3D, Rect2D, X_PLUS_2D, Y_PLUS_2D};

// We use this trait to allow various regions to be analyzed for adjacency
pub trait AdjacencyAnalyzeable<TID> {
    fn increment_adjacency(&mut self, id : Option<TID>);
    fn add_edge(&mut self, point : Point3D);
}

// We do a sweep of the world in the x+ and z+ directions, checking for district adjacency
pub fn analyze_adjacency<TID, TAnalyzeable>(objects : &mut HashMap<TID, TAnalyzeable>, height_map : &Vec<Vec<i32>>, map : &Vec<Vec<Option<TID>>>, world_rect : &Rect2D, ignore_edge_addition : bool) 
    where 
        TID: Copy + std::hash::Hash + Eq,
        TAnalyzeable: AdjacencyAnalyzeable<TID>,
{
    for point in world_rect.iter() {
        if map[point.x as usize][point.y as usize].is_none() {
            continue;
        }

        let id = map[point.x as usize][point.y as usize].expect("This should be here");
        let mut is_edge = false;
        let height = height_map[point.x as usize][point.y as usize];

        for neighbour_point in [point + X_PLUS_2D, point + Y_PLUS_2D, point - X_PLUS_2D, point - Y_PLUS_2D] {
            if !world_rect.contains(neighbour_point) {
                continue;
            }

            // If the neighbour is empty, only increment the adjacency count
            if map[neighbour_point.x as usize][neighbour_point.y as usize].is_none() {
                objects.get_mut(&id).expect("Could not find region with id").increment_adjacency(None);
                is_edge = true;
                continue;
            }

            let neighbour_district_id = map[neighbour_point.x as usize][neighbour_point.y as usize].expect("This should be here");

            if neighbour_district_id == id {
                continue;
            }

            is_edge = true;

            let neighbour_height = height_map[neighbour_point.x as usize][neighbour_point.y as usize];

            // If the neighbour is not walkable from this point
            // TODO: Consider whether this is useful
            if (neighbour_height - height).abs() > 1 {
                continue;
            }

            // Add ajacency to both regions
            objects.get_mut(&id).expect("Could not find region with id").increment_adjacency(Some(neighbour_district_id));
            objects.get_mut(&neighbour_district_id).expect("Could not find region with id").increment_adjacency(Some(id));
        }

        if is_edge && !ignore_edge_addition {
            let item = objects.get_mut(&id).expect("Could not find region with id");
            item.add_edge(Point3D::new(point.x, height, point.y));
        }
    }
}