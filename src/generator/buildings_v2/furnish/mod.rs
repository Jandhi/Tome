#[cfg(test)]
mod test;
pub mod data;

use std::collections::{HashMap, HashSet, VecDeque};

use serde_derive::Deserialize;

use crate::editor::Editor;
use crate::geometry::{Cardinal, Point2D, Point3D, Rect2D};
use crate::minecraft::{Block, BlockID};
use crate::noise::RNG;
use super::frame::Frame;
use super::rooms::{FloorCell, FloorMap, Room, RoomPlan};
use super::RoomType;
use data::{FurnitureItemDef, RoomFurnitureDef, resolve_furniture_list};

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// What constraint a furniture block imposes on its floor cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellConstraint {
    Wall,
    Accessible,
    None,
}

/// How a block's "facing" state relates to the wall direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FacingMode {
    #[default]
    None,
    AwayFromWall,
    TowardWall,
    Perpendicular,
}

/// Which vertical layer a block occupies at its (x,z) coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockLayer {
    Ground,
    Ceiling,
}

/// A block to place in the world, with offset from anchor.
#[derive(Debug, Clone)]
pub struct PlacedBlock {
    pub block: Block,
    pub offset: (i32, i32, i32),  // (along_wall, away_from_wall, up)
    pub layer: BlockLayer,
}

/// A floor cell constraint, with offset from anchor.
#[derive(Debug, Clone)]
pub struct PlacedConstraint {
    pub offset: (i32, i32),       // (along_wall, away_from_wall)
    pub constraint: CellConstraint,
    pub facing: FacingMode,
}

/// A complete furniture piece.
#[derive(Debug, Clone)]
pub struct FurnitureItem {
    pub name: String,
    pub unique: bool,
    pub blocks: Vec<PlacedBlock>,
    pub constraints: Vec<PlacedConstraint>,
}

/// Furniture lists for a room type.
#[derive(Debug, Clone)]
pub struct FurnitureList {
    pub required: Vec<FurnitureItem>,
    pub optional: Vec<FurnitureItem>,
}

/// Maximum fraction of interior cells filled before stopping optional placement.
const FILL_THRESHOLD: f32 = 0.4;

// ---------------------------------------------------------------------------
// Offset and facing resolution
// ---------------------------------------------------------------------------

/// Convert a wall-relative offset to world (dx, dz, dy).
fn resolve_offset(offset: (i32, i32, i32), wall_dir: Cardinal) -> (i32, i32, i32) {
    let along: Point2D = wall_dir.rotate_right().into();
    let away: Point2D = (-wall_dir).into();
    let dx = along.x * offset.0 + away.x * offset.1;
    let dz = along.y * offset.0 + away.y * offset.1;
    (dx, dz, offset.2)
}

/// Convert a 2D wall-relative offset to world (dx, dz).
fn resolve_offset_2d(offset: (i32, i32), wall_dir: Cardinal) -> (i32, i32) {
    let (dx, dz, _) = resolve_offset((offset.0, offset.1, 0), wall_dir);
    (dx, dz)
}

/// Resolve facing for a constraint given the wall direction.
fn resolve_facing(mode: FacingMode, wall_dir: Cardinal) -> Option<String> {
    match mode {
        FacingMode::None => Option::None,
        FacingMode::AwayFromWall => Some((-wall_dir).to_string()),
        FacingMode::TowardWall => Some(wall_dir.to_string()),
        FacingMode::Perpendicular => Some(wall_dir.rotate_right().to_string()),
    }
}

/// Clone a block and merge a facing state into it.
fn apply_facing(block: &Block, facing: Option<String>) -> Block {
    let mut result = block.clone();
    if let Some(f) = facing {
        let state = result.state.get_or_insert_with(HashMap::new);
        state.insert("facing".into(), f);
    }
    result
}

/// Map a cell constraint to its FloorCell impact.
fn constraint_to_floor_cell(constraint: CellConstraint) -> Option<FloorCell> {
    match constraint {
        CellConstraint::Wall => Some(FloorCell::Blocked),
        CellConstraint::Accessible => Some(FloorCell::ReachableBlocked),
        CellConstraint::None => Option::None,
    }
}

