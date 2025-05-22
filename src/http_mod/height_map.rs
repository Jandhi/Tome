use std::fmt::Display;

pub enum HeightMapType {
    WorldSurface,
    OceanFloorNoPlants,
    MotionBlockingNoPlants,
}

impl Display for HeightMapType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HeightMapType::WorldSurface => write!(f, "WORLD_SURFACE"),
            HeightMapType::OceanFloorNoPlants => write!(f, "OCEAN_FLOOR_NO_PLANTS"),
            HeightMapType::MotionBlockingNoPlants => write!(f, "MOTION_BLOCKING_NO_PLANTS"),
        }
    }
}