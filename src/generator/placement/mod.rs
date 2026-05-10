mod placement;
#[cfg(test)]
mod test;

pub use placement::{
    place_rural_building,
    place_urban_building,
    place_urban_buildings,
    footprint_dims_for_rotation,
    anchor_offset_for_rotation,
    score_candidate,
    Candidate,
    CandidateScore,
    BLEND_RADIUS,
    EDGE_WEIGHT,
    FLATNESS_WEIGHT,
    MAX_BLEND_DELTA,
    NUM_CANDIDATES,
    ROAD_SEARCH_RADIUS,
    ROAD_WEIGHT,
    WATER_MARGIN_RADIUS,
    WATER_WEIGHT,
    YARD_RADIUS,
};
