use crate::generator::{buildings::BuildingID, paths::PathType};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub enum BuildClaim {
    Nature,
    Wall,
    Gate,
    Path(PathType),
    Building(BuildingID),
    None,
}