use serde_derive::Deserialize;

use crate::minecraft::Biome;

#[derive(Debug, Clone, Deserialize)]
pub struct PositionedBiome {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub id: Biome,
}