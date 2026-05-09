use crate::generator::{buildings::BuildingID, nbts::StructureId, paths::PathType};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BuildClaim {
    Nature,
    Wall,
    Gate,
    Path(PathType),
    Building(BuildingID),
    Structure(StructureId),
    None,
}
