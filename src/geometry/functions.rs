use std::{collections::HashSet, hash::Hash};

use crate::geometry::{Point2D, ALL_8, CARDINALS_2D};

pub fn get_neighbours_in_set(point: Point2D, points: &HashSet<Point2D>) -> Vec<Point2D> {
    point.neighbours()
        .into_iter()
        .filter(|neighbour| points.contains(neighbour))
        .collect()
}

pub fn get_neighbours_not_in_set(point: Point2D, points: &HashSet<Point2D>) -> Vec<Point2D> {
    point.neighbours()
        .into_iter()
        .filter(|neighbour| !points.contains(neighbour))
        .collect()
}


pub fn get_outer_points(points: &HashSet<Point2D>) -> (HashSet<Point2D>) {
    points.iter()
        .filter(|point| {
            point.neighbours()
                .iter()
                .any(|neighbour| !points.contains(neighbour))
        })
        .cloned()
        .collect()
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
    ALL_8.iter().all(|direction| {
        let neighbour = point + *direction;
        points.contains(&neighbour)
    })
}

pub fn get_surrounding_set(points: &HashSet<Point2D>, distance: u32) -> HashSet<Point2D> {
    if distance == 0 {
        return HashSet::new();
    }

    let surrounding = points.iter()
        .flat_map(|point| point.neighbours())
        .filter(|neighbour| !points.contains(neighbour))
        .collect();

    if distance == 1 {
        return surrounding;
    } else {
        return surrounding
            .union(&get_surrounding_set(&surrounding, distance - 1))
            .copied()
            .collect();
    }
}
