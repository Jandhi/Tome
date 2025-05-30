use std::collections::HashMap;

use fastnbt::{ByteArray, LongArray, Value};
use serde_derive::{Serialize, Deserialize};

use crate::minecraft::Biome;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Chunks {
    #[serde(rename = "Chunks")]
    pub chunks: Vec<Chunk>,
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Chunk {
    #[serde(rename = "xPos")]
    pub x_pos : i32,
    #[serde(rename = "yPos")]
    pub y_pos : i32,
    #[serde(rename = "zPos")]
    pub z_pos : i32,
    #[serde(rename = "sections")]
    pub sections: Vec<ChunkSection>,
    #[serde(rename = "Heightmaps")]
    pub heightmaps : HeightMaps,

    #[serde(rename = "block_entities")]
    pub block_entities: Option<Vec<Value>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChunkSection {
    #[serde(rename = "Y")]
    pub y: i32,
    #[serde(rename = "block_states")]
    pub block_states: Option<BlockStates>, 
    #[serde(rename = "biomes")]
    pub biomes: Option<Biomes>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BlockStates {
    #[serde(rename = "palette")]
    pub palette: Vec<Block>,

    // If this is none, all blocks in the section are the same
    #[serde(rename = "data")]
    pub data: Option<LongArray>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Block {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Properties")]
    pub properties : Option<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Biomes {
    #[serde(rename = "palette")]
    pub biomes: Vec<Biome>,

    // If this is none, all blocks in the section have the same biome
    #[serde(rename = "data")]
    pub data: Option<LongArray>, 
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HeightMaps {
    #[serde(rename = "MOTION_BLOCKING")]
    pub motion_blocking: LongArray,
    #[serde(rename = "MOTION_BLOCKING_NO_LEAVES")]
    pub motion_blocking_no_leaves: LongArray,
    #[serde(rename = "OCEAN_FLOOR")]
    pub ocean_floor: LongArray,
    #[serde(rename = "OCEAN_FLOOR_WG")]
    pub ocean_floor_wg: Option<LongArray>,
    #[serde(rename = "WORLD_SURFACE")]
    pub world_surface: LongArray,
    #[serde(rename = "WORLD_SURFACE_WG")]
    pub world_surface_wg: Option<LongArray>,
}
