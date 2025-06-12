use std::collections::HashMap;

use serde_derive::{Serialize, Deserialize};

use crate::{geometry::Point3D, minecraft::{Block, BlockID}};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Structure {
    pub size : [i32; 3],
    pub palette : Vec<PaletteBlock>,
    pub blocks : Vec<BlockData>,
    pub entities : Vec<Entity>,
}

impl Structure {
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
            nbt: block.data.map(|s| fastnbt::Value::from(s)),
        });
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
    pub nbt : Option<fastnbt::Value>
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Entity {
    pub pos: [f64; 3],
    #[serde(rename = "blockPos")]
    pub block_pos: [i32; 3],
    pub nbt: fastnbt::Value,
}