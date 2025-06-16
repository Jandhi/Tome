use std::collections::{HashMap, HashSet};

use crate::{editor::Editor, generator::{data::LoadedData, materials::{MaterialPlacer, Placer}, paths::path::Path}, geometry::{get_surrounding_set, Point2D, Point3D, DOWN, UP}, minecraft::{BlockForm, BlockID}, util::MeanExt};

pub async fn build_path(
    editor : &mut Editor,
    data : &LoadedData,
    path : &Path,
) {
    let mut points_2d = path.points()
        .iter()
        .map(|p| p.drop_y())
        .collect::<HashSet<_>>();

    for point in get_surrounding_set(&points_2d, path.width() - 1).iter().filter(|p| editor.world().is_in_bounds_2d(**p)) {
        points_2d.insert(*point);
    }

    let mut height_by_point = path.points()
        .iter()
        .map(|p| {
            (p.drop_y(), p.y as f32)
        })
        .collect::<HashMap<Point2D, f32>>(); 

    for point in points_2d.iter() {
        height_by_point.insert(*point, [point.neighbours(), vec![*point]].concat().iter()
            .filter(|&neighbour| height_by_point.contains_key(neighbour))
            .map(|neighbour| {
                height_by_point[neighbour]
            })
            .mean()); 
    }

    for point in points_2d.iter() {
        height_by_point.insert(*point, [point.neighbours(), vec![*point]].concat().iter()
            .filter(|&neighbour| height_by_point.contains_key(neighbour))
            .map(|neighbour| {
                height_by_point[neighbour]
            })
            .mean()); 
    }

    for point in points_2d.iter() {
        if point.neighbours().iter().all(|neighbour| !height_by_point.contains_key(neighbour) || height_by_point[neighbour] > height_by_point[&point]) {
            height_by_point.insert(*point, height_by_point[point] + 1.0);
            continue;
        }

        if point.neighbours().iter().all(|neighbour| !height_by_point.contains_key(neighbour) || height_by_point[neighbour] < height_by_point[&point]) {
            height_by_point.insert(*point, height_by_point[point] - 1.0);
            continue;
        }
    }

    for point in points_2d.iter() {
        height_by_point.insert(*point, [point.neighbours(), vec![*point]].concat().iter()
            .filter(|&neighbour| height_by_point.contains_key(neighbour))
            .map(|neighbour| {
                height_by_point[neighbour]
            })
            .mean()); 
    }

    let placer = MaterialPlacer::new(
        Placer::new(&data.materials), 
        path.material().clone()
    );

    for point in points_2d.iter() {
        let height = height_by_point.get(point).cloned().expect("Height for point should be calculated");
        let int_height = height.floor() as i32;
        let point3d = Point3D {
            x: point.x,
            y: int_height,
            z: point.y,
        };

        let remainder = height - int_height as f32;

        

        let world_height = editor.world().get_height_at(*point);

        for i in 0..=3 {
            editor.place_block(&BlockID::Air.into(), point3d + UP * i).await;
        }

        if remainder > 0.3 {
            placer.place_block(editor, point3d, BlockForm::Slab, None, None).await;
        }

        placer.place_block(editor, point3d + DOWN, BlockForm::Block, None, None).await;
    }
}