/// Whether a furniture item needs wall-slot anchoring.
pub fn needs_wall(item: &FurnitureItem) -> bool {
    item.constraints.iter().any(|c| {
        c.constraint == CellConstraint::Wall || c.facing != FacingMode::None
    })
}

// ---------------------------------------------------------------------------
// RoomType → data key mapping
// ---------------------------------------------------------------------------

impl RoomType {
    pub fn furniture_key(&self) -> &'static str {
        match self {
            RoomType::Common => "common",
            RoomType::Hearth => "hearth",
            RoomType::GreatRoom => "great_room",
            RoomType::Bedroom => "bedroom",
            RoomType::MultiBedroom => "multi_bedroom",
            RoomType::MasterBedroom => "master_bedroom",
            RoomType::Study => "study",
            RoomType::Storage => "storage",
            RoomType::Dining => "dining",
            RoomType::Kitchen => "kitchen",
            RoomType::Pantry => "pantry",
            RoomType::Library => "library",
            RoomType::Studio => "studio",
            RoomType::Armory => "armory",
        }
    }
}

// ---------------------------------------------------------------------------
// Wall slots
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct WallSlot {
    cell: Point2D,
    wall_dir: Cardinal,
}

fn interior_rect(room: &Room) -> Option<Rect2D> {
    let shrunk = room.rect.shrink(1);
    if shrunk.size.x <= 0 || shrunk.size.y <= 0 { Option::None } else { Some(shrunk) }
}

fn wall_slots(interior: &Rect2D) -> Vec<WallSlot> {
    let mut slots = Vec::new();
    for cell in interior.iter() {
        if cell.x == interior.min().x { slots.push(WallSlot { cell, wall_dir: Cardinal::West }); }
        if cell.x == interior.max().x { slots.push(WallSlot { cell, wall_dir: Cardinal::East }); }
        if cell.y == interior.min().y { slots.push(WallSlot { cell, wall_dir: Cardinal::North }); }
        if cell.y == interior.max().y { slots.push(WallSlot { cell, wall_dir: Cardinal::South }); }
    }
    slots
}

// ---------------------------------------------------------------------------
// Connectivity
// ---------------------------------------------------------------------------

const NEIGHBORS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

fn is_walkable(floor_map: &FloorMap, cell: (i32, i32)) -> bool {
    matches!(floor_map.get(&cell), Some(FloorCell::Open | FloorCell::ReachableOpen))
}

fn flood_fill(start: (i32, i32), floor_map: &FloorMap) -> HashSet<(i32, i32)> {
    let mut visited = HashSet::new();
    if !is_walkable(floor_map, start) { return visited; }
    let mut queue = VecDeque::new();
    visited.insert(start);
    queue.push_back(start);
    while let Some((x, z)) = queue.pop_front() {
        for (dx, dz) in NEIGHBORS {
            let next = (x + dx, z + dz);
            if is_walkable(floor_map, next) && visited.insert(next) {
                queue.push_back(next);
            }
        }
    }
    visited
}

fn check_connectivity(floor_map: &FloorMap) -> bool {
    let entrances: Vec<(i32, i32)> = floor_map.iter()
        .filter(|(_, &v)| v == FloorCell::ReachableOpen)
        .map(|(&k, _)| k)
        .collect();

    if entrances.is_empty() { return true; }

    let reached = flood_fill(entrances[0], floor_map);

    if !entrances.iter().all(|e| reached.contains(e)) { return false; }

    for (&cell, &state) in floor_map {
        if state != FloorCell::ReachableBlocked { continue; }
        let adjacent = NEIGHBORS.iter()
            .any(|&(dx, dz)| reached.contains(&(cell.0 + dx, cell.1 + dz)));
        if !adjacent { return false; }
    }

    true
}

fn placement_keeps_connectivity(
    changes: &[((i32, i32), FloorCell)],
    floor_map: &FloorMap,
) -> bool {
    let mut temp = floor_map.clone();
    for &(cell, state) in changes { temp.insert(cell, state); }
    check_connectivity(&temp)
}

