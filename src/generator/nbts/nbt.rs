use std::collections::HashMap;

use serde_derive::{Serialize, Deserialize};

use crate::{geometry::Point3D, http_mod::PositionedBlock, minecraft::{Block, BlockID}};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NBTStructure {
    pub size : [i32; 3],
    pub palette : Vec<PaletteBlock>,
    pub blocks : Vec<BlockData>,
    pub entities : Vec<Entity>,
}

impl NBTStructure {
    pub fn add_block(&mut self, block : Block, pos : Point3D) {
        let state = self.palette.iter().position(|b| b.name == block.id && b.properties == block.state).unwrap_or_else(|| {
            self.palette.push(PaletteBlock {
                name: block.id,
                properties: block.state,
            });
            self.palette.len() - 1
        });

        self.blocks.push(BlockData {
            state,
            pos: [pos.x as i32, pos.y as i32, pos.z as i32],
            nbt: block.data,
        });
    }

    pub fn from_blocks(blocks : Vec<(Block, Point3D)>) -> Self {
        let mut palette = Vec::new();
        let mut block_data = Vec::new();

        let min : Point3D = blocks.iter().fold(
            Point3D { x: 0, y: 0, z: 0 },
            |acc, (_, pos)| Point3D {
                x: acc.x.min(pos.x),
                y: acc.y.min(pos.y),
                z: acc.z.min(pos.z),
            },
        );
        let max : Point3D = blocks.iter().fold(
            Point3D { x: 0, y: 0, z: 0 },
            |acc, (_, pos)| Point3D {
                x: acc.x.max(pos.x),
                y: acc.y.max(pos.y),
                z: acc.z.max(pos.z),
            },
        );

        for (block, pos) in blocks {
            let state = palette.iter().position(|b : &PaletteBlock| b.name == block.id && b.properties == block.state).unwrap_or_else(|| {
                palette.push(PaletteBlock {
                    name: block.id,
                    properties: block.state,
                });
                palette.len() - 1
            });

            block_data.push(BlockData {
                state,
                pos: [pos.x as i32, pos.y as i32, pos.z as i32],
                nbt: block.data,
            });
        }

        NBTStructure {
            size: [max.x - min.x + 1, max.y - min.y + 1, max.z - min.z + 1],
            palette,
            blocks: block_data,
            entities: Vec::new(), // Entities can be added later if needed
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PaletteBlock {
    #[serde(rename = "Name")]
    pub name : BlockID,
    #[serde(rename = "Properties")]
    pub properties : Option<HashMap<String, String>>,
}


#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BlockData {
    pub state: usize,
    pub pos : [i32; 3],
    pub nbt : Option<String>
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Entity {
    pub pos: [f64; 3],
    #[serde(rename = "blockPos")]
    pub block_pos: [i32; 3],
    pub nbt: fastnbt::Value,
}