mod district;
mod super_district;
mod test;
mod adjacency;
mod analysis;
mod merge;
mod constants;
mod data;

mod district_painter;

pub use district::District;
pub use district::DistrictID;
pub use analysis::DistrictAnalysis;
pub use district::generate_districts;
pub use super_district::SuperDistrict;
pub use super_district::SuperDistrictID;
pub use data::{DistrictData, HasDistrictData};
pub use district::generate_districts;
pub use district_painter::replace_ground;