// ---------------------------------------------------------------------------
// Fill ratio
// ---------------------------------------------------------------------------

fn fill_ratio(floor_map: &FloorMap) -> f32 {
    let total = floor_map.len();
    if total == 0 { return 0.0; }
    let filled = floor_map.values()
        .filter(|&&v| v != FloorCell::Open && v != FloorCell::ReachableOpen)
        .count();
    filled as f32 / total as f32
}

// ---------------------------------------------------------------------------
// Occupancy map
// ---------------------------------------------------------------------------

pub struct OccupancyMap {
    ground: HashSet<(i32, i32)>,
    ceiling: HashSet<(i32, i32)>,
}

impl OccupancyMap {
    fn new() -> Self {
        Self { ground: HashSet::new(), ceiling: HashSet::new() }
    }

    fn is_occupied(&self, cell: (i32, i32), layer: BlockLayer) -> bool {
        match layer {
            BlockLayer::Ground => self.ground.contains(&cell),
            BlockLayer::Ceiling => self.ceiling.contains(&cell),
        }
    }

    fn insert(&mut self, cell: (i32, i32), layer: BlockLayer) {
        match layer {
            BlockLayer::Ground => self.ground.insert(cell),
            BlockLayer::Ceiling => self.ceiling.insert(cell),
        };
    }
}

// ---------------------------------------------------------------------------
// Placement algorithm
// ---------------------------------------------------------------------------

struct ResolvedBlock {
    world_pos: Point3D,
    cell: (i32, i32),
    block: Block,
    floor_cell: Option<FloorCell>,
    layer: BlockLayer,
}

/// Try to place a furniture item anchored at a wall slot.
fn try_place_at_wall_slot(
    item: &FurnitureItem,
    slot: &WallSlot,
    interior: &Rect2D,
    floor_map: &FloorMap,
    occupied: &OccupancyMap,
    floor_y: i32,
) -> Option<Vec<ResolvedBlock>> {
    let mut resolved = Vec::new();
    let mut changes: Vec<((i32, i32), FloorCell)> = Vec::new();

    // Validate and collect constraint changes
    for pc in &item.constraints {
        let (dx, dz) = resolve_offset_2d(pc.offset, slot.wall_dir);
        let wx = slot.cell.x + dx;
        let wz = slot.cell.y + dz;
        let cell = (wx, wz);

        if let Some(fc) = constraint_to_floor_cell(pc.constraint) {
            if !interior.contains(Point2D::new(wx, wz)) { return Option::None; }
            if !matches!(floor_map.get(&cell), Some(FloorCell::Open)) { return Option::None; }

            if pc.constraint == CellConstraint::Wall && !interior.on_edge(Point2D::new(wx, wz)) {
                return Option::None;
            }

            changes.push((cell, fc));
        }
    }

    // Check connectivity with all constraint changes
    if !changes.is_empty() && !placement_keeps_connectivity(&changes, floor_map) {
        return Option::None;
    }

    // Resolve blocks — find matching constraint for facing
    for pb in &item.blocks {
        let (dx, dz, dy) = resolve_offset(pb.offset, slot.wall_dir);
        let wx = slot.cell.x + dx;
        let wz = slot.cell.y + dz;
        let wy = floor_y + dy;
        let cell = (wx, wz);

        if occupied.is_occupied(cell, pb.layer) { return Option::None; }

        // Find matching constraint for this block's 2D offset
        let facing = item.constraints.iter()
            .find(|c| c.offset == (pb.offset.0, pb.offset.1))
            .and_then(|c| resolve_facing(c.facing, slot.wall_dir));

        let block = apply_facing(&pb.block, facing);

        // Floor cell from matching constraint
        let floor_cell = item.constraints.iter()
            .find(|c| c.offset == (pb.offset.0, pb.offset.1))
            .and_then(|c| constraint_to_floor_cell(c.constraint));

        resolved.push(ResolvedBlock {
            world_pos: Point3D::new(wx, wy, wz),
            cell,
            block,
            floor_cell,
            layer: pb.layer,
        });
    }

    Some(resolved)
}

