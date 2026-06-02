mod grid;
mod test;
mod shape;
mod placement;
mod id;
mod data;
mod floor;
mod stairs;
mod set;
mod foundation;
pub mod roofs;
pub mod walls;
pub mod constants;

pub use grid::Grid;
pub use id::BuildingID;
pub use data::BuildingData;
pub use floor::build_floor;
pub use stairs::build_stairs;
pub use set::{
    BuildingSet,
    BuildingSetID,
};
pub use placement::{
    PavingType,
    place_buildings,
    get_city_blocks_and_off_limits,
    smooth_and_pave_road,
};