use std::collections::HashMap;

use serde_derive::{Serialize, Deserialize};

use crate::minecraft::BlockID;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Structure {
    pub size : [i32; 3],
    pub palette : Vec<PaletteBlock>,
    pub blocks : Vec<BlockData>,
    pub entities : Vec<Entity>,
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