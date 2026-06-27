mod placement;
#[cfg(test)]
mod test;

pub use placement::{
    district_seatable_footprints,
    place_rural_building,
    place_urban_building,
    place_urban_buildings,
    resolve_rural_production,
    try_place_rural,
    PlacedRural,
};

#[cfg(test)]
pub use placement::{
    footprint_dims_for_rotation,
    anchor_offset_for_rotation,
    anchor_world_posts,
};