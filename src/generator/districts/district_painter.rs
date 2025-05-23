use std::collections::{HashMap, HashSet};

use log::{error, warn};

use crate::{editor::World, geometry::{Point2D, Point3D, Rect2D, CARDINALS, CARDINALS_2D, X_PLUS_2D, Y_PLUS_2D}, minecraft::{Biome, BlockID}, noise::{Seed, RNG}};

pub fn replace_ground(
    points: &HashSet<Point2D>,
    block_dict: &HashMap<u32, BlockID>,
    rng: &mut RNG,
    world: &World,
    height_offset: Option<i32>,
    permit_blocks: Option<&HashSet<BlockID>>,
    ignore_water: Option<bool>) {
        for point in points {
            let mut height = world.get_height(point.x, point.y);
            if let Some(offset) = height_offset {
                height += offset;
            }
            if height < 0 {
                continue;
            }
            let block = world.get_block(point.x, height, point.y);
            if let Some(permit_blocks) = permit_blocks {
                if permit_blocks.contains(&block) {
                    continue;
                }
            }
            if let Some(ignore_water) = ignore_water {
                if ignore_water && block == BlockID::Water {
                    continue;
                }
            }
            let biome = world.get_biome(point.x, point.y);
            if let Some(block_id) = block_dict.get(&biome) {
                world.set_block(point.x, height, point.y, *block_id);
            } else {
                error!("Biome {:?} not found in block dictionary", biome);
            }
        }
    }