use crate::generator::buildings::BuildingID;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub enum BuildClaim {
    Nature,
    Wall,
    Gate,
    Building(BuildingID),
    None,
}