use std::collections::HashMap;

use crate::{
    editor::Editor,
    generator::materials::{Material, MaterialId, MaterialRole, Palette},
    geometry::{Cardinal, Point2D, Point3D},
    minecraft::{Block, BlockForm, BlockID},
    noise::RNG,
};

use super::{Frame, Opening, OpeningKind, WallSegment};

/// Resolved wall materials from a palette.
/// Use this to ensure consistent wall blocks across regular walls and gable walls.
pub struct WallMaterials {
    pub wall_block: Block,
    pub pillar_block: Block,
}

impl WallMaterials {
    /// Create wall materials from a palette.
    pub fn from_palette(
        palette: &Palette,
        materials: &HashMap<MaterialId, Material>,
        rng: &mut RNG,
    ) -> Self {
        let wall_block_id = palette
            .get_block(MaterialRole::PrimaryWall, &BlockForm::Block, materials, rng)
            .cloned()
            .unwrap_or_else(|| BlockID::from("stone_bricks"));

        let pillar_block_id = palette
            .get_block(MaterialRole::WoodPillar, &BlockForm::Block, materials, rng)
            .cloned()
            .unwrap_or_else(|| BlockID::from("oak_log"));

        Self {
            wall_block: Block::from(wall_block_id),
            pillar_block: Block::from(pillar_block_id),
        }
    }
}

/// Place a single wall block at the given position.
/// This is the core wall placement operation - customize this for different wall styles.
pub async fn place_wall_block(
    editor: &Editor,
    pos: Point3D,
    wall_mats: &WallMaterials,
) {
    editor.place_block(&wall_mats.wall_block, pos).await;
}

/// Place a single pillar block at the given position.
/// Used for corner posts and gable wall edges.
pub async fn place_pillar_block(
    editor: &Editor,
    pos: Point3D,
    wall_mats: &WallMaterials,
) {
    editor.place_block(&wall_mats.pillar_block, pos).await;
}

/// Places corner posts (vertical columns) at each footprint vertex.
/// Posts extend from base_y to the top of the building.
pub async fn place_corner_posts(
    frame: &Frame,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    let pillar_block = palette
        .get_block(MaterialRole::WoodPillar, &BlockForm::Block, materials, rng)
        .cloned()
        .unwrap_or_else(|| BlockID::from("oak_log"));

    let block = Block::from(pillar_block);

    for vertex in &frame.footprint.vertices {
        for y in frame.base_y..(frame.base_y + frame.total_height()) {
            let pos = vertex.add_y(y);
            editor.place_block(&block, pos).await;
        }
    }
}

/// Places wall blocks for a single wall segment on a specific floor.
/// Skips positions where openings exist.
pub async fn place_wall_segment(
    segment: &WallSegment,
    floor_y: i32,
    wall_height: i32,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    let wall_mats = WallMaterials::from_palette(palette, materials, rng);
    place_wall_segment_with_materials(segment, floor_y, wall_height, editor, &wall_mats).await;
}

/// Places wall blocks for a single wall segment using pre-resolved materials.
/// Use this when you need to place multiple segments with the same materials.
pub async fn place_wall_segment_with_materials(
    segment: &WallSegment,
    floor_y: i32,
    wall_height: i32,
    editor: &Editor,
    wall_mats: &WallMaterials,
) {
    let positions = segment.positions();

    // Skip first and last positions (corners are handled by corner posts)
    for (i, pos_2d) in positions.iter().enumerate() {
        if i == 0 || i == positions.len() - 1 {
            continue;
        }

        for y_offset in 0..wall_height {
            // Check if this position is an opening
            if segment.is_opening_at(i as i32, y_offset) {
                continue;
            }

            let pos = pos_2d.add_y(floor_y + y_offset);
            place_wall_block(editor, pos, wall_mats).await;
        }
    }
}

