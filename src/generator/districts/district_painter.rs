use std::collections::{HashMap, HashSet};

use log::{error, warn};

use crate::{editor::{World, Editor}, geometry::{Point2D, Point3D, Rect2D, CARDINALS, CARDINALS_2D, X_PLUS_2D, Y_PLUS_2D}, minecraft::{Biome, Block, BlockID}, noise::{Seed, RNG}};

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
        println!("Road points: {:?}", points);
        for point in points {
            let mut height = world.get_height_at(*point);
            let block = editor.get_block(Point3D::new(point.x, height, point.y)).await;
            if world.is_built(*point) { // already built on point
                continue;
            }
            if let Some(permit_blocks) = permit_blocks {
                if permit_blocks.contains(&block.id) {
                    continue;
                }
            }
            if let Some(ignore_water) = ignore_water {
                if ignore_water && block.id == BlockID::Water { // can use is_water(), unsure if it is better
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