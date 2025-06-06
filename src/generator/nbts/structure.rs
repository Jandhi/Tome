use std::collections::HashMap;

use serde_derive::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Structure {
    size : [i32; 3],
    palette : Vec<PaletteBlock>,
    blocks : Vec<BlockData>,
    entities : Vec<Entity>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct PaletteBlock {
    name : String,
    properties : Option<HashMap<String, String>>,
}


#[derive(Serialize, Deserialize, Clone, Debug)]
struct BlockData {
    state: usize,
    pos : [i32; 3],
    nbt : Option<fastnbt::Value>
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Entity {
    pos: [f64; 3],
    #[serde(rename = "blockPos")]
    block_pos: [i32; 3],
    nbt: fastnbt::Value,
}