/// Places all walls for a frame across all floors.
pub async fn place_walls(
    frame: &Frame,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    for floor in 0..frame.floors {
        let floor_y = frame.floor_y(floor);

        for segment in frame.wall_segments() {
            place_wall_segment(
                segment,
                floor_y,
                frame.wall_height,
                editor,
                palette,
                materials,
                rng,
            )
            .await;
        }
    }
}

/// Place a gable wall block at the given position.
/// Uses pillar blocks at edges (is_edge=true) and wall blocks elsewhere.
/// Skips the crossbar level (skip_y) to avoid overriding timber frame crossbars.
pub async fn place_gable_wall_block(
    editor: &Editor,
    pos: Point3D,
    is_edge: bool,
    skip_y: Option<i32>,
    wall_mats: &WallMaterials,
) {
    // Skip if this is the crossbar level
    if let Some(crossbar_y) = skip_y {
        if !is_edge && pos.y == crossbar_y {
            return;
        }
    }

    if is_edge {
        place_pillar_block(editor, pos, wall_mats).await;
    } else {
        place_wall_block(editor, pos, wall_mats).await;
    }
}

/// Places floor surface blocks within the footprint bounds.
/// Only places blocks that are inside the polygon.
pub async fn place_floor(
    frame: &Frame,
    floor: u32,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    let floor_block = palette
        .get_block(MaterialRole::PrimaryStone, &BlockForm::Block, materials, rng)
        .cloned()
        .unwrap_or_else(|| BlockID::from("stone"));

    let block = Block::from(floor_block);
    let y = frame.floor_y(floor) - 1;

    if let Some((min, max)) = frame.footprint.bounds() {
        for x in min.x..=max.x {
            for z in min.y..=max.y {
                let point = Point2D::new(x, z);
                if frame.footprint.contains(point) {
                    editor.place_block(&block, point.add_y(y)).await;
                }
            }
        }
    }
}

/// Places floor surfaces for all floors in a building.
pub async fn place_floors(
    frame: &Frame,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    for floor in 0..frame.floors {
        place_floor(frame, floor, editor, palette, materials, rng).await;
    }
}

/// Places horizontal pillar crossbars at the top of each floor's walls for a timber frame look.
/// Crossbars run along each wall segment at the ceiling level.
pub async fn place_wall_crossbars(
    frame: &Frame,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    let pillar_block = palette
        .get_block(MaterialRole::WoodPillar, &BlockForm::Block, materials, rng)
        .cloned()
        .unwrap_or_else(|| BlockID::from("oak_log"));

    for floor in 0..frame.floors {
        let ceiling_y = frame.ceiling_y(floor) - 1;

        for segment in frame.wall_segments() {
            let positions = segment.positions();

            // Determine axis along the wall direction
            let axis = if segment.is_x_aligned() { "x" } else { "z" };
            let mut state = HashMap::new();
            state.insert("axis".to_string(), axis.to_string());
            let block = Block::new(pillar_block.clone(), Some(state), None);

            // Skip first and last positions (corners are handled by corner posts)
            for (i, pos_2d) in positions.iter().enumerate() {
                if i == 0 || i == positions.len() - 1 {
                    continue;
                }

                editor.place_block(&block, pos_2d.add_y(ceiling_y)).await;
            }
        }
    }
}

/// Places a complete building frame (corners, walls, floors, doors, windows, wall crossbars).
pub async fn place_frame(
    frame: &Frame,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    place_corner_posts(frame, editor, palette, materials, rng).await;
    place_wall_crossbars(frame, editor, palette, materials, rng).await;
    place_walls(frame, editor, palette, materials, rng).await;
    place_floors(frame, editor, palette, materials, rng).await;
    place_doors(frame, editor, palette, materials, rng).await;
    place_windows(frame, editor, palette, materials, rng).await;
}

