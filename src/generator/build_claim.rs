use crate::generator::{buildings::BuildingID, nbts::StructureID, paths::PathType};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BuildClaim {
    Nature,
    Wall,
    Gate,
    Path(PathType),
    /// Cell reserved for a road that hasn't been physically paved yet. Treated
    /// like a road for purposes that only care about the intent (e.g. frontage
    /// detection) but doesn't block foundation terrain blending — so houses
    /// placed before paving will raise the heightmap on these cells, and the
    /// later pave step picks up foundation-influenced heights.
    PathPlanned(PathType),
    Building(BuildingID),
    Structure(StructureID),
    ProductionArea(StructureID),
    /// Footprint of a placed ship (scattered on a water district). Reserves the
    /// water cells so later passes (harbour, roads, bridges) and other ships skip them.
    Ship,
    None,
}
