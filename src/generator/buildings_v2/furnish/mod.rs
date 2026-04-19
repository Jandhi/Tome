#[cfg(test)]
mod test;
pub mod data;

use std::collections::{HashMap, HashSet, VecDeque};

use serde_derive::Deserialize;

use crate::editor::Editor;
use crate::generator::materials::{Material, MaterialId, MaterialRole, Palette};
use crate::geometry::{Cardinal, Point2D, Point3D, Rect2D};
use crate::minecraft::{Block, BlockForm, color_block, string_to_block};
use crate::noise::RNG;
use super::frame::Frame;
use super::pipeline::BuildCtx;
use super::rooms::{CellState, ConstraintMap, PlacedFurniture, Room, RoomPlan};
use data::{Furniture, PaletteSwap, RoomFurnitureList};

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// What constraint a furniture block imposes on its floor cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellConstraint {
    Wall,
    BlockedReachable,
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
/// `Both` is for blocks that should reserve both layer slots — e.g. a wall
/// banner that wants to keep a hanging lantern from being placed directly
/// above it, even when the banner itself only places a single block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockLayer {
    Ground,
    Ceiling,
    Both,
}

impl BlockLayer {
    pub fn occupies_ground(self) -> bool { matches!(self, BlockLayer::Ground | BlockLayer::Both) }
    pub fn occupies_ceiling(self) -> bool { matches!(self, BlockLayer::Ceiling | BlockLayer::Both) }
}

/// Default fraction of interior cells filled before stopping optional placement.
/// Room types with `fill_threshold` set in rooms.yaml override this.
const DEFAULT_FILL_THRESHOLD: f32 = 0.75;

// ---------------------------------------------------------------------------
// Offset and facing resolution
// ---------------------------------------------------------------------------

/// Convert a wall-relative offset [along, y, away] to world (dx, dz, dy).
fn resolve_offset(offset: [i32; 3], wall_dir: Cardinal) -> (i32, i32, i32) {
    let along: Point2D = wall_dir.rotate_right().into();
    let away: Point2D = (-wall_dir).into();
    let dx = along.x * offset[0] + away.x * offset[2];
    let dz = along.y * offset[0] + away.y * offset[2];
    (dx, dz, offset[1])
}

