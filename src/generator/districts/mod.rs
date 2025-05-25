mod district;
mod super_district;
mod test;
mod adjacency;

pub use district::District;
pub use district::DistrictID;
pub use super_district::SuperDistrict;
pub use super_district::SuperDistrictID;
pub use district::generate_districts;