/// Get the facing direction for a door based on the wall segment direction.
/// Doors face outward (perpendicular to the wall, away from building interior).
fn wall_to_door_facing(segment: &WallSegment) -> Cardinal {
    let dir = segment.direction();
    // Wall direction is along the wall; door faces perpendicular (outward)
    // For a wall going East (+x), door faces South (+z)
    // For a wall going South (+z), door faces West (-x)
    if dir.x > 0 {
        Cardinal::South
    } else if dir.x < 0 {
        Cardinal::North
    } else if dir.y > 0 {
        Cardinal::West
    } else {
        Cardinal::East
    }
}

/// Places a door block with proper facing and hinge states.
async fn place_door_block(
    editor: &Editor,
    pos: crate::geometry::Point3D,
    door_block_id: &BlockID,
    facing: Cardinal,
    is_upper: bool,
    hinge_right: bool,
) {
    let mut state = HashMap::new();
    state.insert("facing".to_string(), facing.to_string());
    state.insert("half".to_string(), if is_upper { "upper" } else { "lower" }.to_string());
    state.insert("hinge".to_string(), if hinge_right { "right" } else { "left" }.to_string());
    state.insert("open".to_string(), "false".to_string());

    let block = Block::new(door_block_id.clone(), Some(state), None);
    editor.place_block(&block, pos).await;
}

/// Places a single door opening (door blocks + lintel) in a wall segment.
pub async fn place_door_opening(
    segment: &WallSegment,
    opening: &Opening,
    floor_y: i32,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    let door_type = match opening.kind {
        OpeningKind::Door(dt) => dt,
        _ => return, // Not a door
    };

    let facing = wall_to_door_facing(segment);
    let positions = segment.positions();

    // Get floor/threshold block
    let floor_block = palette
        .get_block(MaterialRole::PrimaryStone, &BlockForm::Block, materials, rng)
        .cloned()
        .unwrap_or_else(|| BlockID::from("stone"));

    // Place blocks under the door opening
    for i in 0..opening.width {
        let pos_idx = (opening.position + i) as usize;
        if pos_idx >= positions.len() {
            continue;
        }
        let pos_2d = positions[pos_idx];
        let threshold_pos = pos_2d.add_y(floor_y - 1);
        editor.place_block_no_update(&Block::from(floor_block.clone()), threshold_pos).await;
    }

    // Get door block from palette (use PrimaryWood's door form)
    let door_block_id = palette
        .get_block(MaterialRole::PrimaryWood, &BlockForm::Door, materials, rng)
        .cloned()
        .unwrap_or_else(|| BlockID::from("oak_door"));

    // Get lintel block (same as wall material)
    let lintel_block = palette
        .get_block(MaterialRole::PrimaryWall, &BlockForm::Block, materials, rng)
        .cloned()
        .unwrap_or_else(|| BlockID::from("stone_bricks"));

    

    

    // Place door blocks if this door type has them
    if door_type.has_door_block() {
        for i in 0..opening.width {
            let pos_idx = (opening.position + i) as usize;
            if pos_idx >= positions.len() {
                continue;
            }
            let pos_2d = positions[pos_idx];

            // Determine hinge side (hinges on outer edges for double doors)
            let hinge_right = i == 0;

            // Place lower door block
            let lower_pos = pos_2d.add_y(floor_y);
            place_door_block(editor, lower_pos, &door_block_id, facing, false, hinge_right).await;

            // Place upper door block
            let upper_pos = pos_2d.add_y(floor_y + 1);
            place_door_block(editor, upper_pos, &door_block_id, facing, true, hinge_right).await;
        }
    }

    // Place lintel above the door
    let lintel_y = floor_y + door_type.height();
    for i in 0..opening.width {
        let pos_idx = (opening.position + i) as usize;
        if pos_idx >= positions.len() {
            continue;
        }
        let pos_2d = positions[pos_idx];
        let lintel_pos = pos_2d.add_y(lintel_y);
        editor.place_block(&Block::from(lintel_block.clone()), lintel_pos).await;
    }
}