/// Convert a 2D wall-relative offset [along, away] to world (dx, dz).
fn resolve_offset_2d(offset: [i32; 2], wall_dir: Cardinal) -> (i32, i32) {
    let (dx, dz, _) = resolve_offset([offset[0], 0, offset[1]], wall_dir);
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

/// Parse a block string into a Block.
fn parse_block(block_str: &str) -> Block {
    string_to_block(block_str)
        .unwrap_or_else(|| Block::from_id(block_str.into()))
}

/// Apply palette substitution to a block.
pub(crate) fn swap_block_for_palette(
    block: Block,
    swap: PaletteSwap,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) -> Block {
    match swap {
        PaletteSwap::None => block,
        PaletteSwap::Wood => {
            // Furniture wants the SECONDARY wood so it contrasts with the
            // building's primary wood (used for floors/frame). Palette
            // auto-falls-back to PrimaryWood when SecondaryWood isn't defined
            // (see MaterialRole::backup_role).
            let form = BlockForm::infer_from_block(&block.id);
            if let Some(new_id) = palette.get_block(MaterialRole::SecondaryWood, &form, materials, rng) {
                Block::new(new_id.clone(), block.state, block.data)
            } else {
                block
            }
        }
        PaletteSwap::Color => {
            if let Some(color) = palette.primary_color {
                Block::new(color_block(block.id, color), block.state, block.data)
            } else {
                block
            }
        }
    }
}

/// Rotate any existing `facing` state in a block.
/// North is identity (no rotation). East = 1 clockwise, South = 2, West = 3.
fn rotate_block(block: &Block, dir: Cardinal) -> Block {
    let mut result = block.clone();
    if let Some(state) = &mut result.state {
        if let Some(facing) = state.get("facing") {
            let parsed: Option<Cardinal> = match facing.as_str() {
                "north" => Some(Cardinal::North),
                "south" => Some(Cardinal::South),
                "east" => Some(Cardinal::East),
                "west" => Some(Cardinal::West),
                _ => None,
            };
            if let Some(orig) = parsed {
                let rotated = match dir {
                    Cardinal::North => orig,
                    Cardinal::East => orig.rotate_right(),
                    Cardinal::South => orig.rotate_right().rotate_right(),
                    Cardinal::West => orig.rotate_right().rotate_right().rotate_right(),
                };
                state.insert("facing".into(), rotated.to_string());
            }
        }
    }
    result
}

/// Whether a furniture item is a ceiling-only item (lanterns, etc.).
fn is_ceiling_item(item: &Furniture) -> bool {
    item.blocks.iter().all(|b| b.layer == BlockLayer::Ceiling)
}

/// Whether a furniture item must be placed against a wall
/// (has a Wall constraint or a facing that needs wall direction).
fn needs_wall(item: &Furniture) -> bool {
    item.constraints.iter().any(|c| {
        c.constraint == CellConstraint::Wall || c.facing != FacingMode::None
    })
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
    let interior = room.interior;
    if interior.size.x <= 0 || interior.size.y <= 0 { Option::None } else { Some(interior) }
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

/// Flood fill from `start` through walkable cells in the constraint map.
fn flood_fill(start: (i32, i32), constraints: &ConstraintMap) -> HashSet<(i32, i32)> {
    let mut visited = HashSet::new();
    if !constraints.is_walkable(start) { return visited; }
    let mut queue = VecDeque::new();
    visited.insert(start);
    queue.push_back(start);
    while let Some((x, z)) = queue.pop_front() {
        for (dx, dz) in NEIGHBORS {
            let next = (x + dx, z + dz);
            if constraints.is_walkable(next) && visited.insert(next) {
                queue.push_back(next);
            }
        }
    }
    visited
}

/// Check that all reserved cells are reachable from each other.
/// Reserved cells aren't walkable themselves, so we verify each one
/// is adjacent to at least one cell in the walkable flood-fill region.
fn check_connectivity(constraints: &ConstraintMap) -> bool {
    let reserved: Vec<(i32, i32)> = constraints.iter_ground()
        .filter(|(_, s)| *s == CellState::BlockedReachable)
        .map(|(k, _)| k)
        .collect();

    if reserved.is_empty() { return true; }

    // Start flood fill from a walkable cell adjacent to any BlockedReachable cell
    let start = reserved.iter()
        .flat_map(|&(x, z)| NEIGHBORS.iter().map(move |&(dx, dz)| (x + dx, z + dz)))
        .find(|&cell| constraints.is_walkable(cell));

    let reached = match start {
        Some(s) => flood_fill(s, constraints),
        None => HashSet::new(),
    };

    // Every BlockedReachable cell must be adjacent to the reached walkable region
    reserved.iter().all(|&(x, z)| {
        NEIGHBORS.iter().any(|&(dx, dz)| reached.contains(&(x + dx, z + dz)))
    })
}

/// Check whether adding new constraints + block placements would break connectivity.
/// `block_cells` pairs each ground-block cell with its `walkable` flag —
/// non-walkable cells become Blocked, walkable cells become UnblockedReachable
/// so the connectivity flood fill treats them correctly.
/// Temporarily applies changes, checks, then restores originals to avoid cloning.
fn placement_keeps_connectivity(
    new_blocked: &[(i32, i32)],
    new_reserved: &[(i32, i32)],
    block_cells: &[((i32, i32), bool)],
    constraints: &mut ConstraintMap,
) -> bool {
    // Save original states for every cell we'll touch
    let saved: Vec<((i32, i32), CellState)> = new_blocked.iter()
        .chain(new_reserved.iter())
        .chain(block_cells.iter().map(|(c, _)| c))
        .filter_map(|&cell| constraints.get(cell).map(|s| (cell, s)))
        .collect();

    // Apply changes. block_cells last so they override BR (e.g. bed foot:
    // has a BR constraint AND an explicit block; the final state is Blocked).
    for &cell in new_blocked { constraints.set(cell, CellState::Blocked); }
    for &cell in new_reserved { constraints.set(cell, CellState::BlockedReachable); }
    for &(cell, walkable) in block_cells {
        let state = if walkable { CellState::UnblockedReachable } else { CellState::Blocked };
        constraints.set(cell, state);
    }

    let ok = check_connectivity(constraints);

    // Restore originals
    for (cell, state) in saved { constraints.set(cell, state); }

    ok
}

/// Compute the ground-block cells a furniture item will occupy at a given
/// anchor, paired with each block's `walkable` flag.
fn ground_block_cells(
    item: &Furniture,
    anchor: Point2D,
    dir: Cardinal,
) -> Vec<((i32, i32), bool)> {
    item.blocks.iter()
        .filter(|pb| pb.layer.occupies_ground())
        .map(|pb| {
            let (dx, dz, _) = resolve_offset(pb.offset, dir);
            ((anchor.x + dx, anchor.y + dz), pb.walkable)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Placement algorithm
// ---------------------------------------------------------------------------

struct ResolvedBlock {
    world_pos: Point3D,
    cell: (i32, i32),
    block: Block,
    layer: BlockLayer,
    swap: PaletteSwap,
    walkable: bool,
}

struct PlacementResult {
    blocks: Vec<ResolvedBlock>,
    new_blocked: Vec<(i32, i32)>,
    new_reserved: Vec<(i32, i32)>,
}

/// Try to place a furniture item anchored at a wall slot.
fn try_place_at_wall_slot(
    item: &Furniture,
    slot: &WallSlot,
    interior: &Rect2D,
    constraints: &mut ConstraintMap,
    floor_y: i32,
) -> Option<PlacementResult> {
    let mut blocks = Vec::new();
    let mut new_blocked = Vec::new();
    let mut new_reserved = Vec::new();

    // Validate constraints and collect changes
    for pc in &item.constraints {
        let (dx, dz) = resolve_offset_2d(pc.offset, slot.wall_dir);
        let cell = (slot.cell.x + dx, slot.cell.y + dz);

        match pc.constraint {
            CellConstraint::Wall => {
                if !constraints.is_open(cell) { return Option::None; }
                if !interior.on_edge(Point2D::new(cell.0, cell.1)) { return Option::None; }
                new_blocked.push(cell);
            }
            CellConstraint::BlockedReachable => {
                if !constraints.is_open(cell) { return Option::None; }
                new_reserved.push(cell);
            }
            CellConstraint::None => {}
        }
    }

    // Pre-compute ground-block cells and verify they're open. Must come before
    // the connectivity check so block placements are treated as blocking.
    let block_cells = ground_block_cells(item, slot.cell, slot.wall_dir);
    for &(cell, _) in &block_cells {
        if !constraints.is_open(cell) { return Option::None; }
    }

    // Check connectivity with proposed changes (constraints + block placements)
    if (!new_blocked.is_empty() || !new_reserved.is_empty() || !block_cells.is_empty())
        && !placement_keeps_connectivity(&new_blocked, &new_reserved, &block_cells, constraints)
    {
        return Option::None;
    }

    // Resolve blocks
    for pb in &item.blocks {
        let (dx, dz, dy) = resolve_offset(pb.offset, slot.wall_dir);
        let cell = (slot.cell.x + dx, slot.cell.y + dz);

        if pb.layer.occupies_ceiling() && constraints.ceiling_occupied(cell) {
            return Option::None;
        }

        let facing = item.constraints.iter()
            .find(|c| c.offset == [pb.offset[0], pb.offset[1]])
            .and_then(|c| resolve_facing(c.facing, slot.wall_dir));

        blocks.push(ResolvedBlock {
            world_pos: Point3D::new(cell.0, floor_y + dy, cell.1),
            cell,
            // Rotate YAML-authored facings (e.g. wall_sign[facing=west],
            // trapdoor[facing=south]) by the wall direction. Constraint-derived
            // facings still override via apply_facing — same shape as the
            // freestanding path below.
            block: apply_facing(&rotate_block(&parse_block(&pb.block), slot.wall_dir), facing),
            layer: pb.layer,
            swap: pb.swap,
            walkable: pb.walkable,
        });
    }

    Some(PlacementResult { blocks, new_blocked, new_reserved })
}

/// Try to place a freestanding item at any open cell in the interior.
/// Tries all 4 rotations at each cell.
fn try_place_freestanding(
    item: &Furniture,
    interior: &Rect2D,
    constraints: &mut ConstraintMap,
    floor_y: i32,
    open_cells: &[(i32, i32)],
) -> Option<PlacementResult> {
    let rotations = [Cardinal::North, Cardinal::East, Cardinal::South, Cardinal::West];

    for &(ax, az) in open_cells {
        for &dir in &rotations {
            let mut blocks = Vec::new();
            let mut new_blocked = Vec::new();
            let mut new_reserved = Vec::new();
            let mut ok = true;

            for pc in &item.constraints {
                let (dx, dz) = resolve_offset_2d(pc.offset, dir);
                let cell = (ax + dx, az + dz);
                match pc.constraint {
                    CellConstraint::Wall => {
                        if !constraints.is_open(cell) { ok = false; break; }
                        if !interior.on_edge(Point2D::new(cell.0, cell.1)) { ok = false; break; }
                        new_blocked.push(cell);
                    }
                    CellConstraint::BlockedReachable => {
                        if !constraints.is_open(cell) { ok = false; break; }
                        new_reserved.push(cell);
                    }
                    CellConstraint::None => {}
                }
            }
            if !ok { continue; }

            // Pre-compute ground block cells and verify they're open + in interior.
            let block_cells = ground_block_cells(item, Point2D::new(ax, az), dir);
            let mut block_ok = true;
            for &(cell, _) in &block_cells {
                if !interior.contains(Point2D::new(cell.0, cell.1)) { block_ok = false; break; }
                if !constraints.is_open(cell) { block_ok = false; break; }
            }
            if !block_ok { continue; }

            if (!new_blocked.is_empty() || !new_reserved.is_empty() || !block_cells.is_empty())
                && !placement_keeps_connectivity(&new_blocked, &new_reserved, &block_cells, constraints)
            {
                continue;
            }

            for pb in &item.blocks {
                let (dx, dz, dy) = resolve_offset(pb.offset, dir);
                let cell = (ax + dx, az + dz);
                if pb.layer.occupies_ceiling() {
                    if !interior.contains(Point2D::new(cell.0, cell.1)) { ok = false; break; }
                    if constraints.ceiling_occupied(cell) { ok = false; break; }
                }

                let facing = item.constraints.iter()
                    .find(|c| c.offset == [pb.offset[0], pb.offset[1]])
                    .and_then(|c| resolve_facing(c.facing, dir));

                blocks.push(ResolvedBlock {
                    world_pos: Point3D::new(cell.0, floor_y + dy, cell.1),
                    cell,
                    block: apply_facing(&rotate_block(&parse_block(&pb.block), dir), facing),
                    layer: pb.layer,
                    swap: pb.swap,
                    walkable: pb.walkable,
                });
            }
            if !ok { continue; }

            return Some(PlacementResult { blocks, new_blocked, new_reserved });
        }
    }
    None
}

/// Place a ceiling-only item at the room center (lanterns, etc.).
fn try_place_ceiling(
    item: &Furniture,
    interior: &Rect2D,
    constraints: &mut ConstraintMap,
    ceiling_y: i32,
) -> Option<PlacementResult> {
    let center = interior.midpoint();
    let mut blocks = Vec::new();

    for pb in &item.blocks {
        let cell = (center.x + pb.offset[0], center.y + pb.offset[1]);
        if constraints.ceiling_occupied(cell) { return Option::None; }

        blocks.push(ResolvedBlock {
            world_pos: Point3D::new(cell.0, ceiling_y - 1 + pb.offset[2], cell.1),
            cell,
            block: parse_block(&pb.block),
            layer: pb.layer,
            swap: pb.swap,
            walkable: pb.walkable,
        });
    }

    Some(PlacementResult { blocks, new_blocked: vec![], new_reserved: vec![] })
}

// ---------------------------------------------------------------------------
// Room furnishing
// ---------------------------------------------------------------------------

/// Try to place a single furniture item. Returns the occupied cells if placed.
async fn try_place_item(
    editor: &Editor,
    item: &Furniture,
    interior: &Rect2D,
    constraints: &mut ConstraintMap,
    slots: &[WallSlot],
    open_cells: &[(i32, i32)],
    floor_y: i32,
    ceiling_y: i32,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) -> Option<Vec<(i32, i32)>> {
    let result = if is_ceiling_item(item) {
        try_place_ceiling(item, interior, constraints, ceiling_y)
    } else if needs_wall(item) {
        let mut found = None;
        for slot in slots {
            if let Some(r) = try_place_at_wall_slot(item, slot, interior, constraints, floor_y) {
                found = Some(r);
                break;
            }
        }
        found
    } else {
        try_place_freestanding(item, interior, constraints, floor_y, open_cells)
    };

    if let Some(placement) = result {
        let mut cells = Vec::new();
        for &cell in &placement.new_blocked { constraints.set(cell, CellState::Blocked); }
        for &cell in &placement.new_reserved { constraints.set(cell, CellState::BlockedReachable); }
        for rb in &placement.blocks {
            let block = swap_block_for_palette(rb.block.clone(), rb.swap, palette, materials, rng);
            editor.place_block(&block, rb.world_pos).await;
            if rb.layer.occupies_ceiling() {
                constraints.set_ceiling(rb.cell);
            }
            if rb.layer.occupies_ground() {
                let state = if rb.walkable { CellState::UnblockedReachable } else { CellState::Blocked };
                constraints.set(rb.cell, state);
                cells.push(rb.cell);
            }
        }
        // Wall-constraint cells that don't have an explicit block still belong
        // to the item — e.g. the bed head, which Minecraft auto-generates from
        // the foot. Add them to the returned cells so the blueprint shows them.
        for &cell in &placement.new_blocked {
            if !cells.contains(&cell) {
                cells.push(cell);
            }
        }
        Some(cells)
    } else {
        None
    }
}


/// Every tag that identifies an item — its own name (implicit self-tag)
/// plus any explicit tags declared in YAML.
fn item_tags<'a>(name: &'a str, item: &'a Furniture) -> impl Iterator<Item = &'a str> {
    std::iter::once(name).chain(item.tags.iter().map(String::as_str))
}

/// Resolve a rooms.yaml entry (like `bed` or `chair`) to every eligible
/// furniture item. Candidates match by name or explicit tag, pass the
/// room-area gates, aren't ceiling items in an attic, and — if unique —
/// don't share any tag with an already-placed unique item.
fn resolve_candidates<'a>(
    entry: &str,
    items: &'a HashMap<String, Furniture>,
    room_area: i32,
    is_attic: bool,
    placed_tags: &HashSet<String>,
    rng: &mut RNG,
) -> Vec<(&'a String, &'a Furniture)> {
    let mut out: Vec<(&String, &Furniture)> = items.iter()
        .filter(|(name, item)| {
            name.as_str() == entry || item.tags.iter().any(|t| t == entry)
        })
        .filter(|(_, item)| {
            item.min_room_area.map_or(true, |min| room_area >= min)
                && item.max_room_area.map_or(true, |max| room_area <= max)
        })
        .filter(|(_, item)| !(is_attic && is_ceiling_item(item)))
        .filter(|(name, item)| {
            !item.unique || !item_tags(name, item).any(|t| placed_tags.contains(t))
        })
        .collect();
    // HashMap iteration order is non-deterministic — sort before shuffling
    // so the RNG draw is the only source of randomness.
    out.sort_by(|a, b| a.0.cmp(b.0));
    shuffle(&mut out, rng);
    out
}

/// Place furniture in a single room.
async fn furnish_room(
    editor: &Editor,
    room: &mut Room,
    frame: &Frame,
    room_list: &RoomFurnitureList,
    items: &HashMap<String, Furniture>,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    let interior = match interior_rect(room) {
        Some(r) => r,
        Option::None => return,
    };

    let floor_y = frame.floor_y(room.floor);
    let ceiling_y = if room.role == super::rooms::RoomRole::Attic {
        frame.roof_y(room.rect_index)
    } else {
        frame.ceiling_y(room.floor)
    };
    let mut slots = wall_slots(&interior);
    shuffle(&mut slots, rng);

    let mut open_cells: Vec<(i32, i32)> = interior.iter().map(|p| (p.x, p.y)).collect();
    shuffle(&mut open_cells, rng);

    let room_area = interior.area();
    let is_attic = room.role == super::rooms::RoomRole::Attic;
    let mut placed_tags: HashSet<String> = HashSet::new();

    for entry in &room_list.required {
        let candidates = resolve_candidates(entry, items, room_area, is_attic, &placed_tags, rng);
        for (name, item) in candidates {
            if let Some(cells) = try_place_item(
                editor, item, &interior, &mut room.constraints,
                &slots, &open_cells, floor_y, ceiling_y,
                palette, materials, rng,
            ).await {
                if item.unique {
                    for tag in item_tags(name, item) {
                        placed_tags.insert(tag.to_string());
                    }
                }
                room.furniture.push(PlacedFurniture { name: name.clone(), cells });
                break;
            }
        }
    }

    let fill_threshold = room_list.fill_threshold.unwrap_or(DEFAULT_FILL_THRESHOLD);
    // Rooms that explicitly set a threshold (storage, pantry) run the
    // optional list in repeated passes until nothing more fits — packing
    // them until the threshold is hit or a full pass places nothing.
    let aggressive = room_list.fill_threshold.is_some();

    loop {
        if room.constraints.fill_ratio() >= fill_threshold {
            break;
        }
        let mut placed_this_pass = false;
        for entry in &room_list.optional {
            if room.constraints.fill_ratio() >= fill_threshold { break; }
            let candidates = resolve_candidates(entry, items, room_area, is_attic, &placed_tags, rng);
            for (name, item) in candidates {
                if let Some(cells) = try_place_item(
                    editor, item, &interior, &mut room.constraints,
                    &slots, &open_cells, floor_y, ceiling_y,
                    palette, materials, rng,
                ).await {
                    if item.unique {
                        for tag in item_tags(name, item) {
                            placed_tags.insert(tag.to_string());
                        }
                    }
                    room.furniture.push(PlacedFurniture { name: name.clone(), cells });
                    placed_this_pass = true;
                    break;
                }
            }
        }
        if !aggressive || !placed_this_pass {
            break;
        }
    }
}

/// Furnish all rooms in a building using loaded furniture data.
pub async fn furnish_rooms(
    ctx: &mut BuildCtx<'_>,
    room_plan: &mut RoomPlan,
    frame: &Frame,
) {
    let editor: &Editor = &*ctx.editor;
    let palette = ctx.palette;
    let furniture_data = &ctx.data.furniture;
    let materials = &ctx.data.materials;
    let rng = &mut *ctx.rng;

    for room in &mut room_plan.rooms {
        let key = room.room_type.furniture_key();
        let room_list = match furniture_data.rooms.get(key) {
            Some(r) => r,
            None => continue,
        };
        let mut room_rng = rng.derive();
        furnish_room(editor, room, frame, room_list, &furniture_data.items, palette, materials, &mut room_rng).await;
    }
}

fn shuffle<T>(items: &mut [T], rng: &mut RNG) {
    for i in (1..items.len()).rev() {
        let j = rng.rand_i32_range(0, (i + 1) as i32) as usize;
        items.swap(i, j);
    }
}
