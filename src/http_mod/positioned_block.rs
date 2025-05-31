use std::collections::HashMap;

use serde_derive::{Deserialize, Serialize};

use crate::minecraft::{Block, BlockID};

use super::{coordinate::Coordinate3D, Coordinate};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionedBlock {
    pub id : BlockID,
    pub x : Coordinate,
    pub y : Coordinate,
    pub z : Coordinate,
    pub state : Option<HashMap<String, String>>,
    pub data: Option<String>,
}


impl PositionedBlock {
    pub fn from_block(block : Block, position : Coordinate3D) -> Self {
        PositionedBlock {
            id: block.id,
            x: position.x,
            y: position.y,
            z: position.z,
            state: block.states,
            data: block.data,
        }
    }

    pub fn get_coordinate(&self) -> Coordinate3D {
        Coordinate3D::new(self.x, self.y, self.z)
    }

    pub fn get_block(&self) -> Block {
        Block {
            id: self.id,
            states: self.state.clone(),
            data: self.data.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockPlacementResponse {
    pub status : i32
}