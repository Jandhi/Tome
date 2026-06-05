mod placement;
#[cfg(test)]
mod test;

pub use placement::{
    place_rural_building,
    place_urban_building,
    place_urban_buildings,
};

#[cfg(test)]
pub use placement::{
    footprint_dims_for_rotation,
    anchor_offset_for_rotation,
};