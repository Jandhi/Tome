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
    Unknown, // Placeholder for unknown block ids
    
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
    #[serde(rename = "minecraft:white_wool")]
    WhiteWool,
    #[serde(rename = "minecraft:orange_wool")]
    OrangeWool,
    #[serde(rename = "minecraft:magenta_wool")]
    MagentaWool,
    #[serde(rename = "minecraft:light_blue_wool")]
    LightBlueWool,
    #[serde(rename = "minecraft:yellow_wool")]
    YellowWool,
    #[serde(rename = "minecraft:lime_wool")]
    LimeWool,
    #[serde(rename = "minecraft:pink_wool")]
    PinkWool,
    #[serde(rename = "minecraft:gray_wool")]
    GrayWool,
    #[serde(rename = "minecraft:light_gray_wool")]
    LightGrayWool,
    #[serde(rename = "minecraft:cyan_wool")]
    CyanWool,
    #[serde(rename = "minecraft:purple_wool")]
    PurpleWool,
    #[serde(rename = "minecraft:blue_wool")]
    BlueWool,
    #[serde(rename = "minecraft:brown_wool")]
    BrownWool,
    #[serde(rename = "minecraft:green_wool")]
    GreenWool,
    #[serde(rename = "minecraft:red_wool")]
    RedWool,
    #[serde(rename = "minecraft:black_wool")]
    BlackWool,

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
        serde_json::from_str::<BlockID>(&format!("\"{}\"", value)).unwrap_or(BlockID::Unknown)
    }
}