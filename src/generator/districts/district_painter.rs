use std::collections::{HashMap, HashSet};


use crate::{editor::{World, Editor}, geometry::{Point2D, Point3D, CARDINALS_2D, cardinal_to_str}, minecraft::{Block, BlockID}, noise::RNG};

pub async fn replace_ground(
    points: &HashSet<Point2D>,
    block_dict: &HashMap<u32, f32>,
    block_list: &Vec<Block>,
    rng: &mut RNG,
    world: &World,
    editor: &mut Editor,
    height_offset: Option<i32>,
    permit_blocks: Option<&HashSet<BlockID>>, // should this be a set of blocks to permit or a set of blocks to ignore? currently treated as ignore
    ignore_water: Option<bool>) { //thereotically could be part of permit blocks
        for point in points {
            if world.is_claimed(*point) { // already built on point
                continue;
            }
            if let Some(ignore_water) = ignore_water {
                if !ignore_water && world.is_claimed(*point) { // can use is_water(), unsure if it is better
                    continue;
                }
            }

            let mut height = world.get_height_at(*point) - 1; // -1 to ensure we are placing on the ground
            let block = editor.get_block(Point3D::new(point.x, height, point.y), world);
            
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
    world: &World,
    editor: &mut Editor,
    height_offset: Option<i32>,
    permit_blocks: Option<&HashSet<BlockID>>, // should this be a set of blocks to permit or a set of blocks to ignore? currently treated as ignore
    ignore_water: Option<bool>) { //thereotically could be part of permit blocks
        for point in points {
            print!("Replacing ground at {:?}\n", point);
            if world.is_claimed(*point) { // already built on point
                continue;
            }
            if let Some(ignore_water) = ignore_water {
                if !ignore_water && world.is_claimed(*point) { // can use is_water(), unsure if it is better
                    continue;
                }
            }

            let mut height = world.get_height_at(*point); 
            let block = editor.get_block(Point3D::new(point.x, height, point.y), world);
            
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
                    y_in_dir.insert(direction, world.get_height_at(neighbor));
                }
                if !points.contains(&opposite_neighbour) {
                    continue; // skip if opposite neighbor is not in points
                }
                if world.get_height_at(neighbor) == height + 1 && world.get_height_at(opposite_neighbour) == height - 1 {
                    //place stair
                    block = block_list[*rng.choose_weighted(block_dict.get(&1).unwrap()) as usize].clone();
                    block.state = Some(HashMap::from([("facing".to_string(), cardinal_to_str(&direction).unwrap())]));
                    print!("Placing {:?} stair at {:?} facing {:?}\n",block, point, direction);
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