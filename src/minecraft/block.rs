use std::collections::HashMap;

use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub id : BlockID,
    pub states : Option<HashMap<String, String>>,
    pub data: Option<String>,
}

impl Block {
    pub fn new(id: BlockID, states: Option<HashMap<String, String>>, data: Option<String>) -> Self {
        Block { id, states, data }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlockID {
    #[serde(rename = "minecraft:air")]
    Air,
    #[serde(rename = "minecraft:stone")]
    Stone,

    #[serde(rename = "minecraft:grass_block")]
    GrassBlock,
    #[serde(rename = "minecraft:dirt")]
    Dirt,

    #[serde(rename = "minecraft:water")]
    Water,

    // Wool
    #[serde(rename = "minecraft:red_wool")]
    RedWool,
    #[serde(rename = "minecraft:green_wool")]
    GreenWool,
    #[serde(rename = "minecraft:blue_wool")]
    BlueWool,
    #[serde(rename = "minecraft:yellow_wool")]
    YellowWool,
    #[serde(rename = "minecraft:magenta_wool")]
    MagentaWool,
    #[serde(rename = "minecraft:light_blue_wool")]
    LightBlueWool,
    #[serde(rename = "minecraft:orange_wool")]
    OrangeWool,
}

impl BlockID {
    pub fn is_water(self) -> bool {
        matches!(self, BlockID::Water)
    }
}