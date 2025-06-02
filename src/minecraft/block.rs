use std::collections::HashMap;

use serde_derive::{Deserialize, Serialize};

use crate::minecraft::BlockID;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub id : BlockID,
    pub state : Option<HashMap<String, String>>,
    pub data: Option<String>,
}

impl Block {
    pub fn new(id: BlockID, state: Option<HashMap<String, String>>, data: Option<String>) -> Self {
        Block { id, state, data }
    }
}

impl From<BlockID> for Block {
    fn from(id: BlockID) -> Self {
        Block {
            id,
            state: None,
            data: None,
        }
    }
}