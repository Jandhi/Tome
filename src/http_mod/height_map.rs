use std::fmt::Display;

use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HeightMapType {
    #[serde(rename = "WORLD_SURFACE")]
    WorldSurface,
    #[serde(rename = "OCEAN_FLOOR_NO_PLANTS")]
    OceanFloorNoPlants,
    #[serde(rename = "MOTION_BLOCKING_NO_LEAVES")]
    MotionBlockingNoPlants,
    #[serde(rename = "MOTION_BLOCKING")]
    MotionBlocking
}

impl Display for HeightMapType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HeightMapType::WorldSurface => write!(f, "WORLD_SURFACE"),
            HeightMapType::OceanFloorNoPlants => write!(f, "OCEAN_FLOOR_NO_PLANTS"),
            HeightMapType::MotionBlockingNoPlants => write!(f, "MOTION_BLOCKING_NO_LEAVES"),
            HeightMapType::MotionBlocking => write!(f, "MOTION_BLOCKING"),
        }
    }
}