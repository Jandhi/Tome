use lerp::num_traits::clamp;

use crate::{editor::Editor, generator::{materials::MaterialId, paths::{a_star, path::{Path, PathPriority}}}, geometry::{Point2D, Point3D, ALL_8}};

fn mod4_point(point : Point3D, editor : &Editor) -> Point3D {
    let point = Point2D{
        x : point.x - point.x.rem_euclid(4),
        y : point.z - point.z.rem_euclid(4),
    };

    editor.world().add_height(point)
}

fn get_best_mod4_point(point : Point3D, editor : &Editor) -> Point3D {
    vec![(0, 0), (0, 4), (4, 0), (4, 4)]
        .into_iter()
        .map(|(dx, dz)| Point3D {
            x: point.x + dx,
            y: point.y,
            z: point.z + dz,
        })
        .filter(|p| editor.world().is_in_bounds_2d(p.drop_y()))
        .map(|p| mod4_point(p, editor))
        .min_by_key(|p| {
            p.y.abs_diff(point.y)
        })
        .unwrap_or(mod4_point(point, editor))
}

pub async fn get_path(
    editor: &Editor,
    start: Point3D,
    end: Point3D,
    priority : PathPriority,
    material : MaterialId,
    explore_callback: impl AsyncFnMut(&Vec<Point3D>)
) -> Option<Path> {
    let new_start = get_best_mod4_point(start, editor);
    let new_end = get_best_mod4_point(end, editor);

    let width = match priority {
        PathPriority::Low => 1,
        PathPriority::Medium => 2,
        PathPriority::High => 3,
    };

    let mut path = route_path(editor, new_start, new_end, explore_callback).await?;

    if !path.is_empty() {
        path = fill_out_path(path, priority != PathPriority::Low);
    }

    Some(Path::new(
        path,
        width,
        material,
        priority,
    ))
}

pub async fn route_path(
    editor: &Editor,
    start : Point3D,
    end: Point3D,
    explore_callback: impl AsyncFnMut(&Vec<Point3D>)
) -> Option<Vec<Point3D>> {
    let new_start = get_best_mod4_point(start, editor);
    let new_end = get_best_mod4_point(end, editor);

    let heuristic_weight = 10;

    let get_neighbours_4 = |points : &Vec<Point3D>| {
        let mut neighbours = vec![];
        let point = points.last().unwrap();

        for direction in ALL_8 {
            let neighbour_2d = point.drop_y() + direction * 4;

            if editor.world().is_in_bounds_2d(neighbour_2d) {
                let mut neighbour = editor.world().add_height(neighbour_2d);

                if !point.y.abs_diff(neighbour.y) > 4 {
                    neighbour.y = clamp(neighbour.y, point.y - 2, point.y + 2);
                    neighbours.push(neighbour);
                    continue;
                }
            }

            let neighbour_2d = point.drop_y() + direction * 2;

             if editor.world().is_in_bounds_2d(neighbour_2d) {
                let mut neighbour = editor.world().add_height(neighbour_2d);

                if !point.y.abs_diff(neighbour.y) > 2 {
                    neighbour.y = clamp(neighbour.y, point.y - 2, point.y + 2);
                    neighbours.push(neighbour);
                    continue;
                }
            }
        }

        neighbours.into_iter().map(|point| {
            let mut new_points = points.clone();
            new_points.push(point);
            new_points
        }).collect::<Vec<Vec<_>>>()
    };

    let get_cost = |prev_cost : u64, points : &Vec<Point3D>| {
        if points.len() < 2 {
            return 0;
        }

        let last = points[points.len() - 1];
        let prev = points[points.len() - 2];
        let mut cost = prev_cost + last.distance(prev) as u64;


        if points.len() >= 3 {
            let prev_prev = points[points.len() - 3];
            
            let wobble = (last - prev).distance(prev - prev_prev) as u64;
            cost += wobble;
        }

        let burrowing_cost = editor.world().get_height_at(last.drop_y()).abs_diff(last.y) as u64 * 10;
        cost += burrowing_cost;

        let height_diff = last.y.abs_diff(new_end.y) as u64;
        cost += height_diff * 3;

        if editor.world().is_water(last.drop_y()) {
            cost += 30; // Water cost
        }

        cost
    };

    let get_heuristic = |points : &Vec<Point3D>| {
        if points.is_empty() {
            return 0;
        }

        let last = points.last().unwrap();
        last.distance(new_end) as u64 * heuristic_weight
    };

    let is_end = |points : &Vec<Point3D>| {
        if points.is_empty() {
            return false;
        }

        let last = points.last().unwrap();
        last.drop_y() == new_end.drop_y() && last.y.abs_diff(new_end.y) <= 4
    };

    a_star(
        vec![new_start],
        is_end,
        get_neighbours_4,
        get_cost,
        get_heuristic,
        explore_callback
    ).await
}


pub fn fill_out_path(mut points: Vec<Point3D>, allow_diagonals: bool) -> Vec<Point3D> {
    if points.is_empty() {
        return vec![];
    }
    let mut curr_point = points.remove(0);
    let mut full_points = vec![curr_point];
    if points.is_empty() {
        return full_points;
    }
    let mut next_point = points.remove(0);

    let mut x_axis_first = true;
    let mut can_update_y = true;

    while !points.is_empty() || curr_point != next_point {
        if can_update_y {
            if curr_point.y < next_point.y {
                curr_point.y += 1;
                can_update_y = !can_update_y;
            } else if curr_point.y > next_point.y {
                curr_point.y -= 1;
                can_update_y = !can_update_y;
            }
        } else {
            can_update_y = true;
        }

        if allow_diagonals {
            if curr_point.x > next_point.x && curr_point.z > next_point.z {
                curr_point.x -= 1;
                curr_point.z -= 1;
                full_points.push(curr_point);
                continue;
            }
            if curr_point.x < next_point.x && curr_point.z < next_point.z {
                curr_point.x += 1;
                curr_point.z += 1;
                full_points.push(curr_point);
                continue;
            }
            if curr_point.x > next_point.x && curr_point.z < next_point.z {
                curr_point.x -= 1;
                curr_point.z += 1;
                full_points.push(curr_point);
                continue;
            }
            if curr_point.x < next_point.x && curr_point.z > next_point.z {
                curr_point.x += 1;
                curr_point.z -= 1;
                full_points.push(curr_point);
                continue;
            }
        }

        if x_axis_first {
            if curr_point.x < next_point.x {
                curr_point.x += 1;
                full_points.push(curr_point);
                x_axis_first = !x_axis_first;
                continue;
            }
            if curr_point.x > next_point.x {
                curr_point.x -= 1;
                full_points.push(curr_point);
                x_axis_first = !x_axis_first;
                continue;
            }
        }
        if curr_point.z < next_point.z {
            curr_point.z += 1;
            full_points.push(curr_point);
            x_axis_first = !x_axis_first;
            continue;
        }
        if curr_point.z > next_point.z {
            curr_point.z -= 1;
            full_points.push(curr_point);
            x_axis_first = !x_axis_first;
            continue;
        }
        if curr_point.x < next_point.x {
            curr_point.x += 1;
            full_points.push(curr_point);
            x_axis_first = !x_axis_first;
            continue;
        }
        if curr_point.x > next_point.x {
            curr_point.x -= 1;
            full_points.push(curr_point);
            x_axis_first = !x_axis_first;
            continue;
        }

        // curr_point must be equal to next_point
        full_points.push(curr_point);
        if !points.is_empty() {
            next_point = points.remove(0);
        } else {
            break;
        }
    }

    full_points
}