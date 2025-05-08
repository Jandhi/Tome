use serde_derive::{Deserialize, Serialize};

use super::Coordinate;


#[derive(Clone, Deserialize, Debug)]
pub struct EntityResponse {
    pub uuid: String,
    pub data: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PositionedEntity {
    pub x: Coordinate,
    pub y: Coordinate,
    pub z: Coordinate,
    pub id: String,
    pub data: Option<String>,
}