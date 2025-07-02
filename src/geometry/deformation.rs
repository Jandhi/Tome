use std::collections::{HashMap, HashSet};

use crate::geometry::Point3D;

pub fn average_to_neighbours(points : &HashSet<Point3D>) -> HashSet<Point3D> {
    let height_by_point = points.iter()
        .map(|point| (point.drop_y(), point.y))
        .collect::<HashMap<_, _>>();

    points.iter()
        .map(|point| {
            let mut total_height = point.y;
            let mut count = 1;

            for neighbour in point.neighbours_2d() {
                if let Some(height) = height_by_point.get(&neighbour.drop_y()) {
                    total_height += height;
                    count += 1;
                }
            }

            if count > 0 {
                Point3D::new(point.x, total_height / count as i32, point.z)
            } else {
                *point
            }
        })
        .collect()
}

pub fn average_to_neighbours_multi(points : &HashSet<Point3D>, iterations: usize) -> HashSet<Point3D> {
    let mut current_points = points.clone();
    
    for _ in 0..iterations {
        current_points = average_to_neighbours(&current_points);
    }
    
    current_points
}

pub fn average_to_neighbours_5_away(points : &HashSet<Point3D>) -> HashSet<Point3D> {
    let mut neighbour_vecs = vec![];

    neighbour_vecs.extend(Point3D::NEIGHBOURS_1_AWAY);
    neighbour_vecs.extend(Point3D::NEIGHBOURS_2_AWAY);
    neighbour_vecs.extend(Point3D::NEIGHBOURS_3_AWAY);
    neighbour_vecs.extend(Point3D::NEIGHBOURS_4_AWAY);
    neighbour_vecs.extend(Point3D::NEIGHBOURS_5_AWAY);

    let height_by_point = points.iter()
        .map(|point| (point.drop_y(), point.y))
        .collect::<HashMap<_, _>>();

    points.iter()
        .map(|point| {
            let mut total_height = point.y;
            let mut count = 1;

            for neighbour in neighbour_vecs.iter().map(|&d| *point + d) {
                if let Some(height) = height_by_point.get(&neighbour.drop_y()) {
                    total_height += height;
                    count += 1;
                }
            }

            if count > 0 {
                Point3D::new(point.x, total_height / count as i32, point.z)
            } else {
                *point
            }
        })
        .collect()
}

pub fn average_to_neighbours_5_away_multi(points : &HashSet<Point3D>, iterations: usize) -> HashSet<Point3D> {
    let mut current_points = points.clone();
    
    for _ in 0..iterations {
        current_points = average_to_neighbours_5_away(&current_points);
    }
    
    current_points
}