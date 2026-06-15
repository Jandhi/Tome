//! The placement algorithm: wall slots, connectivity preservation, attic roof
//! clearance, and the three placement strategies (wall-anchored, freestanding,
//! ceiling) that produce a `PlacementResult` for a single furniture item.

use std::collections::{HashSet, VecDeque};

use crate::geometry::{Cardinal, Point2D, Point3D, Rect2D};
use crate::minecraft::Block;

use super::block::{apply_facing, parse_block, resolve_facing, resolve_offset, resolve_offset_2d, rotate_block};
use super::data::{Furniture, PaletteSwap};
use super::types::{BlockLayer, CellConstraint, FacingMode};
use super::super::roof::heightmap::RoofHeightmap;
use super::super::rooms::{CellState, ConstraintMap, Room};

/// Whether a furniture item is a ceiling-only item (lanterns, etc.).
pub(super) fn is_ceiling_item(item: &Furniture) -> bool {
    item.blocks.iter().all(|b| b.layer == BlockLayer::Ceiling)
}

/// Whether a furniture item must be placed against a wall
/// (has a Wall constraint or a facing that needs wall direction).
pub(super) fn needs_wall(item: &Furniture) -> bool {
    item.constraints.iter().any(|c| {
        c.constraint == CellConstraint::Wall || c.facing != FacingMode::None
    })
}

#[derive(Debug, Clone, Copy)]
pub(super) struct WallSlot {
    pub(super) cell: Point2D,
    pub(super) wall_dir: Cardinal,
}

pub(super) fn interior_rect(room: &Room) -> Option<Rect2D> {
    let interior = room.interior;
    if interior.size.x <= 0 || interior.size.y <= 0 { Option::None } else { Some(interior) }
}

pub(super) fn wall_slots(interior: &Rect2D) -> Vec<WallSlot> {
    let mut slots = Vec::new();
    for cell in interior.iter() {
        if cell.x == interior.min().x { slots.push(WallSlot { cell, wall_dir: Cardinal::West }); }
        if cell.x == interior.max().x { slots.push(WallSlot { cell, wall_dir: Cardinal::East }); }
        if cell.y == interior.min().y { slots.push(WallSlot { cell, wall_dir: Cardinal::North }); }
        if cell.y == interior.max().y { slots.push(WallSlot { cell, wall_dir: Cardinal::South }); }
    }
    slots
}

const NEIGHBORS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

/// Flood fill from `start` through walkable cells in the constraint map.
pub(super) fn flood_fill(start: (i32, i32), constraints: &ConstraintMap) -> HashSet<(i32, i32)> {
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
pub(super) fn check_connectivity(constraints: &ConstraintMap) -> bool {
    let walkable: Vec<(i32, i32)> = constraints.iter_ground()
        .filter(|(_, s)| matches!(s, CellState::Empty | CellState::UnblockedReachable))
        .map(|(k, _)| k)
        .collect();
    let reserved: Vec<(i32, i32)> = constraints.iter_ground()
        .filter(|(_, s)| *s == CellState::BlockedReachable)
        .map(|(k, _)| k)
        .collect();

    if walkable.is_empty() {
        // No walkable cells anywhere — only OK if there are no BR cells either.
        return reserved.is_empty();
    }

    // All walkable cells must form a single connected component.
    let reached = flood_fill(walkable[0], constraints);
    if !walkable.iter().all(|c| reached.contains(c)) {
        return false;
    }

    // Every BlockedReachable cell must touch that walkable component.
    reserved.iter().all(|&(x, z)| {
        NEIGHBORS.iter().any(|&(dx, dz)| reached.contains(&(x + dx, z + dz)))
    })
}

/// Check whether adding new constraints + block placements would break connectivity.
/// `block_cells` pairs each ground-block cell with its `walkable` flag —
/// non-walkable cells become Blocked, walkable cells become UnblockedReachable
/// so the connectivity flood fill treats them correctly.
/// Temporarily applies changes, checks, then restores originals to avoid cloning.
pub(super) fn placement_keeps_connectivity(
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
/// True if the item has a y=0 (floor-level) block at the given 2D
/// (along, away) offset — used to decide whether a `Wall`-constrained
/// cell is physically occupied at the floor or just hosts something
/// hanging above (e.g. a wall banner).
fn has_floor_block_at(item: &Furniture, offset_2d: [i32; 2]) -> bool {
    item.blocks.iter().any(|pb| {
        pb.offset[1] == 0
            && pb.offset[0] == offset_2d[0]
            && pb.offset[2] == offset_2d[1]
    })
}

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

/// Per-cell roof-block y for an attic room. Used to reject furniture cells
/// that would poke into (or against) the sloped roof.
pub(crate) struct RoofClearance<'a> {
    pub(super) hm: &'a RoofHeightmap,
    /// Wall-top y for this rect (= roof_y from Frame). The lowest roof block
    /// at (x, z) sits at `roof_y + heightmap.get(x, z).floor()`.
    pub(super) roof_y: i32,
}

impl RoofClearance<'_> {
    /// Y of the lowest roof block at (x, z), or None if outside the heightmap.
    fn roof_block_y(&self, cell: (i32, i32)) -> Option<i32> {
        let h = self.hm.get(cell.0, cell.1);
        if h == f32::NEG_INFINITY { None } else { Some(self.roof_y + h.floor() as i32) }
    }

    /// True if a furniture block at (cell, world_y) leaves at least one air
    /// cell of headroom above it (block at y, air at y+1, roof block at ≥y+2).
    fn allows_block(&self, cell: (i32, i32), world_y: i32) -> bool {
        match self.roof_block_y(cell) {
            Some(rb_y) => world_y + 1 < rb_y,
            None => false,
        }
    }
}

