use crate::generator::{buildings::BuildingID, nbts::StructureID, paths::PathType};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BuildClaim {
    Nature,
    Wall,
    Gate,
    Path(PathType),
    Building(BuildingID),
    Structure(StructureID),
    None,
}
