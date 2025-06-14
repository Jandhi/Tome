use std::collections::{HashMap, HashSet};

use crate::{geometry::{Point2D, Point3D, CARDINALS_2D}};

pub fn get_neighbours_in_set(point: Point2D, points: &HashSet<Point2D>) -> Vec<Point2D> {
    let mut neighbours = Vec::new();
    for cardinal in CARDINALS_2D {
        let neighbour = point + cardinal;
        if points.contains(&neighbour) {
            neighbours.push(neighbour);
        }
    }
    neighbours
}

pub fn get_neighbours_not_in_set(point: Point2D, points: &HashSet<Point2D>) -> Vec<Point2D> {
    let mut neighbours = Vec::new();
    for cardinal in CARDINALS_2D {
        let neighbour = point + cardinal;
        if !points.contains(&neighbour) {
            neighbours.push(neighbour);
        }
    }
    neighbours
}

pub fn get_outer_points(points: &HashSet<Point2D>) -> HashSet<Point2D> {
    let mut outer_points = HashSet::new();

    for point in points {
        let neighbours = get_neighbours_in_set(*point, points);
        if neighbours.len() < 4 {
            outer_points.insert(*point);
        }
    }
    outer_points
}

pub fn get_outer_and_inner_points(points: &HashSet<Point2D>, distance: u32) -> (HashSet<Point2D>, HashSet<Point2D>) {
    let mut outer_points = get_outer_points(points);
    let mut visited = outer_points.clone();
    let mut queue = outer_points.iter().map(|p| (*p, 0 as u32)).collect::<Vec<_>>();

    while queue.len() > 0 {
        let (point, edge_distance) = queue.remove(0);
        if edge_distance >= distance {
            continue;
        }

        for direction in CARDINALS_2D {
            let neighbour = point + direction;
            if !points.contains(&neighbour) {
                continue;
            }
            if visited.contains(&neighbour) {
                continue;
            }
            visited.insert(neighbour);
            outer_points.insert(neighbour);
            queue.push((neighbour, edge_distance + 1));
        }
    }

    let inner_points: HashSet<Point2D> = points.difference(&outer_points).cloned().collect();

    (outer_points, inner_points)
}

pub fn is_straight_point2d(first: Point2D, second: Point2D, length: i32) -> bool {
    let line = first - second;
    ((line.x.abs() == length || line.y.abs() == length) && (line.x == 0 || line.y == 0)) || 
        (line.x.abs() == length && line.y.abs() == length)
}

pub fn is_straight_not_diagonal_point2d(first: Point2D, second: Point2D, length: i32) -> bool {
    let line = first - second;
    (line.x.abs() == length || line.y.abs() == length) && (line.x == 0 || line.y == 0)
}

// include diagonal points
pub fn is_point_surrounded_by_points(
    point: Point2D, points: &HashSet<Point2D>
) -> bool {
    // Define all 8 directions (cardinals + diagonals)
    let directions = [
        Point2D { x: 1, y: 0 },
        Point2D { x: -1, y: 0 },
        Point2D { x: 0, y: 1 },
        Point2D { x: 0, y: -1 },
        Point2D { x: 1, y: 1 },
        Point2D { x: 1, y: -1 },
        Point2D { x: -1, y: 1 },
        Point2D { x: -1, y: -1 },
    ];

    for direction in directions.iter() {
        let neighbour = point + *direction;
        if !points.contains(&neighbour) {
            return false;
        }
    }
    true
}

pub fn get_surrounding_set(points: &HashSet<Point2D>, distance: u32) -> HashSet<Point2D> {
    let mut surrounding = HashSet::new();
    if distance == 0 {
        return surrounding;
    }

    for point in points {
        for direction in CARDINALS_2D {
            let neighbour = *point + direction;
            if !points.contains(&neighbour) {
                surrounding.insert(neighbour);
            }
        }
    }
    if distance == 1 {
        return surrounding;
    } else {
        return surrounding.union(&get_surrounding_set(points, distance - 1)).cloned().collect();
    }
}