/// Reject a placement if any of its blocks fails the attic roof-clearance test.
fn placement_fits_under_roof(placement: &PlacementResult, clearance: &RoofClearance) -> bool {
    placement.blocks.iter().all(|rb| clearance.allows_block(rb.cell, rb.world_pos.y))
}

pub(super) struct ResolvedBlock {
    pub(super) world_pos: Point3D,
    pub(super) cell: (i32, i32),
    pub(super) block: Block,
    pub(super) layer: BlockLayer,
    pub(super) swap: PaletteSwap,
    pub(super) walkable: bool,
    pub(super) place: bool,
    pub(super) loot: Option<String>,
}

pub(super) struct PlacementResult {
    pub(super) blocks: Vec<ResolvedBlock>,
    pub(super) new_blocked: Vec<(i32, i32)>,
    pub(super) new_reserved: Vec<(i32, i32)>,
    /// EmptyReachable constraint cells: kept walkable, unplaceable.
    pub(super) new_empty_reachable: Vec<(i32, i32)>,
}

/// Try to place a furniture item anchored at a wall slot.
pub(super) fn try_place_at_wall_slot(
    item: &Furniture,
    slot: &WallSlot,
    interior: &Rect2D,
    constraints: &mut ConstraintMap,
    floor_y: i32,
    roof_clearance: Option<&RoofClearance>,
) -> Option<PlacementResult> {
    let mut blocks = Vec::new();
    let mut new_blocked = Vec::new();
    let mut new_reserved = Vec::new();
    let mut new_empty_reachable = Vec::new();

    // Validate constraints and collect changes
    for pc in &item.constraints {
        let (dx, dz) = resolve_offset_2d(pc.offset, slot.wall_dir);
        let cell = (slot.cell.x + dx, slot.cell.y + dz);

        match pc.constraint {
            CellConstraint::Wall => {
                if !constraints.is_open(cell) { return Option::None; }
                if !interior.on_edge(Point2D::new(cell.0, cell.1)) { return Option::None; }
                if has_floor_block_at(item, pc.offset) {
                    new_blocked.push(cell);
                } else {
                    new_empty_reachable.push(cell);
                }
            }
            CellConstraint::BlockedReachable => {
                if !constraints.is_open(cell) { return Option::None; }
                new_reserved.push(cell);
            }
            CellConstraint::EmptyReachable => {
                if !constraints.is_open(cell) { return Option::None; }
                new_empty_reachable.push(cell);
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
            .find(|c| c.offset == [pb.offset[0], pb.offset[2]])
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
            place: pb.place,
            loot: pb.loot.clone(),
        });
    }

    let placement = PlacementResult { blocks, new_blocked, new_reserved, new_empty_reachable };
    if let Some(rc) = roof_clearance {
        if !placement_fits_under_roof(&placement, rc) { return Option::None; }
    }
    Some(placement)
}

/// Try to place a freestanding item at any open cell in the interior.
/// Tries all 4 rotations at each cell.
pub(super) fn try_place_freestanding(
    item: &Furniture,
    interior: &Rect2D,
    constraints: &mut ConstraintMap,
    floor_y: i32,
    open_cells: &[(i32, i32)],
    roof_clearance: Option<&RoofClearance>,
) -> Option<PlacementResult> {
    let rotations = [Cardinal::North, Cardinal::East, Cardinal::South, Cardinal::West];

    for &(ax, az) in open_cells {
        for &dir in &rotations {
            let mut blocks = Vec::new();
            let mut new_blocked = Vec::new();
            let mut new_reserved = Vec::new();
            let mut new_empty_reachable = Vec::new();
            let mut ok = true;

            for pc in &item.constraints {
                let (dx, dz) = resolve_offset_2d(pc.offset, dir);
                let cell = (ax + dx, az + dz);
                match pc.constraint {
                    CellConstraint::Wall => {
                        if !constraints.is_open(cell) { ok = false; break; }
                        if !interior.on_edge(Point2D::new(cell.0, cell.1)) { ok = false; break; }
                        if has_floor_block_at(item, pc.offset) {
                            new_blocked.push(cell);
                        } else {
                            new_empty_reachable.push(cell);
                        }
                    }
                    CellConstraint::BlockedReachable => {
                        if !constraints.is_open(cell) { ok = false; break; }
                        new_reserved.push(cell);
                    }
                    CellConstraint::EmptyReachable => {
                        if !constraints.is_open(cell) { ok = false; break; }
                        new_empty_reachable.push(cell);
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
                    .find(|c| c.offset == [pb.offset[0], pb.offset[2]])
                    .and_then(|c| resolve_facing(c.facing, dir));

                blocks.push(ResolvedBlock {
                    world_pos: Point3D::new(cell.0, floor_y + dy, cell.1),
                    cell,
                    block: apply_facing(&rotate_block(&parse_block(&pb.block), dir), facing),
                    layer: pb.layer,
                    swap: pb.swap,
                    walkable: pb.walkable,
                    place: pb.place,
                    loot: pb.loot.clone(),
                });
            }
            if !ok { continue; }

            let placement = PlacementResult { blocks, new_blocked, new_reserved, new_empty_reachable };
            if let Some(rc) = roof_clearance {
                if !placement_fits_under_roof(&placement, rc) { continue; }
            }
            return Some(placement);
        }
    }
    None
}

/// Place a ceiling-only item at the room center (lanterns, etc.).
pub(super) fn try_place_ceiling(
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
            place: pb.place,
            loot: pb.loot.clone(),
        });
    }

    Some(PlacementResult { blocks, new_blocked: vec![], new_reserved: vec![], new_empty_reachable: vec![] })
}