/// Places all door openings for a wall segment on a specific floor.
pub async fn place_door_openings(
    segment: &WallSegment,
    floor_y: i32,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    for opening in &segment.openings {
        if opening.is_door() {
            place_door_opening(segment, opening, floor_y, editor, palette, materials, rng).await;
        }
    }
}

/// Places all doors for a frame (ground floor only per the plan).
pub async fn place_doors(
    frame: &Frame,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    let floor_y = frame.floor_y(0); // Doors only on ground floor

    for segment in frame.wall_segments() {
        place_door_openings(segment, floor_y, editor, palette, materials, rng).await;
    }
}

/// Places a single window opening (glass blocks + lintel) in a wall segment.
pub async fn place_window_opening(
    segment: &WallSegment,
    opening: &Opening,
    floor_y: i32,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    if !opening.is_window() {
        return; // Not a window
    }

    let positions = segment.positions();

    // Get glass block for windows - use glass block (not pane) for consistency
    let glass_block_id = BlockID::from("glass");

    // Get lintel block (same as wall material)
    let lintel_block = palette
        .get_block(MaterialRole::PrimaryWall, &BlockForm::Block, materials, rng)
        .cloned()
        .unwrap_or_else(|| BlockID::from("stone_bricks"));

    // Place glass blocks for the window
    for i in 0..opening.width {
        let pos_idx = (opening.position + i) as usize;
        if pos_idx >= positions.len() {
            continue;
        }
        let pos_2d = positions[pos_idx];

        for y in 0..opening.height {
            let glass_y = floor_y + opening.y_offset + y;
            let glass_pos = pos_2d.add_y(glass_y);
            editor.place_block(&Block::from(glass_block_id.clone()), glass_pos).await;
        }
    }

    // Place lintel above the window
    let lintel_y = floor_y + opening.y_offset + opening.height;
    for i in 0..opening.width {
        let pos_idx = (opening.position + i) as usize;
        if pos_idx >= positions.len() {
            continue;
        }
        let pos_2d = positions[pos_idx];
        let lintel_pos = pos_2d.add_y(lintel_y);
        editor.place_block(&Block::from(lintel_block.clone()), lintel_pos).await;
    }
}

/// Places all window openings for a wall segment on a specific floor.
pub async fn place_window_openings(
    segment: &WallSegment,
    floor_y: i32,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    for opening in &segment.openings {
        if opening.is_window() {
            place_window_opening(segment, opening, floor_y, editor, palette, materials, rng).await;
        }
    }
}

/// Places all windows for a frame (all floors).
pub async fn place_windows(
    frame: &Frame,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    for floor in 0..frame.floors {
        let floor_y = frame.floor_y(floor);

        for segment in frame.wall_segments() {
            place_window_openings(segment, floor_y, editor, palette, materials, rng).await;
        }
    }
}

/// Configuration for building placement.
pub struct PlacementConfig {
    /// Whether to place corner posts.
    pub place_corners: bool,
    /// Whether to place walls.
    pub place_walls: bool,
    /// Whether to place floors.
    pub place_floors: bool,
    /// Whether to place doors.
    pub place_doors: bool,
    /// Whether to place windows.
    pub place_windows: bool,
}

impl Default for PlacementConfig {
    fn default() -> Self {
        Self {
            place_corners: true,
            place_walls: true,
            place_floors: true,
            place_doors: true,
            place_windows: true,
        }
    }
}

/// Places a building frame with configuration options.
pub async fn place_frame_with_config(
    frame: &Frame,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
    config: &PlacementConfig,
) {
    if config.place_corners {
        place_corner_posts(frame, editor, palette, materials, rng).await;
    }
    if config.place_walls {
        place_walls(frame, editor, palette, materials, rng).await;
    }
    if config.place_floors {
        place_floors(frame, editor, palette, materials, rng).await;
    }
    if config.place_doors {
        place_doors(frame, editor, palette, materials, rng).await;
    }
    if config.place_windows {
        place_windows(frame, editor, palette, materials, rng).await;
    }
}