/// Place a non-wall item at the room center (lanterns, etc.).
fn try_place_ceiling(
    item: &FurnitureItem,
    interior: &Rect2D,
    occupied: &OccupancyMap,
    ceiling_y: i32,
) -> Option<Vec<ResolvedBlock>> {
    let center = interior.midpoint();
    let mut resolved = Vec::new();

    for pb in &item.blocks {
        let cell = (center.x + pb.offset.0, center.y + pb.offset.1);
        if occupied.is_occupied(cell, pb.layer) { return Option::None; }

        resolved.push(ResolvedBlock {
            world_pos: Point3D::new(cell.0, ceiling_y - 1 + pb.offset.2, cell.1),
            cell,
            block: pb.block.clone(),
            floor_cell: Option::None,
            layer: pb.layer,
        });
    }

    Some(resolved)
}

// ---------------------------------------------------------------------------
// Room furnishing
// ---------------------------------------------------------------------------

/// Try to place a single furniture item. Returns true if placed.
async fn try_place_item(
    editor: &Editor,
    item: &FurnitureItem,
    interior: &Rect2D,
    room: &mut Room,
    occupied: &mut OccupancyMap,
    placed_unique: &mut HashSet<String>,
    slots: &[WallSlot],
    floor_y: i32,
    ceiling_y: i32,
) -> bool {
    if item.unique && placed_unique.contains(&item.name) {
        return false;
    }

    let result = if needs_wall(item) {
        slots.iter().find_map(|slot| {
            try_place_at_wall_slot(item, slot, interior, &room.floor_map, occupied, floor_y)
        })
    } else {
        try_place_ceiling(item, interior, occupied, ceiling_y)
    };

    if let Some(resolved_blocks) = result {
        if item.unique {
            placed_unique.insert(item.name.clone());
        }

        for rb in &resolved_blocks {
            editor.place_block(&rb.block, rb.world_pos).await;
            occupied.insert(rb.cell, rb.layer);
            if let Some(fc) = rb.floor_cell {
                room.floor_map.insert(rb.cell, fc);
            }
        }
        true
    } else {
        false
    }
}

/// Place furniture in a single room.
async fn furnish_room(
    editor: &Editor,
    room: &mut Room,
    frame: &Frame,
    furniture: &FurnitureList,
    rng: &mut RNG,
) {
    let interior = match interior_rect(room) {
        Some(r) => r,
        Option::None => return,
    };

    let floor_y = frame.floor_y(room.floor);
    let ceiling_y = frame.ceiling_y(room.floor);

    let mut slots = wall_slots(&interior);
    shuffle(&mut slots, rng);

    let mut occupied = OccupancyMap::new();
    let mut placed_unique: HashSet<String> = HashSet::new();

    for item in &furniture.required {
        try_place_item(
            editor, item, &interior, room, &mut occupied, &mut placed_unique,
            &slots, floor_y, ceiling_y,
        ).await;
    }

    for item in &furniture.optional {
        if fill_ratio(&room.floor_map) >= FILL_THRESHOLD {
            break;
        }
        try_place_item(
            editor, item, &interior, room, &mut occupied, &mut placed_unique,
            &slots, floor_y, ceiling_y,
        ).await;
    }
}

/// Furnish all rooms in a building using loaded furniture data.
pub async fn furnish_rooms(
    editor: &Editor,
    room_plan: &mut RoomPlan,
    frame: &Frame,
    furniture_items: &HashMap<String, FurnitureItemDef>,
    room_furniture: &HashMap<String, RoomFurnitureDef>,
    rng: &mut RNG,
) {
    for room in &mut room_plan.rooms {
        let key = room.room_type.furniture_key();
        let furniture = resolve_furniture_list(key, room_furniture, furniture_items);
        let mut room_rng = rng.derive();
        furnish_room(editor, room, frame, &furniture, &mut room_rng).await;
    }
}

fn shuffle<T>(items: &mut [T], rng: &mut RNG) {
    for i in (1..items.len()).rev() {
        let j = rng.rand_i32_range(0, (i + 1) as i32) as usize;
        items.swap(i, j);
    }
}
