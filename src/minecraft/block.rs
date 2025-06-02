use std::collections::HashMap;

use serde_derive::{Deserialize, Serialize};

use crate::minecraft::BlockID;

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

impl From<&BlockID> for Block {
    fn from(id: &BlockID) -> Self {
        Block {
            id: id.clone(),
            states: None,
            data: None,
        }
    }
}