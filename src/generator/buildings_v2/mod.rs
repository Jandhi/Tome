mod footprint;
mod frame;
mod wall;
mod placement;
mod generate;
pub mod roof;
mod test;

pub use footprint::Footprint;
pub use frame::Frame;
pub use wall::{WallSegment, Opening, OpeningKind, DoorType, WindowType, WallError};
pub use placement::{
    place_corner_posts, place_wall_segment, place_wall_segment_with_materials, place_walls,
    place_floor, place_floors, place_frame, place_frame_with_config,
    place_door_opening, place_door_openings, place_doors,
    place_window_opening, place_window_openings, place_windows,
    place_wall_block, place_pillar_block, place_gable_wall_block,
    WallMaterials, PlacementConfig,
};
pub use generate::{
    DoorRules, DoorPlacements, generate_doors, apply_door_placements, add_doors_to_frame,
    WindowRules, WindowPlacements, generate_windows, apply_window_placements, add_windows_to_frame,
};
pub use roof::{
    RoofType, RoofPitch, RoofConfig, Roof, RoofRules, GableConfig, HipConfig, GableDecoration,
    place_roof, place_hip_roof, place_gable_roof, place_gable_walls, place_gable_decorations, generate_roof,
};
