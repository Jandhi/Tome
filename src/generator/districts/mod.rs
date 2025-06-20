mod district;
mod super_district;
mod test;
mod adjacency;
mod analysis;
mod merge;
mod constants;
mod data;
mod classification;
mod wall;
mod district_painter;
mod gate;

pub use district::District;
pub use district::DistrictID;
pub use analysis::DistrictAnalysis;
pub use district::DistrictType;
pub use district::generate_districts;
pub use super_district::SuperDistrict;
pub use super_district::SuperDistrictID;
pub use data::{DistrictData, HasDistrictData};
pub use district_painter::*;
pub use wall::{build_wall, WallType};
pub use gate::build_wall_gate;