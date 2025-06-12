use std::{collections::{HashMap, HashSet}, hash::Hash};

use log::info;

use crate::{
    editor::Editor,
    geometry::{Point2D, Point3D},
    noise::{RNG, Seed},
    minecraft::{Block, BlockID},
};

pub async fn log_stems(editor: &mut Editor, points: HashSet<Point2D>) {
    for point in points {
        let height = editor.world().get_height_at(point) - 1; // checking ground
        let mut block_id = editor.get_block(Point3D::new(point.x, height, point.y)).id;

        if !block_id.is_tree() {
            continue;
        }
        editor.place_block(&Block::new(BlockID::Air, None, None), Point3D::new(point.x, height, point.y)).await;

        for y in 1..40 {
            block_id = editor.get_block(Point3D::new(point.x, height - y, point.y)).id;
            if block_id.is_tree() {
                editor.place_block(&Block::new(BlockID::Air, None, None), Point3D::new(point.x, height - y, point.y)).await;
            } else if block_id == BlockID::Dirt {
                editor.place_block(&Block::new(BlockID::GrassBlock, None, None), Point3D::new(point.x, height - y, point.y)).await;
            } else if block_id != BlockID::Air {
                continue;
            }
        }
    }
}

pub async fn log_trees(editor: &mut Editor, points: HashSet<Point2D>) {
    for point in points {
        let height = editor.world().get_motion_blocking_height_at(point) - 1; // checking ground
        let mut point3d = Point3D::new(point.x, height, point.y);
        let mut block_id = editor.get_block(point3d).id;

        if !block_id.is_tree_or_leaf() {
            //println!("Skipping block {:?} at point {:?}: block is not a tree or leaf", block_id, point3d);
            editor.place_block(&Block::new(BlockID::BlueWool, None, None), point3d).await;
            continue;
        }
        //println!("Logging tree at point {:?} with block {:?}", point3d, block_id);
        editor.place_block(&Block::new(BlockID::RedWool, None, None), point3d).await;
        for y in 1..40 {
            block_id = editor.get_block(Point3D::new(point.x, height - y, point.y)).id;
            if block_id.is_tree_or_leaf() {
                editor.place_block(&Block::new(BlockID::BlackWool, None, None), Point3D::new(point.x, height - y, point.y)).await;
            } else if block_id == BlockID::Dirt {
                editor.place_block(&Block::new(BlockID::GreenWool, None, None), Point3D::new(point.x, height - y, point.y)).await;
            } else if block_id != BlockID::Air {
                continue;
            }
        }
    }
}