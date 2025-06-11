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

impl From<&BlockID> for Block {
    fn from(id: &BlockID) -> Self {
        Block {
            id: id.clone(),
            state: None,
            data: None,
        }
    }
}

// Converts a string representation of a block into a Block struct.
pub fn string_to_block(block: &str) -> Option<Block> {
    if block.contains('[') {
        let mut iter = block.split('[');
        let id = iter.next()?.into();
        let state_list = iter.next()?.trim_end_matches(']');
        let state: Option<HashMap<String, String>> = state_list.split(',').map(|s| {
            let mut kv = s.split('=');
            let key = kv.next()?.trim().to_string();
            let value = kv.next()?.trim().to_string();
            Some((key, value))
        }).collect();
        println!("Parsed block: {:?} with state: {:?} iter {:?} statelist {:?}", id, state, iter, state_list);
        Some(Block {
                    id: id,
                    state: state,
                    data: None,
                })
    } else {
        Some(Block {
            id: block.into(),
            state: None,
            data: None,
        })
    }
}