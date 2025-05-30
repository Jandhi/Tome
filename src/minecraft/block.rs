use std::collections::HashMap;

use log::info;
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

impl From<BlockID> for Block {
    fn from(id: BlockID) -> Self {
        Block {
            id,
            states: None,
            data: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlockID {
    Unknown,

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

    #[serde(rename = "minecraft:bedrock")]
    Bedrock,
}

impl BlockID {
    pub fn is_water(self) -> bool {
        matches!(self, BlockID::Water)
    }
}

impl From<&str> for BlockID {
    fn from(value: &str) -> Self {
        
        
        info!("Converting string to BlockID: {}", value);


        serde_json::from_str::<BlockID>(&format!("\"{}\"", value)).unwrap_or(BlockID::Unknown)
    }
}