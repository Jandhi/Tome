use std::collections::{HashMap, HashSet};


use log::info;

use crate::{editor::{World, Editor}, generator::BuildClaim, geometry::{Point2D, Point3D, CARDINALS_2D, cardinal_to_str}, minecraft::{Block, BlockID}, noise::RNG, generator::terrain::{Forest, generate_tree}};

pub async fn replace_ground(
    points: &HashSet<Point2D>,
    block_dict: &HashMap<u32, f32>,
    block_list: &Vec<Block>,
    rng: &mut RNG,
    editor: &mut Editor,
    height_offset: Option<i32>,
    permit_blocks: Option<&HashSet<BlockID>>, // should this be a set of blocks to permit or a set of blocks to ignore? currently treated as ignore
    ignore_water: Option<bool>) { //thereotically could be part of permit blocks
        for point in points {
            if editor.world_mut().is_claimed(*point) { // already built on point
                continue;
            }
            if let Some(ignore_water) = ignore_water {
                if editor.world_mut().is_water(*point) && ignore_water {
                    continue; // skip water points if ignore_water is true
                }
            }

            let mut height = editor.world_mut().get_height_at(*point) - 1; // -1 to ensure we are placing on the ground
            let block = editor.get_block(Point3D::new(point.x, height, point.y));
            
            if let Some(permit_blocks) = permit_blocks {
                if permit_blocks.contains(&block.id) {
                    continue;
                }
            }
            if let Some(offset) = height_offset {
                height += offset;
            }
            let block_pos = rng.choose_weighted(block_dict);
            editor.place_block(&block_list[*block_pos as usize], Point3D::new(point.x, height, point.y)).await;

        }
    }

pub async fn replace_ground_smooth(
    points: &HashSet<Point2D>,
    block_dict: &HashMap<u32, HashMap<u32, f32>>,
    block_list: &Vec<Block>,
    rng: &mut RNG,
    editor: &mut Editor,
    height_offset: Option<i32>,
    permit_blocks: Option<&HashSet<BlockID>>, // should this be a set of blocks to permit or a set of blocks to ignore? currently treated as ignore
    ignore_water: Option<bool>) { //thereotically could be part of permit blocks
        for point in points {
            if editor.world_mut().is_claimed(*point) { // already built on point
                continue;
            }
            if let Some(ignore_water) = ignore_water {
                if editor.world_mut().is_water(*point) && ignore_water {
                    continue; // skip water points if ignore_water is true
                }
            }

            let mut height = editor.world_mut().get_height_at(*point); 
            let block = editor.get_block(Point3D::new(point.x, height, point.y));
            
            if let Some(permit_blocks) = permit_blocks {
                if permit_blocks.contains(&block.id) {
                    continue;
                }
            }
            if let Some(offset) = height_offset {
                height += offset;
            }

            let mut y_in_dir: HashMap<Point2D, i32> = HashMap::new();
            let mut block = Block::new(BlockID::Unknown, None, None);

            for direction in CARDINALS_2D {
                let neighbor = *point + direction;
                let opposite_neighbour = *point - direction;
                if !points.contains(&neighbor) {
                    continue; // skip if neighbor is not in points
                }
                if points.contains(&neighbor) {
                    y_in_dir.insert(direction, editor.world_mut().get_height_at(neighbor));
                }
                if !points.contains(&opposite_neighbour) {
                    continue; // skip if opposite neighbor is not in points
                }
                if editor.world_mut().get_height_at(neighbor) == height + 1 && editor.world_mut().get_height_at(opposite_neighbour) == height - 1 {
                    //place stair
                    block = block_list[*rng.choose_weighted(block_dict.get(&1).unwrap()) as usize].clone();
                    block.state = Some(HashMap::from([("facing".to_string(), cardinal_to_str(&direction).unwrap())]));
                    info!("Placing {:?} stair at {:?} facing {:?}\n",block, point, direction);
                    break;
                }
            }
            if y_in_dir.values().all(|&y| y <= height) && y_in_dir.values().any(|&y| y < height) {
                // all neighbors are less than or equal to the current height, and at least one is less
                block = block_list[*rng.choose_weighted(block_dict.get(&2).unwrap()) as usize].clone();
            }
            if block.id == BlockID::Unknown {
                // normal block
                block = block_list[*rng.choose_weighted(block_dict.get(&0).unwrap()) as usize].clone();
            }

            editor.place_block(&block, Point3D::new(point.x, height-1, point.y)).await;// height-1 to ensure we are placing on the ground

        }
    }

pub async fn plant_forest(
    points: &HashSet<Point2D>,
    forest: Forest,
    rng: &mut RNG,
    editor: &mut Editor,
    permit_blocks: Option<&HashSet<BlockID>>,
    ignore_water: bool,
) {
    let mut shuffled_points: Vec<Point2D> = points.iter().cloned().collect();
    for point in points {
        let point = rng.pop(&mut shuffled_points).unwrap_or(*point); // get randomness in

        if editor.world_mut().is_claimed(point) { // already built on point
            continue;
        }
        if editor.world_mut().is_water(point) && ignore_water {
            continue; // skip water points if ignore_water is true
        }

        let mut height = editor.world_mut().get_height_at(point);
        let block = editor.get_block(Point3D::new(point.x, height, point.y));

        if let Some(permit_blocks) = permit_blocks {
            if permit_blocks.contains(&block.id) {
                continue;
            }
        }

        let tree_type = *rng.choose_weighted(forest.trees());
        let palette = forest.tree_palette().get(&tree_type).expect("Tree type not found in forest palette");

        generate_tree(tree_type, editor, point.add_y(height), rng, palette).await;

        for x in (point.x - forest.tree_density() as i32 + 1)..(point.x + forest.tree_density() as i32) {
            for y in (point.y - forest.tree_density() as i32 + 1)..(point.y + forest.tree_density() as i32) {
                editor.world_mut().claim(Point2D::new(x, y), BuildClaim::Nature);
            }
        }
    }
}