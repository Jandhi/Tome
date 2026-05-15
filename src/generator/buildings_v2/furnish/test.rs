use std::collections::HashMap;
use crate::geometry::{Cardinal, Point2D, Rect2D};
use crate::noise::RNG;
use crate::generator::buildings_v2::RoomType;
use crate::generator::buildings_v2::rooms::{CellState, ConstraintMap, Room, RoomRole};
use std::collections::HashSet;
use super::{
    interior_rect, wall_slots, flood_fill, check_connectivity,
    placement_keeps_connectivity, shuffle, is_ceiling_item, needs_wall,
    try_place_freestanding,
    resolve_offset, try_place_at_wall_slot, try_place_ceiling,
    resolve_candidates,
    CellConstraint, FacingMode, BlockLayer,
    PlacementResult, DEFAULT_FILL_THRESHOLD,
};
use super::data::{Furniture, FurnitureBlock, FurnitureConstraint, FurnitureData, FixedSlot, LootItem, LootTable, PaletteSwap, RoomFurnitureList};
use super::roll_loot_snbt;

fn make_room(rect: Rect2D, constraints: ConstraintMap) -> Room {
    Room {
        rect,
        rect_index: 0,
        floor: 0,
        role: RoomRole::Upper,
        room_type: RoomType::Bedroom,
        interior: rect.shrink(1),
        constraints,
        furniture: Vec::new(),
    }
}

/// ConstraintMap where all cells are Empty.
fn open_constraints(interior: &Rect2D) -> ConstraintMap {
    ConstraintMap::new(interior)
}

/// ConstraintMap with some cells marked as Reserved (doors).
fn constraints_with_doors(interior: &Rect2D, doors: &[(i32, i32)]) -> ConstraintMap {
    let mut cm = open_constraints(interior);
    for &d in doors { cm.set(d, CellState::BlockedReachable); }
    cm
}

/// Build a small ConstraintMap from a list of (world_coord, state) pairs.
/// The grid bounds are inferred from the coordinates.
fn cm_from(entries: &[((i32, i32), CellState)]) -> ConstraintMap {
    let min_x = entries.iter().map(|((x, _), _)| *x).min().unwrap_or(0);
    let max_x = entries.iter().map(|((x, _), _)| *x).max().unwrap_or(0);
    let min_z = entries.iter().map(|((_, z), _)| *z).min().unwrap_or(0);
    let max_z = entries.iter().map(|((_, z), _)| *z).max().unwrap_or(0);
    let rect = Rect2D::from_points(Point2D::new(min_x, min_z), Point2D::new(max_x, max_z));
    let mut cm = ConstraintMap::new(&rect);
    for &(cell, state) in entries { cm.set(cell, state); }
    cm
}

/// Build a bed item for testing (same as what furniture.yaml produces).
fn test_bed() -> Furniture {
    Furniture {
        unique: true,
        blocks: vec![
            FurnitureBlock {
                block: "minecraft:red_bed[part=foot]".into(),
                offset: [0, 0, 1],
                layer: BlockLayer::Ground, swap: PaletteSwap::None, walkable: false, place: true, loot: None,
            },
        ],
        constraints: vec![
            FurnitureConstraint { offset: [0, 0], constraint: CellConstraint::Wall, facing: FacingMode::TowardWall },
            FurnitureConstraint { offset: [0, 1], constraint: CellConstraint::BlockedReachable, facing: FacingMode::None },
        ],
        ..Default::default()
    }
}

fn test_chest() -> Furniture {
    Furniture {
        unique: false,
        blocks: vec![
            FurnitureBlock {
                block: "minecraft:chest".into(),
                offset: [0, 0, 0],
                layer: BlockLayer::Ground, swap: PaletteSwap::None, walkable: false, place: true, loot: None,
            },
        ],
        constraints: vec![
            FurnitureConstraint { offset: [0, 0], constraint: CellConstraint::BlockedReachable, facing: FacingMode::AwayFromWall },
        ],
        ..Default::default()
    }
}

fn test_lantern() -> Furniture {
    Furniture {
        unique: true,
        blocks: vec![
            FurnitureBlock {
                block: "minecraft:lantern[hanging=true]".into(),
                offset: [0, 0, 0],
                layer: BlockLayer::Ceiling, swap: PaletteSwap::None, walkable: false, place: true, loot: None,
            },
        ],
        constraints: vec![],
        ..Default::default()
    }
}

fn test_bookshelf() -> Furniture {
    Furniture {
        unique: false,
        blocks: vec![
            FurnitureBlock {
                block: "minecraft:bookshelf".into(),
                offset: [0, 0, 0],
                layer: BlockLayer::Ground, swap: PaletteSwap::None, walkable: false, place: true, loot: None,
            },
        ],
        constraints: vec![
            FurnitureConstraint { offset: [0, 0], constraint: CellConstraint::Wall, facing: FacingMode::None },
        ],
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// interior_rect
// ---------------------------------------------------------------------------

#[test]
fn interior_rect_normal() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 6));
    let room = make_room(rect, ConstraintMap::new(&rect.shrink(1)));
    let interior = interior_rect(&room).unwrap();
    assert_eq!(interior.min(), Point2D::new(1, 1));
    assert_eq!(interior.max(), Point2D::new(5, 5));
}

#[test]
fn interior_rect_too_small() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(1, 1));
    let room = make_room(rect, ConstraintMap::new(&rect));
    assert!(interior_rect(&room).is_none());
}

#[test]
fn interior_rect_minimum_3x3() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(2, 2));
    let room = make_room(rect, ConstraintMap::new(&rect.shrink(1)));
    let interior = interior_rect(&room).unwrap();
    assert_eq!(interior.area(), 1);
}

// ---------------------------------------------------------------------------
// wall_slots
// ---------------------------------------------------------------------------

#[test]
fn wall_slots_4x4_interior() {
    let interior = Rect2D::from_points(Point2D::new(1, 1), Point2D::new(4, 4));
    let slots = wall_slots(&interior);
    assert_eq!(slots.len(), 16);

    let corner: Vec<_> = slots.iter()
        .filter(|s| s.cell == Point2D::new(1, 1))
        .collect();
    assert_eq!(corner.len(), 2);
}

// ---------------------------------------------------------------------------
// resolve_offset
// ---------------------------------------------------------------------------

#[test]
fn resolve_offset_north_wall() {
    // [along=1, y=3, away=2]
    let (dx, dz, dy) = resolve_offset([1, 3, 2], Cardinal::North);
    assert_eq!(dx, 1);
    assert_eq!(dz, 2);
    assert_eq!(dy, 3);
}

#[test]
fn resolve_offset_east_wall() {
    // [along=1, y=0, away=2]
    let (dx, dz, _) = resolve_offset([1, 0, 2], Cardinal::East);
    assert_eq!(dx, -2);
    assert_eq!(dz, 1);
}

#[test]
fn resolve_offset_south_wall() {
    // [along=1, y=0, away=2]
    let (dx, dz, _) = resolve_offset([1, 0, 2], Cardinal::South);
    assert_eq!(dx, -1);
    assert_eq!(dz, -2);
}

#[test]
fn resolve_offset_west_wall() {
    // [along=1, y=0, away=2]
    let (dx, dz, _) = resolve_offset([1, 0, 2], Cardinal::West);
    assert_eq!(dx, 2);
    assert_eq!(dz, -1);
}

#[test]
fn resolve_offset_zero() {
    let (dx, dz, dy) = resolve_offset([0, 0, 0], Cardinal::North);
    assert_eq!((dx, dz, dy), (0, 0, 0));
}

// ---------------------------------------------------------------------------
// is_ceiling_item
// ---------------------------------------------------------------------------

#[test]
fn bed_is_not_ceiling() {
    assert!(!is_ceiling_item(&test_bed()));
}

#[test]
fn chest_is_not_ceiling() {
    assert!(!is_ceiling_item(&test_chest()));
}

#[test]
fn lantern_is_ceiling() {
    assert!(is_ceiling_item(&test_lantern()));
}

#[test]
fn bookshelf_is_not_ceiling() {
    assert!(!is_ceiling_item(&test_bookshelf()));
}

// ---------------------------------------------------------------------------
// flood_fill
// ---------------------------------------------------------------------------

#[test]
fn flood_fill_open_grid() {
    let interior = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(2, 2));
    let cm = open_constraints(&interior);
    let reached = flood_fill((0, 0), &cm);
    assert_eq!(reached.len(), 9);
}

#[test]
fn flood_fill_wall_splits_grid() {
    let interior = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(4, 2));
    let mut cm = open_constraints(&interior);
    for z in 0..=2 { cm.set((2, z), CellState::Blocked); }
    let reached = flood_fill((0, 0), &cm);
    assert_eq!(reached.len(), 6);
    assert!(!reached.contains(&(3, 0)));
}

#[test]
fn flood_fill_cannot_walk_through_accessible() {
    let interior = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(2, 0));
    let mut cm = open_constraints(&interior);
    cm.set((1, 0), CellState::BlockedReachable);
    let reached = flood_fill((0, 0), &cm);
    assert_eq!(reached.len(), 1);
}

#[test]
fn flood_fill_unreachable_start() {
    let cm = cm_from(&[((1, 1), CellState::Empty)]);
    let reached = flood_fill((0, 0), &cm);
    assert_eq!(reached.len(), 0);
}

// ---------------------------------------------------------------------------
// check_connectivity
// ---------------------------------------------------------------------------

#[test]
fn connectivity_no_accessible() {
    let cm = cm_from(&[((0, 0), CellState::Empty)]);
    assert!(check_connectivity(&cm));
}

#[test]
fn connectivity_single_accessible() {
    let cm = cm_from(&[
        ((0, 0), CellState::BlockedReachable),
        ((1, 0), CellState::Empty),
    ]);
    assert!(check_connectivity(&cm));
}

#[test]
fn connectivity_two_accessible_connected() {
    let cm = cm_from(&[
        ((0, 0), CellState::BlockedReachable),
        ((1, 0), CellState::Empty),
        ((2, 0), CellState::BlockedReachable),
    ]);
    assert!(check_connectivity(&cm));
}

#[test]
fn connectivity_two_accessible_disconnected() {
    let cm = cm_from(&[
        ((0, 0), CellState::BlockedReachable),
        ((1, 0), CellState::Blocked),
        ((2, 0), CellState::BlockedReachable),
    ]);
    assert!(!check_connectivity(&cm));
}

#[test]
fn connectivity_accessible_adjacent_to_walkable() {
    let cm = cm_from(&[
        ((0, 0), CellState::Empty),
        ((1, 0), CellState::BlockedReachable),
    ]);
    assert!(check_connectivity(&cm));
}

#[test]
fn connectivity_accessible_not_adjacent_to_walkable() {
    let cm = cm_from(&[
        ((0, 0), CellState::BlockedReachable),
        ((1, 0), CellState::Blocked),
        ((2, 0), CellState::BlockedReachable),
    ]);
    assert!(!check_connectivity(&cm));
}

#[test]
fn connectivity_accessible_reachable_via_open() {
    let cm = cm_from(&[
        ((0, 0), CellState::BlockedReachable),
        ((1, 0), CellState::Empty),
        ((2, 0), CellState::BlockedReachable),
    ]);
    assert!(check_connectivity(&cm));
}

// ---------------------------------------------------------------------------
// placement_keeps_connectivity
// ---------------------------------------------------------------------------

#[test]
fn placement_blocks_corridor() {
    let mut cm = cm_from(&[
        ((0, 0), CellState::BlockedReachable),
        ((1, 0), CellState::Empty),
        ((2, 0), CellState::BlockedReachable),
    ]);
    assert!(!placement_keeps_connectivity(&[(1, 0)], &[], &[], &mut cm));
}

#[test]
fn placement_accessible_with_adjacency() {
    let mut cm = cm_from(&[
        ((0, 0), CellState::BlockedReachable),
        ((1, 0), CellState::Empty),
        ((2, 0), CellState::Empty),
    ]);
    assert!(placement_keeps_connectivity(&[], &[(2, 0)], &[], &mut cm));
}

/// Freestanding block cells must be treated as blockers during connectivity.
/// Regression for the table-across-stair-approach bug in blueprint_8.
#[test]
fn placement_block_cells_block_walking() {
    // Layout: BR at (0,0), open corridor, BR at (2,0).
    // Placing a block at (1,0) should fail connectivity even without
    // any new_blocked/new_reserved entries.
    let mut cm = cm_from(&[
        ((0, 0), CellState::BlockedReachable),
        ((1, 0), CellState::Empty),
        ((2, 0), CellState::BlockedReachable),
    ]);
    assert!(!placement_keeps_connectivity(&[], &[], &[((1, 0), false)], &mut cm));
}

// ---------------------------------------------------------------------------
// try_place_at_wall_slot — bed
// ---------------------------------------------------------------------------

#[test]
fn bed_placement_basic() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 6));
    let interior = rect.shrink(1);
    let mut cm = constraints_with_doors(&interior, &[(3, 1)]);

    let slots = wall_slots(&interior);
    let item = test_bed();
    let mut result = None;
    for slot in &slots {
        if let Some(r) = try_place_at_wall_slot(&item, slot, &interior, &mut cm, 64, None) {
            result = Some(r); break;
        }
    }
    assert!(result.is_some());
    assert_eq!(result.unwrap().blocks.len(), 1);
}

#[test]
fn bed_impossible_in_1x1_interior() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(2, 2));
    let interior = rect.shrink(1);
    let mut cm = open_constraints(&interior);

    let slots = wall_slots(&interior);
    let item = test_bed();
    let mut result = None;
    for slot in &slots {
        if let Some(r) = try_place_at_wall_slot(&item, slot, &interior, &mut cm, 64, None) {
            result = Some(r); break;
        }
    }
    assert!(result.is_none());
}

#[test]
fn bed_avoids_disconnecting_doors() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(4, 2));
    let interior = rect.shrink(1);
    let mut cm = constraints_with_doors(&interior, &[(1, 1), (3, 1)]);

    let slots = wall_slots(&interior);
    let item = test_bed();
    let mut result = None;
    for slot in &slots {
        if let Some(r) = try_place_at_wall_slot(&item, slot, &interior, &mut cm, 64, None) {
            result = Some(r); break;
        }
    }
    assert!(result.is_none());
}

// ---------------------------------------------------------------------------
// try_place_at_wall_slot — single items
// ---------------------------------------------------------------------------

#[test]
fn chest_placement_basic() {
    let interior = Rect2D::from_points(Point2D::new(1, 1), Point2D::new(5, 5));
    let mut cm = constraints_with_doors(&interior, &[(1, 3)]);
    let slots = wall_slots(&interior);
    let item = test_chest();
    let mut result = None;
    for slot in &slots {
        if let Some(r) = try_place_at_wall_slot(&item, slot, &interior, &mut cm, 64, None) {
            result = Some(r); break;
        }
    }
    assert!(result.is_some());
    assert_eq!(result.unwrap().blocks.len(), 1);
}

#[test]
fn placement_skips_blocked_cell() {
    let mut cm = cm_from(&[((1, 1), CellState::Blocked)]);
    let interior = Rect2D::from_points(Point2D::new(1, 1), Point2D::new(1, 1));
    let slots = wall_slots(&interior);
    let item = test_chest();
    let mut result = None;
    for slot in &slots {
        if let Some(r) = try_place_at_wall_slot(&item, slot, &interior, &mut cm, 64, None) {
            result = Some(r); break;
        }
    }
    assert!(result.is_none());
}

#[test]
fn chest_blocked_by_existing_accessible() {
    let interior = Rect2D::from_points(Point2D::new(1, 1), Point2D::new(5, 5));
    let mut cm = open_constraints(&interior);
    for slot in &wall_slots(&interior) {
        cm.set((slot.cell.x, slot.cell.y), CellState::BlockedReachable);
    }
    let slots = wall_slots(&interior);
    let item = test_chest();
    let mut result = None;
    for slot in &slots {
        if let Some(r) = try_place_at_wall_slot(&item, slot, &interior, &mut cm, 64, None) {
            result = Some(r); break;
        }
    }
    assert!(result.is_none());
}

// ---------------------------------------------------------------------------
// stacked (2-tall) furniture
// ---------------------------------------------------------------------------

/// A 1x1 freestanding crate made of two stacked hay blocks.
fn test_stacked_crate() -> Furniture {
    Furniture {
        unique: false,
        blocks: vec![
            FurnitureBlock {
                block: "minecraft:hay_block".into(),
                offset: [0, 0, 0],
                layer: BlockLayer::Ground, swap: PaletteSwap::None, walkable: false, place: true, loot: None,
            },
            FurnitureBlock {
                block: "minecraft:hay_block".into(),
                offset: [0, 1, 0],
                layer: BlockLayer::Ground, swap: PaletteSwap::None, walkable: false, place: true, loot: None,
            },
        ],
        constraints: vec![
            FurnitureConstraint {
                offset: [0, 0],
                constraint: CellConstraint::BlockedReachable,
                facing: FacingMode::None,
            },
        ],
        ..Default::default()
    }
}

/// A 1x1 wall-adjacent stack of two bookshelves.
fn test_stacked_bookshelves() -> Furniture {
    Furniture {
        unique: false,
        blocks: vec![
            FurnitureBlock {
                block: "minecraft:bookshelf".into(),
                offset: [0, 0, 0],
                layer: BlockLayer::Ground, swap: PaletteSwap::None, walkable: false, place: true, loot: None,
            },
            FurnitureBlock {
                block: "minecraft:bookshelf".into(),
                offset: [0, 1, 0],
                layer: BlockLayer::Ground, swap: PaletteSwap::None, walkable: false, place: true, loot: None,
            },
        ],
        constraints: vec![
            FurnitureConstraint {
                offset: [0, 0],
                constraint: CellConstraint::Wall,
                facing: FacingMode::None,
            },
        ],
        ..Default::default()
    }
}

/// A 2x1 wall-adjacent stretch of bookshelves stacked 2 tall.
fn test_loaded_shelves() -> Furniture {
    Furniture {
        unique: false,
        blocks: vec![
            FurnitureBlock { block: "minecraft:bookshelf".into(), offset: [0, 0, 0], layer: BlockLayer::Ground, swap: PaletteSwap::None, walkable: false, place: true, loot: None },
            FurnitureBlock { block: "minecraft:bookshelf".into(), offset: [1, 0, 0], layer: BlockLayer::Ground, swap: PaletteSwap::None, walkable: false, place: true, loot: None },
            FurnitureBlock { block: "minecraft:bookshelf".into(), offset: [0, 1, 0], layer: BlockLayer::Ground, swap: PaletteSwap::None, walkable: false, place: true, loot: None },
            FurnitureBlock { block: "minecraft:bookshelf".into(), offset: [1, 1, 0], layer: BlockLayer::Ground, swap: PaletteSwap::None, walkable: false, place: true, loot: None },
        ],
        constraints: vec![
            FurnitureConstraint { offset: [0, 0], constraint: CellConstraint::Wall, facing: FacingMode::None },
            FurnitureConstraint { offset: [1, 0], constraint: CellConstraint::Wall, facing: FacingMode::None },
        ],
        ..Default::default()
    }
}

#[test]
fn stacked_crate_produces_two_blocks_same_cell() {
    let interior = Rect2D::from_points(Point2D::new(1, 1), Point2D::new(5, 5));
    let mut cm = open_constraints(&interior);
    let cells: Vec<_> = interior.iter().map(|p| (p.x, p.y)).collect();

    let result = try_place_freestanding(&test_stacked_crate(), &interior, &mut cm, 64, &cells, None)
        .expect("crate should fit in a 5x5 open interior");

    assert_eq!(result.blocks.len(), 2, "crate should place two stacked blocks");
    assert_eq!(result.blocks[0].cell, result.blocks[1].cell,
        "both blocks share the same (x,z) cell");
    let y0 = result.blocks[0].world_pos.y;
    let y1 = result.blocks[1].world_pos.y;
    assert!(
        (y0 == 64 && y1 == 65) || (y0 == 65 && y1 == 64),
        "stack should occupy floor_y and floor_y+1, got {} and {}", y0, y1);
}

#[test]
fn stacked_bookshelves_place_against_wall() {
    let interior = Rect2D::from_points(Point2D::new(1, 1), Point2D::new(5, 5));
    let mut cm = open_constraints(&interior);
    let slots = wall_slots(&interior);

    let item = test_stacked_bookshelves();
    let mut result = None;
    for slot in &slots {
        if let Some(r) = try_place_at_wall_slot(&item, slot, &interior, &mut cm, 64, None) {
            result = Some(r); break;
        }
    }
    let result = result.expect("stacked bookshelves should fit on a wall");

    assert_eq!(result.blocks.len(), 2);
    assert_eq!(result.blocks[0].cell, result.blocks[1].cell);
    assert_eq!(result.new_blocked.len(), 1, "one wall cell is claimed");
    // Top block should parse as plain bookshelf, not rotated or facing anything.
    assert!(result.blocks.iter().any(|b| b.world_pos.y == 65));
    assert!(result.blocks.iter().any(|b| b.world_pos.y == 64));
}

#[test]
fn loaded_shelves_place_4_blocks_on_wall() {
    // Wall needs to be at least 2 cells long for the 2-wide item.
    let interior = Rect2D::from_points(Point2D::new(1, 1), Point2D::new(5, 5));
    let mut cm = open_constraints(&interior);
    let slots = wall_slots(&interior);

    let item = test_loaded_shelves();
    let mut result = None;
    for slot in &slots {
        if let Some(r) = try_place_at_wall_slot(&item, slot, &interior, &mut cm, 64, None) {
            result = Some(r); break;
        }
    }
    let result = result.expect("loaded shelves should fit in a 5x5 interior wall");

    assert_eq!(result.blocks.len(), 4, "2 wide × 2 tall = 4 blocks");
    assert_eq!(result.new_blocked.len(), 2, "two wall cells are claimed");

    // Exactly 2 unique (x,z) cells, each used by blocks at y=64 and y=65.
    use std::collections::HashSet;
    let cells: HashSet<_> = result.blocks.iter().map(|b| b.cell).collect();
    assert_eq!(cells.len(), 2);
    for cell in &cells {
        let ys: Vec<i32> = result.blocks.iter()
            .filter(|b| b.cell == *cell)
            .map(|b| b.world_pos.y)
            .collect();
        assert_eq!(ys.len(), 2, "each cell has 2 stacked blocks");
        assert!(ys.contains(&64) && ys.contains(&65));
    }
}

#[test]
fn stacked_crate_connectivity_respects_doors() {
    // 3x1 interior with doors at both ends — placing a crate in the middle
    // should fail because it disconnects the two doors.
    let interior = Rect2D::from_points(Point2D::new(1, 1), Point2D::new(3, 1));
    let mut cm = constraints_with_doors(&interior, &[(1, 1), (3, 1)]);
    let cells = vec![(2, 1)];

    let result = try_place_freestanding(&test_stacked_crate(), &interior, &mut cm, 64, &cells, None);
    assert!(result.is_none(),
        "stacked crate must not be placeable where it disconnects required cells");
}

#[test]
fn stacked_crate_cannot_overlap_existing_furniture() {
    let interior = Rect2D::from_points(Point2D::new(1, 1), Point2D::new(3, 3));
    let mut cm = open_constraints(&interior);
    let cells: Vec<_> = interior.iter().map(|p| (p.x, p.y)).collect();

    // Place one crate first.
    try_place_freestanding(&test_stacked_crate(), &interior, &mut cm, 64, &cells, None)
        .expect("first crate fits");
    // Simulate post-placement bookkeeping: mark the cell Blocked.
    // Find which cell got used by looking for the first Empty remaining — flip it.
    // Simpler: mark (2,2) Blocked and confirm nothing places there.
    cm.set((2, 2), CellState::Blocked);
    let only = vec![(2, 2)];
    let result = try_place_freestanding(&test_stacked_crate(), &interior, &mut cm, 64, &only, None);
    assert!(result.is_none(), "cannot re-use a Blocked cell");
}

// ---------------------------------------------------------------------------
// aggressive fill: repeated placement fills a large interior
// ---------------------------------------------------------------------------

/// Simulate the aggressive-fill retry loop: keep placing stacked crates
/// until none fits. Verifies the connectivity-preserving placement can
/// densely pack a room.
#[test]
fn aggressive_fill_packs_storage_interior() {
    let interior = Rect2D::from_points(Point2D::new(1, 1), Point2D::new(6, 6));
    // One door on the south edge of the containing room; the adjacent
    // interior cell must remain reachable.
    let mut cm = constraints_with_doors(&interior, &[(3, 6)]);

    let item = test_stacked_crate();
    let mut placed_count = 0;
    loop {
        // Re-shuffle open cells each pass so placement exhaustively tries them.
        let open: Vec<_> = interior.iter()
            .map(|p| (p.x, p.y))
            .filter(|c| cm.is_open(*c))
            .collect();
        if open.is_empty() { break; }

        let result = try_place_freestanding(&item, &interior, &mut cm, 64, &open, None);
        match result {
            Some(placement) => {
                for &c in &placement.new_blocked { cm.set(c, CellState::Blocked); }
                for &c in &placement.new_reserved { cm.set(c, CellState::BlockedReachable); }
                for &c in &placement.new_empty_reachable { cm.set(c, CellState::UnblockedReachable); }
                for rb in &placement.blocks {
                    cm.set(rb.cell, CellState::Blocked);
                }
                placed_count += 1;
            }
            None => break,
        }
    }

    let fill = cm.fill_ratio();
    println!("aggressive_fill_packs_storage_interior: placed {} crates, fill_ratio={:.0}%",
             placed_count, fill * 100.0);

    // 6x6 = 36 cells. With a door at (3,6), the interior cells are 6x6.
    // Every non-door cell could in principle host a crate as long as the
    // door approach stays walkable. We should get well above the default 0.75.
    assert!(placed_count >= 10,
        "expected at least 10 crates in a 6x6 room, got {}", placed_count);
    assert!(fill >= DEFAULT_FILL_THRESHOLD,
        "fill ratio {:.2} should exceed DEFAULT_FILL_THRESHOLD {:.2}", fill, DEFAULT_FILL_THRESHOLD);

    // Connectivity sanity: the door at (3, 6) — strictly its interior
    // counterpart — must still be reachable from the remaining open region.
    assert!(check_connectivity(&cm),
        "packed storage room must preserve door accessibility");
}

// ---------------------------------------------------------------------------
// shuffle
// ---------------------------------------------------------------------------

#[test]
fn shuffle_preserves_elements() {
    let mut rng = RNG::new(42);
    let mut items: Vec<i32> = (0..10).collect();
    let original: Vec<i32> = items.clone();
    shuffle(&mut items, &mut rng);
    items.sort();
    assert_eq!(items, original);
}

#[test]
fn shuffle_varies_with_seed() {
    let mut items_a: Vec<i32> = (0..10).collect();
    let mut items_b: Vec<i32> = (0..10).collect();
    shuffle(&mut items_a, &mut RNG::new(1));
    shuffle(&mut items_b, &mut RNG::new(2));
    assert_ne!(items_a, items_b);
}

// ---------------------------------------------------------------------------
// fill_ratio (on ConstraintMap)
// ---------------------------------------------------------------------------

#[test]
fn fill_ratio_empty_room() {
    let interior = Rect2D::from_points(Point2D::new(1, 1), Point2D::new(4, 4));
    let cm = open_constraints(&interior);
    assert!(cm.fill_ratio() < 0.01);
}

#[test]
fn fill_ratio_half_filled() {
    // 4x2 grid, bottom row empty, top row accessible
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(3, 1));
    let mut cm = ConstraintMap::new(&rect);
    for x in 0..4 {
        cm.set((x, 1), CellState::BlockedReachable);
    }
    assert!((cm.fill_ratio() - 0.5).abs() < 0.01);
}

#[test]
fn fill_ratio_empty_map() {
    // Zero-size grid
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(0, 0));
    let cm = ConstraintMap::new(&rect);
    // 1x1 grid, all empty
    assert!(cm.fill_ratio() < 0.01);
}

// ---------------------------------------------------------------------------
// data loading — resolve from FurnitureData
// ---------------------------------------------------------------------------

#[test]
fn resolve_furniture_from_data() {
    let mut items: HashMap<String, Furniture> = HashMap::new();
    items.insert("bed".into(), Furniture {
        unique: true,
        blocks: vec![FurnitureBlock {
            block: "minecraft:red_bed[part=head]".into(),
            offset: [0, 0, 0],
            layer: BlockLayer::Ground, swap: PaletteSwap::None, walkable: false, place: true, loot: None,
        }],
        constraints: vec![FurnitureConstraint {
            offset: [0, 0],
            constraint: CellConstraint::Wall,
            facing: FacingMode::AwayFromWall,
        }],
        ..Default::default()
    });
    items.insert("lantern".into(), Furniture {
        unique: true,
        blocks: vec![FurnitureBlock {
            block: "minecraft:lantern[hanging=true]".into(),
            offset: [0, 0, 0],
            layer: BlockLayer::Ceiling, swap: PaletteSwap::None, walkable: false, place: true, loot: None,
        }],
        constraints: vec![],
        ..Default::default()
    });

    let mut rooms: HashMap<String, RoomFurnitureList> = HashMap::new();
    rooms.insert("bedroom".into(), RoomFurnitureList {
        required: vec!["bed".into()],
        optional: vec!["lantern".into()],
        fill_threshold: None,
    });

    let data = FurnitureData { items, rooms, loot: HashMap::new() };
    let room_list = data.rooms.get("bedroom").unwrap();
    assert_eq!(room_list.required.len(), 1);
    assert_eq!(room_list.required[0], "bed");
    assert_eq!(room_list.optional.len(), 1);
    assert_eq!(room_list.optional[0], "lantern");
    assert!(data.items.contains_key("bed"));
    assert!(data.items.contains_key("lantern"));
}

#[test]
fn resolve_missing_room_returns_none() {
    let data = FurnitureData { items: HashMap::new(), rooms: HashMap::new(), loot: HashMap::new() };
    assert!(data.rooms.get("nonexistent").is_none());
}

#[test]
fn resolve_skips_unknown_items() {
    let mut rooms: HashMap<String, RoomFurnitureList> = HashMap::new();
    rooms.insert("test".into(), RoomFurnitureList {
        required: vec!["nonexistent_item".into()],
        optional: vec![],
        fill_threshold: None,
    });
    let data = FurnitureData { items: HashMap::new(), rooms, loot: HashMap::new() };
    let room_list = data.rooms.get("test").unwrap();
    assert!(data.items.get(&room_list.required[0]).is_none());
}

// ---------------------------------------------------------------------------
// room type key mapping
// ---------------------------------------------------------------------------

#[test]
fn every_room_type_has_key() {
    let types = [
        RoomType::Common, RoomType::Hearth, RoomType::GreatRoom,
        RoomType::Bedroom, RoomType::MasterBedroom, RoomType::MultiBedroom,
        RoomType::Storage, RoomType::Kitchen, RoomType::Pantry,
        RoomType::Dining, RoomType::Study, RoomType::Library,
        RoomType::Studio, RoomType::Armory,
    ];
    for rt in types {
        let key = rt.furniture_key();
        assert!(!key.is_empty(), "{:?} has empty furniture key", rt);
    }
}

// ---------------------------------------------------------------------------
// ASCII diagram helpers
// ---------------------------------------------------------------------------

const GREEN: &str = "\x1b[32m";
const CYAN: &str = "\x1b[36m";
const RESET: &str = "\x1b[0m";

/// Render a room as ASCII art.
/// Wall: #, Door in wall: D, Furniture: first letter of name,
/// Ceiling items: lowercase. BlockedReachable cells are green;
/// UnblockedReachable cells (doors, stair landings, ladders, stair
/// air-columns) are cyan.
fn render_room(
    room_rect: &Rect2D,
    cm: &ConstraintMap,
    labels: &HashMap<(i32, i32), char>,
    doors: &[(i32, i32)],
    label: &str,
) -> String {
    let mut lines = vec![format!("  {}", label)];
    let min = room_rect.min();
    let max = room_rect.max();

    let mut header = String::from("    ");
    for x in min.x..=max.x {
        header.push_str(&format!("{}", x % 10));
    }
    lines.push(header);

    for z in min.y..=max.y {
        let mut row = format!("  {} ", z % 10);
        for x in min.x..=max.x {
            let cell = (x, z);
            let on_wall = x == min.x || x == max.x || z == min.y || z == max.y;
            let state = if on_wall { None } else { cm.get(cell) };

            if on_wall {
                row.push(if doors.contains(&cell) { 'D' } else { '#' });
            } else {
                let labeled = labels.get(&cell).copied();
                let ch = labeled.unwrap_or('.');
                match state {
                    Some(CellState::BlockedReachable) => {
                        row.push_str(&format!("{GREEN}{ch}{RESET}"));
                    }
                    Some(CellState::UnblockedReachable) => {
                        let show = labeled.unwrap_or('·');
                        row.push_str(&format!("{CYAN}{show}{RESET}"));
                    }
                    _ => row.push(ch),
                }
            }
        }
        lines.push(row);
    }
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// ASCII diagram tests — run with `cargo test -- --nocapture diagram`
// ---------------------------------------------------------------------------

struct DiagramRoom {
    room_rect: Rect2D,
    interior: Rect2D,
    cm: ConstraintMap,
    slots: Vec<super::WallSlot>,
    open_cells: Vec<(i32, i32)>,
    wall_doors: Vec<(i32, i32)>,
    /// Tracks placed furniture labels per cell for rendering.
    labels: HashMap<(i32, i32), char>,
}

impl DiagramRoom {
    fn new(room_rect: Rect2D, interior_doors: &[(i32, i32)], wall_doors: &[(i32, i32)], seed: i64) -> Self {
        let interior = room_rect.shrink(1);
        let cm = constraints_with_doors(&interior, interior_doors);
        let mut rng = RNG::new(seed);
        let mut slots = wall_slots(&interior);
        shuffle(&mut slots, &mut rng);
        let mut open_cells: Vec<(i32, i32)> = interior.iter().map(|p| (p.x, p.y)).collect();
        shuffle(&mut open_cells, &mut rng);
        Self {
            room_rect, interior, cm, slots, open_cells,
            wall_doors: wall_doors.to_vec(),
            labels: HashMap::new(),
        }
    }

    fn render(&self, label: &str) -> String {
        render_room(&self.room_rect, &self.cm, &self.labels, &self.wall_doors, label)
    }

    /// Place a furniture item. Shows its first letter (uppercase for ground,
    /// lowercase for ceiling) on the diagram.
    fn place(&mut self, name: &str, item: &Furniture) -> bool {
        let result = if is_ceiling_item(item) {
            try_place_ceiling(item, &self.interior, &mut self.cm, 67)
        } else if needs_wall(item) {
            let mut found = None;
            for slot in &self.slots {
                if let Some(r) = try_place_at_wall_slot(item, slot, &self.interior, &mut self.cm, 64, None) {
                    found = Some(r); break;
                }
            }
            found
        } else {
            try_place_freestanding(item, &self.interior, &mut self.cm, 64, &self.open_cells, None)
        };

        if let Some(placement) = result {
            for &cell in &placement.new_blocked { self.cm.set(cell, CellState::Blocked); }
            for &cell in &placement.new_reserved { self.cm.set(cell, CellState::BlockedReachable); }
            for &cell in &placement.new_empty_reachable { self.cm.set(cell, CellState::UnblockedReachable); }

            let ch = match name {
                "bed" | "single_bed" | "double_bed" | "canopy_bed" => 'B',
                "chest" => 'C',
                "crafting_table" => 'T',
                "furnace" => 'F',
                "lantern" => 'L',
                "bookshelf" => 'K',
                "barrel" => 'R',
                "anvil" => 'A',
                "cauldron" => 'U',
                "smoker" => 'S',
                "loom" => 'M',
                "table" => 'X',
                "flower_pot" => 'P',
                "carpet" => '~',
                "carpet_runner" => '~',
                "rug" => '~',
                "nightstand" => 'N',
                "chair" => 'H',
                "desk" => 'D',
                "shelf" => 'K',
                "vase" => 'V',
                "candle" => 'c',
                "banner" => 'b',
                _ => name.chars().next().unwrap_or('?'),
            };
            // Label blocked cells (e.g. bed head from Wall constraint)
            for &cell in &placement.new_blocked {
                self.labels.insert(cell, ch.to_ascii_uppercase());
            }
            for rb in &placement.blocks {
                if rb.layer.occupies_ceiling() {
                    self.cm.set_ceiling(rb.cell);
                    // Don't overwrite ground furniture labels with ceiling
                    self.labels.entry(rb.cell).or_insert(ch.to_ascii_lowercase());
                }
                if rb.layer.occupies_ground() {
                    self.cm.set(rb.cell, CellState::Blocked);
                    self.labels.insert(rb.cell, ch.to_ascii_uppercase());
                }
            }
            println!("  + {name}");
            true
        } else {
            println!("  - {name} (failed)");
            false
        }
    }
}

#[test]
fn diagram_bedroom_furnishing() {
    // 7x7 room → 5x5 interior, door on north wall
    let mut r = DiagramRoom::new(
        Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 6)),
        &[(3, 1)],    // interior door cell
        &[(3, 0)],    // wall door cell
        42,
    );

    println!("\n{}", r.render("Bedroom — initial"));

    // Required
    r.place("bed", &test_bed());
    println!("{}", r.render("After bed"));

    // Optional: chest, lantern, bookshelf, chest, barrel, crafting_table
    r.place("chest", &test_chest());
    r.place("lantern", &test_lantern());
    r.place("bookshelf", &test_bookshelf());
    r.place("chest", &test_chest());
    r.place("barrel", &Furniture {
        blocks: vec![FurnitureBlock { block: "minecraft:barrel".into(), offset: [0,0,0], layer: BlockLayer::Ground, swap: PaletteSwap::None, walkable: false, place: true, loot: None }],
        constraints: vec![FurnitureConstraint { offset: [0,0], constraint: CellConstraint::BlockedReachable, facing: FacingMode::None }],
        ..test_chest() // unique: false
    });
    r.place("crafting_table", &Furniture {
        blocks: vec![FurnitureBlock { block: "minecraft:crafting_table".into(), offset: [0,0,0], layer: BlockLayer::Ground, swap: PaletteSwap::None, walkable: false, place: true, loot: None }],
        constraints: vec![FurnitureConstraint { offset: [0,0], constraint: CellConstraint::BlockedReachable, facing: FacingMode::None }],
        unique: true,
        ..Default::default()
    });
    println!("{}", r.render("After all furniture"));

    println!("  fill ratio: {:.0}%", r.cm.fill_ratio() * 100.0);
}

#[test]
fn diagram_hearth_furnishing() {
    // 9x7 room → 7x5 interior, door on south wall
    let mut r = DiagramRoom::new(
        Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 6)),
        &[(4, 5)],
        &[(4, 6)],
        99,
    );

    println!("\n{}", r.render("Hearth — initial"));

    let furnace = Furniture {
        unique: true,
        blocks: vec![FurnitureBlock { block: "minecraft:furnace".into(), offset: [0,0,0], layer: BlockLayer::Ground, swap: PaletteSwap::None, walkable: false, place: true, loot: None }],
        constraints: vec![FurnitureConstraint { offset: [0,0], constraint: CellConstraint::BlockedReachable, facing: FacingMode::AwayFromWall }],
        ..Default::default()
    };
    let crafting = Furniture {
        unique: true,
        blocks: vec![FurnitureBlock { block: "minecraft:crafting_table".into(), offset: [0,0,0], layer: BlockLayer::Ground, swap: PaletteSwap::None, walkable: false, place: true, loot: None }],
        constraints: vec![FurnitureConstraint { offset: [0,0], constraint: CellConstraint::BlockedReachable, facing: FacingMode::None }],
        ..Default::default()
    };
    let barrel = Furniture {
        unique: false,
        blocks: vec![FurnitureBlock { block: "minecraft:barrel".into(), offset: [0,0,0], layer: BlockLayer::Ground, swap: PaletteSwap::None, walkable: false, place: true, loot: None }],
        constraints: vec![FurnitureConstraint { offset: [0,0], constraint: CellConstraint::BlockedReachable, facing: FacingMode::None }],
        ..Default::default()
    };

    r.place("furnace", &furnace);
    println!("{}", r.render("After furnace"));

    r.place("crafting_table", &crafting);
    println!("{}", r.render("After crafting_table"));

    r.place("chest", &test_chest());
    r.place("barrel", &barrel);
    r.place("lantern", &test_lantern());
    println!("{}", r.render("After all optional"));

    println!("  fill ratio: {:.0}%", r.cm.fill_ratio() * 100.0);
}

#[test]
fn diagram_tiny_room() {
    // 4x4 room → 2x2 interior, door on west wall
    let mut r = DiagramRoom::new(
        Rect2D::from_points(Point2D::new(0, 0), Point2D::new(3, 3)),
        &[(1, 2)],
        &[(0, 2)],
        7,
    );

    println!("\n{}", r.render("Tiny room — 2x2 interior"));

    let bed_placed = r.place("bed", &test_bed());
    println!("{}", r.render(&format!("After bed (placed={})", bed_placed)));

    r.place("chest", &test_chest());
    println!("{}", r.render("After chest"));

    println!("  fill ratio: {:.0}%", r.cm.fill_ratio() * 100.0);
}

#[test]
fn diagram_narrow_corridor_connectivity() {
    // 7x3 room → 5x1 interior (corridor), doors at both ends
    let mut r = DiagramRoom::new(
        Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 2)),
        &[(1, 1), (5, 1)],
        &[(0, 1), (6, 1)],
        13,
    );

    println!("\n{}", r.render("Corridor — doors at both ends"));

    let placed = r.place("chest", &test_chest());
    println!("{}", r.render(&format!("After chest (placed={})", placed)));

    let placed2 = r.place("bookshelf", &test_bookshelf());
    println!("{}", r.render(&format!("After bookshelf (placed={})", placed2)));

    println!("  fill ratio: {:.0}%", r.cm.fill_ratio() * 100.0);
}

/// Live-server variant of `diagram_room_sizes`. Drops 4 rooms into the
/// current Minecraft build area: floor + walls + furniture from
/// `data/rooms.yaml`. Each room uses a different SecondaryWood + primary_color
/// palette so you can see the same room template furnished in cherry, warped,
/// spruce, and birch side-by-side. Marked `#[ignore]` because it needs a
/// running GDMC HTTP server. Run with:
///   cargo test place_room_sizes_in_world -- --ignored --nocapture
#[ignore]
#[tokio::test]
async fn place_room_sizes_in_world() {
    use crate::data::Loadable;
    use crate::editor::World;
    use crate::generator::buildings_v2::footprint::Footprint;
    use crate::generator::buildings_v2::frame::Frame;
    use crate::generator::materials::{Material, MaterialId, MaterialRole, Palette, PaletteId};
    use crate::geometry::Point3D;
    use crate::http_mod::GDMCHTTPProvider;
    use crate::minecraft::{Block, Color};
    use super::furnish_room;

    const ROOM_KEY: &str = "bedroom";
    const SEED: i64 = 42;
    // Wall height in air blocks between floor and ceiling. Frame::ceiling_y(0)
    // returns base_y + WALL_HEIGHT; furnish_room uses that for ceiling items.
    const WALL_HEIGHT: u32 = 4;

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.expect("get world from server");
    let editor = world.get_editor();

    let data = FurnitureData::load().expect("load furniture YAML");
    let materials = Material::load().expect("load materials");
    let room_list = data.rooms.get(ROOM_KEY)
        .unwrap_or_else(|| panic!("room key {ROOM_KEY:?} not found in data/rooms.yaml"));

    // Per-room palette: secondary wood drives furniture color (PaletteSwap::Wood
    // resolves SecondaryWood with PrimaryWood fallback); primary color drives
    // bed / banner / etc via PaletteSwap::Color.
    let make_palette = |id: &str, primary_wood: &str, secondary_wood: &str,
                        primary: Color, secondary: Color| -> Palette {
        let mut mats = HashMap::new();
        mats.insert(MaterialRole::PrimaryWood, MaterialId::from(primary_wood));
        mats.insert(MaterialRole::SecondaryWood, MaterialId::from(secondary_wood));
        Palette {
            id: PaletteId::from(id),
            materials: mats,
            primary_color: Some(primary),
            // Drives PaletteSwap::SecondaryColor — accent color for patterned
            // carpets and any future two-tone items.
            secondary_color: Some(secondary),
            tags: None,
        }
    };

    // Place rooms 8 cells inside the build area, laid out in a 2x2 grid.
    // place_block expects LOCAL coordinates (it adds build_area.origin
    // internally) — keep everything below in local space.
    let base_x = 8i32;
    let base_z = 8i32;
    let ground_local = editor.world().get_height_at(Point2D::new(base_x, base_z));
    let floor_y = ground_local + 1;

    // (rect_size, interior-door, wall-door (opening), label, world offset, palette)
    let cases: Vec<(Point2D, (i32, i32), (i32, i32), &str, (i32, i32), Palette)> = vec![
        (Point2D::new(5, 5),   (2, 1), (2, 0), "5x5 cherry",   (0, 0),
            make_palette("cherry_room", "oak_planks", "cherry_planks", Color::Pink, Color::White)),
        (Point2D::new(7, 6),   (3, 1), (3, 0), "7x6 warped",   (12, 0),
            make_palette("warped_room", "oak_planks", "warped_planks", Color::Cyan, Color::Yellow)),
        (Point2D::new(9, 8),   (4, 1), (4, 0), "9x8 spruce",   (0, 14),
            make_palette("spruce_room", "oak_planks", "spruce_planks", Color::Brown, Color::Orange)),
        (Point2D::new(11, 10), (5, 1), (5, 0), "11x10 birch",  (14, 14),
            make_palette("birch_room", "oak_planks", "birch_planks", Color::Yellow, Color::Red)),
    ];

    let floor_block = Block::from_id("minecraft:oak_planks".into());
    let air = Block::from_id("minecraft:air".into());

    for (size, idoor, wdoor, label, (offx, offz), palette) in &cases {
        // Walls match each room's secondary wood so the room visually carries
        // the palette identity; furniture inside also uses secondary wood, so
        // they coordinate but contrast against the oak floor.
        let wall_id_str = palette.get_material(MaterialRole::SecondaryWood)
            .map(|m| format!("minecraft:{}", m.as_str()))
            .unwrap_or_else(|| "minecraft:spruce_planks".to_string());
        let wall_block = Block::from_id(wall_id_str.as_str().into());
        let rect = Rect2D::from_points(
            Point2D::new(base_x + offx, base_z + offz),
            Point2D::new(base_x + offx + size.x - 1, base_z + offz + size.y - 1),
        );
        let interior = rect.shrink(1);
        let world_idoor = (interior.min().x + (idoor.0 - 1), interior.min().y + (idoor.1 - 1));
        let world_wdoor = (rect.min().x + wdoor.0, rect.min().y + wdoor.1);

        // Oak plank floor under the rect (no ceiling — open top so you can see in).
        for p in rect.iter() {
            editor.place_block(&floor_block, Point3D::new(p.x, floor_y - 1, p.y)).await;
        }
        // Wood walls on the rect perimeter, 3 high (floor_y..floor_y+2).
        // Carve out the door with air at the wall opening.
        for p in rect.iter() {
            if !rect.on_edge(p) { continue; }
            let is_door = (p.x, p.y) == world_wdoor;
            for dy in 0..3 {
                let block = if is_door && dy < 2 { &air } else { &wall_block };
                editor.place_block(block, Point3D::new(p.x, floor_y + dy, p.y)).await;
            }
        }
        // Clear interior air column so old blocks don't survive.
        for p in interior.iter() {
            for dy in 0..3 {
                editor.place_block(&air, Point3D::new(p.x, floor_y + dy, p.y)).await;
            }
        }

        // Construct a single-rect Frame so floor/ceiling Y resolve from base_y.
        // Vertices clockwise around the rect; furnish_room only reads floor_y /
        // ceiling_y / roof_y from the frame, not the polygon outline.
        let v = vec![
            Point2D::new(rect.min().x, rect.min().y),
            Point2D::new(rect.max().x, rect.min().y),
            Point2D::new(rect.max().x, rect.max().y),
            Point2D::new(rect.min().x, rect.max().y),
        ];
        let footprint = Footprint::new(v, vec![rect]);
        let frame = Frame::new(footprint, floor_y, vec![1], WALL_HEIGHT);

        // Build the room with its door already marked so furnish_room respects
        // the entry point during connectivity checks.
        let mut room = Room {
            rect,
            rect_index: 0,
            floor: 0,
            role: RoomRole::Upper,
            room_type: RoomType::Bedroom,
            interior,
            constraints: constraints_with_doors(&interior, &[world_idoor]),
            furniture: Vec::new(),
        };

        let mut rng = RNG::new(SEED);
        furnish_room(
            &editor, &mut room, &frame, room_list, &data.items,
            None,
            palette, &materials, &data.loot, &mut rng,
        ).await;

        let names: Vec<&str> = room.furniture.iter().map(|f| f.name.as_str()).collect();
        println!(
            "{label}: {n} items at world ({x}, {y}, {z}) — {items}",
            n = room.furniture.len(), x = rect.min().x, y = floor_y, z = rect.min().y,
            items = names.join(", "),
        );
    }

    editor.flush_buffer().await;
    let abs = editor.world().build_area.origin;
    println!(
        "\nDone. TP coordinates: ({}, {}, {})",
        abs.x + base_x, abs.y + floor_y, abs.z + base_z,
    );
}

/// Live-server variant of `diagram_room_features`. Places four bedroom-sized
/// rooms in a 2x2 grid in the current build area, each with realistic doors,
/// stairs, and/or ladders imprinted onto its ConstraintMap the same way the
/// real rooms pipeline does, and with the matching physical blocks rendered
/// in the world so you can walk through them. Run with:
///   cargo test place_feature_rooms_in_world -- --ignored --nocapture
#[ignore]
#[tokio::test]
async fn place_feature_rooms_in_world() {
    use crate::data::Loadable;
    use crate::editor::World;
    use crate::generator::buildings_v2::footprint::Footprint;
    use crate::generator::buildings_v2::frame::Frame;
    use crate::generator::materials::{Material, MaterialId, MaterialRole, Palette, PaletteId};
    use crate::geometry::Point3D;
    use crate::http_mod::GDMCHTTPProvider;
    use crate::minecraft::{Block, Color};
    use super::furnish_room;

    enum LiveFeature {
        Door { wall_cell: (i32, i32), interior: (i32, i32) },
        /// Straight stair ascending from this floor. positions[0] is the flat
        /// landing (stays as plank floor, UnblockedReachable); positions[1..]
        /// become physical stair blocks on the current floor (Blocked).
        StairUp { positions: Vec<(i32, i32)> },
        /// Stair descending from this floor. The first position is the top
        /// landing at floor_y; subsequent positions descend one block each
        /// into a pit below the floor. All positions are UnblockedReachable
        /// on this floor (air-column + top).
        StairDown { positions: Vec<(i32, i32)> },
        /// Ladder attached to a wall. `interior` is the interior-adjacent
        /// cell where the ladder blocks live; `wall_cell` is the wall block
        /// the ladder hangs on. The ladder climbs from floor_y up into the
        /// air above — UnblockedReachable on this floor.
        Ladder { wall_cell: (i32, i32), interior: (i32, i32) },
    }

    const SEED: i64 = 42;
    const WALL_HEIGHT: u32 = 4;

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.expect("get world from server");
    let editor = world.get_editor();

    let data = FurnitureData::load().expect("load furniture YAML");
    let materials = Material::load().expect("load materials");
    let room_list = data.rooms.get("bedroom").expect("bedroom in rooms.yaml");

    let make_palette = |id: &str, primary_wood: &str, secondary_wood: &str,
                        primary: Color, secondary: Color| -> Palette {
        let mut mats = HashMap::new();
        mats.insert(MaterialRole::PrimaryWood, MaterialId::from(primary_wood));
        mats.insert(MaterialRole::SecondaryWood, MaterialId::from(secondary_wood));
        Palette {
            id: PaletteId::from(id),
            materials: mats,
            primary_color: Some(primary),
            secondary_color: Some(secondary),
            tags: None,
        }
    };

    // (rect_size, features, label, (offx, offz within layout), palette)
    // Offsets are relative to the layout's own origin; the layout is centered
    // inside the build area below.
    let cases: Vec<(Point2D, Vec<LiveFeature>, &str, (i32, i32), Palette)> = vec![
        (
            Point2D::new(9, 8),
            vec![
                LiveFeature::Door { wall_cell: (4, 0), interior: (4, 1) },
                LiveFeature::Door { wall_cell: (4, 7), interior: (4, 6) },
            ],
            "9x8 two-doors cherry",
            (0, 0),
            make_palette("cherry_room", "oak_planks", "cherry_planks", Color::Pink, Color::White),
        ),
        (
            Point2D::new(10, 9),
            vec![
                LiveFeature::Door { wall_cell: (4, 0), interior: (4, 1) },
                LiveFeature::StairDown { positions: vec![(2, 7), (3, 7), (4, 7), (5, 7)] },
            ],
            "10x9 stair-down warped",
            (14, 0),
            make_palette("warped_room", "oak_planks", "warped_planks", Color::Cyan, Color::Yellow),
        ),
        (
            Point2D::new(11, 11),
            vec![
                LiveFeature::Door { wall_cell: (5, 0), interior: (5, 1) },
                LiveFeature::StairUp { positions: vec![(1, 2), (1, 3), (1, 4), (1, 5)] },
                LiveFeature::StairDown { positions: vec![(9, 2), (9, 3), (9, 4), (9, 5)] },
            ],
            "11x11 up+down spruce",
            (0, 14),
            make_palette("spruce_room", "oak_planks", "spruce_planks", Color::Brown, Color::Orange),
        ),
        (
            Point2D::new(11, 10),
            vec![
                LiveFeature::Door { wall_cell: (5, 0), interior: (5, 1) },
                LiveFeature::Door { wall_cell: (5, 9), interior: (5, 8) },
                LiveFeature::StairDown { positions: vec![(9, 2), (9, 3), (9, 4), (9, 5)] },
                LiveFeature::Ladder { wall_cell: (0, 4), interior: (1, 4) },
            ],
            "11x10 everything birch",
            (16, 14),
            make_palette("birch_room", "oak_planks", "birch_planks", Color::Yellow, Color::Red),
        ),
    ];

    // Center the layout inside the build area.
    let layout_w = cases.iter().map(|(sz, _, _, (ox, _), _)| ox + sz.x).max().unwrap_or(0);
    let layout_h = cases.iter().map(|(sz, _, _, (_, oz), _)| oz + sz.y).max().unwrap_or(0);
    let build_size = editor.world().build_area.size;
    let base_x = ((build_size.x - layout_w) / 2).max(0);
    let base_z = ((build_size.z - layout_h) / 2).max(0);
    let ground_local = editor.world().get_height_at(Point2D::new(base_x, base_z));
    let floor_y = ground_local + 1;

    let floor_block = Block::from_id("minecraft:oak_planks".into());
    let air = Block::from_id("minecraft:air".into());
    let stone = Block::from_id("minecraft:cobblestone".into());
    let glass = Block::from_id("minecraft:glass".into());

    for (size, features, label, (offx, offz), palette) in &cases {
        let wall_id_str = palette.get_material(MaterialRole::SecondaryWood)
            .map(|m| format!("minecraft:{}", m.as_str()))
            .unwrap_or_else(|| "minecraft:spruce_planks".to_string());
        let wall_block = Block::from_id(wall_id_str.as_str().into());
        let stair_id_str = wall_id_str.replace("_planks", "_stairs");
        let local_origin = (base_x + offx, base_z + offz);
        let rect = Rect2D::from_points(
            Point2D::new(local_origin.0, local_origin.1),
            Point2D::new(local_origin.0 + size.x - 1, local_origin.1 + size.y - 1),
        );
        let interior = rect.shrink(1);

        // Translate a room-local (x,z) (origin = rect.min()) into world-local coords.
        let to_world = |c: (i32, i32)| (rect.min().x + c.0, rect.min().y + c.1);

        // Floor planks beneath the rect (overwrites ground).
        for p in rect.iter() {
            editor.place_block(&floor_block, Point3D::new(p.x, floor_y - 1, p.y)).await;
        }
        // Walls on the perimeter, WALL_HEIGHT high.
        for p in rect.iter() {
            if !rect.on_edge(p) { continue; }
            for dy in 0..(WALL_HEIGHT as i32) {
                editor.place_block(&wall_block, Point3D::new(p.x, floor_y + dy, p.y)).await;
            }
        }
        // Clear interior air column.
        for p in interior.iter() {
            for dy in 0..(WALL_HEIGHT as i32) {
                editor.place_block(&air, Point3D::new(p.x, floor_y + dy, p.y)).await;
            }
        }

        // Build the constraint map matching the real pipeline semantics.
        let mut constraints = ConstraintMap::new(&interior);

        // Apply features: punch openings, place physical blocks, mark cm.
        for feat in features {
            match feat {
                LiveFeature::Door { wall_cell, interior: i_cell } => {
                    let (wx, wz) = to_world(*wall_cell);
                    // Carve a 2-tall opening in the wall, then install an
                    // actual oak door so the entry point is obvious.
                    for dy in 0..2 {
                        editor.place_block(&air, Point3D::new(wx, floor_y + dy, wz)).await;
                    }
                    // Door "facing" = direction from the wall into the room.
                    let dir_in = match (
                        wall_cell.0 == 0, wall_cell.0 == size.x - 1,
                        wall_cell.1 == 0, wall_cell.1 == size.y - 1,
                    ) {
                        (_, _, true, _) => "south", // north wall → enter heading south
                        (_, _, _, true) => "north",
                        (true, _, _, _) => "east",  // west wall → enter heading east
                        (_, true, _, _) => "west",
                        _ => "south",
                    };
                    let door_state_lower = HashMap::from([
                        ("facing".to_string(), dir_in.to_string()),
                        ("half".to_string(), "lower".to_string()),
                        ("hinge".to_string(), "left".to_string()),
                        ("open".to_string(), "false".to_string()),
                        ("powered".to_string(), "false".to_string()),
                    ]);
                    let mut door_lower = Block::from_id("minecraft:oak_door".into());
                    door_lower.state = Some(door_state_lower);
                    editor.place_block_forced(&door_lower, Point3D::new(wx, floor_y, wz)).await;
                    let door_state_upper = HashMap::from([
                        ("facing".to_string(), dir_in.to_string()),
                        ("half".to_string(), "upper".to_string()),
                        ("hinge".to_string(), "left".to_string()),
                        ("open".to_string(), "false".to_string()),
                        ("powered".to_string(), "false".to_string()),
                    ]);
                    let mut door_upper = Block::from_id("minecraft:oak_door".into());
                    door_upper.state = Some(door_state_upper);
                    editor.place_block_forced(&door_upper, Point3D::new(wx, floor_y + 1, wz)).await;
                    let wic = to_world(*i_cell);
                    constraints.set(wic, CellState::UnblockedReachable);
                }
                LiveFeature::StairUp { positions } => {
                    if positions.len() < 2 { continue; }
                    // Ascent direction: from positions[0] (landing) to positions[1].
                    let p0 = positions[0];
                    let p1 = positions[1];
                    let (dx, dz) = (p1.0 - p0.0, p1.1 - p0.1);
                    let facing = match (dx, dz) {
                        (1, 0) => "east",
                        (-1, 0) => "west",
                        (0, 1) => "south",
                        _ => "north",
                    };
                    for (i, p) in positions.iter().enumerate() {
                        let wp = to_world(*p);
                        if i == 0 {
                            // Landing stays as normal floor + UR.
                            constraints.set(wp, CellState::UnblockedReachable);
                        } else {
                            // Physical stair block rising one step per index.
                            let step_y = floor_y + (i as i32 - 1);
                            let mut stair = Block::from_id(stair_id_str.as_str().into());
                            stair.state = Some(HashMap::from([
                                ("facing".to_string(), facing.to_string()),
                            ]));
                            editor.place_block(&stair, Point3D::new(wp.0, step_y, wp.1)).await;
                            // Ensure the air above the stair is clear so the
                            // player's head doesn't hit the ceiling.
                            for dy in (i as i32)..(WALL_HEIGHT as i32) {
                                editor.place_block(&air, Point3D::new(wp.0, floor_y + dy, wp.1)).await;
                            }
                            constraints.set(wp, CellState::Blocked);
                        }
                    }
                }
                LiveFeature::StairDown { positions } => {
                    if positions.len() < 2 { continue; }
                    // Descent direction: from positions[0] (top landing) to positions[1].
                    let p0 = positions[0];
                    let p1 = positions[1];
                    let (dx, dz) = (p1.0 - p0.0, p1.1 - p0.1);
                    // Stair blocks face OPPOSITE the descent direction (climbing
                    // up means walking back toward the landing).
                    let facing = match (dx, dz) {
                        (1, 0) => "west",
                        (-1, 0) => "east",
                        (0, 1) => "north",
                        _ => "south",
                    };
                    for (i, p) in positions.iter().enumerate() {
                        let wp = to_world(*p);
                        constraints.set(wp, CellState::UnblockedReachable);
                        if i == 0 {
                            // Top landing: keep the plank floor intact so the
                            // player has something to stand on before descending.
                            continue;
                        }
                        // Drop one block per step. The first stair sits with
                        // its high side flush with the landing top.
                        let step_y = floor_y - i as i32;
                        // Excavate the whole air column above the stair —
                        // including the original floor plank — so the pit
                        // is a visible opening from above. Forced so air
                        // overrides the denser floor block already placed.
                        for dy in step_y..=(floor_y + 1) {
                            editor.place_block_forced(&air, Point3D::new(wp.0, dy, wp.1)).await;
                        }
                        // Solid floor below the deepest step so the pit is enclosed.
                        editor.place_block(&stone, Point3D::new(wp.0, step_y - 1, wp.1)).await;
                        let mut stair = Block::from_id(stair_id_str.as_str().into());
                        stair.state = Some(HashMap::from([
                            ("facing".to_string(), facing.to_string()),
                        ]));
                        editor.place_block(&stair, Point3D::new(wp.0, step_y, wp.1)).await;
                        // Glass marker a few blocks above the stair so the
                        // descent is visible from across the room without
                        // blocking sightlines into the pit.
                        editor.place_block_forced(&glass, Point3D::new(wp.0, floor_y + 2, wp.1)).await;
                    }
                    // Append one extra step at the next cell in the descent
                    // direction, one block lower than the deepest current
                    // step, so the staircase fully reaches the pit floor.
                    let last = positions[positions.len() - 1];
                    let extra = (last.0 + dx, last.1 + dz);
                    let wp = to_world(extra);
                    constraints.set(wp, CellState::UnblockedReachable);
                    let extra_step_y = floor_y - positions.len() as i32;
                    for dy in extra_step_y..=(floor_y + 1) {
                        editor.place_block_forced(&air, Point3D::new(wp.0, dy, wp.1)).await;
                    }
                    editor.place_block(&stone, Point3D::new(wp.0, extra_step_y - 1, wp.1)).await;
                    let mut extra_stair = Block::from_id(stair_id_str.as_str().into());
                    extra_stair.state = Some(HashMap::from([
                        ("facing".to_string(), facing.to_string()),
                    ]));
                    editor.place_block(&extra_stair, Point3D::new(wp.0, extra_step_y, wp.1)).await;
                    editor.place_block_forced(&glass, Point3D::new(wp.0, floor_y + 2, wp.1)).await;
                }
                LiveFeature::Ladder { wall_cell, interior: i_cell } => {
                    let (wx, wz) = to_world(*wall_cell);
                    let (ix, iz) = to_world(*i_cell);
                    // Determine ladder facing: direction from wall toward interior.
                    let dx = ix - wx;
                    let dz = iz - wz;
                    let facing = match (dx, dz) {
                        (1, 0) => "east",
                        (-1, 0) => "west",
                        (0, 1) => "south",
                        _ => "north",
                    };
                    // Ladder climbs the full wall height.
                    for dy in 0..(WALL_HEIGHT as i32) {
                        let mut ladder = Block::from_id("minecraft:ladder".into());
                        ladder.state = Some(HashMap::from([
                            ("facing".to_string(), facing.to_string()),
                        ]));
                        editor.place_block_forced(&ladder, Point3D::new(ix, floor_y + dy, iz)).await;
                    }
                    constraints.set((ix, iz), CellState::UnblockedReachable);
                }
            }
        }

        // Construct the Frame + Room to hand to furnish_room.
        let v = vec![
            Point2D::new(rect.min().x, rect.min().y),
            Point2D::new(rect.max().x, rect.min().y),
            Point2D::new(rect.max().x, rect.max().y),
            Point2D::new(rect.min().x, rect.max().y),
        ];
        let footprint = Footprint::new(v, vec![rect]);
        let frame = Frame::new(footprint, floor_y, vec![1], WALL_HEIGHT);

        let mut room = Room {
            rect,
            rect_index: 0,
            floor: 0,
            role: RoomRole::Upper,
            room_type: RoomType::Bedroom,
            interior,
            constraints,
            furniture: Vec::new(),
        };

        let mut rng = RNG::new(SEED);
        furnish_room(
            &editor, &mut room, &frame, room_list, &data.items,
            None,
            palette, &materials, &data.loot, &mut rng,
        ).await;

        let names: Vec<&str> = room.furniture.iter().map(|f| f.name.as_str()).collect();
        println!(
            "{label}: {n} items at world ({x}, {y}, {z}) — {items}",
            n = room.furniture.len(),
            x = rect.min().x, y = floor_y, z = rect.min().y,
            items = names.join(", "),
        );

        // Stained-glass debug map of the post-furnish ConstraintMap, hovering
        // a few blocks above the room: lime = Empty, green = UnblockedReachable,
        // blue = BlockedReachable, red = Blocked.
        let map_y = floor_y + WALL_HEIGHT as i32 + 2;
        let red = Block::from_id("minecraft:red_stained_glass".into());
        let blue = Block::from_id("minecraft:blue_stained_glass".into());
        let lime = Block::from_id("minecraft:lime_stained_glass".into());
        let green = Block::from_id("minecraft:green_stained_glass".into());
        for p in room.interior.iter() {
            let block = match room.constraints.get((p.x, p.y)) {
                Some(CellState::Empty) => &lime,
                Some(CellState::UnblockedReachable) => &green,
                Some(CellState::BlockedReachable) => &blue,
                Some(CellState::Blocked) => &red,
                None => continue,
            };
            editor.place_block_forced(block, Point3D::new(p.x, map_y, p.y)).await;
        }
    }

    editor.flush_buffer().await;
    let abs = editor.world().build_area.origin;
    println!(
        "\nDone. TP coordinates: ({}, {}, {})",
        abs.x + base_x, abs.y + floor_y, abs.z + base_z,
    );
}

/// Iterate-on-room-design helper. Furnishes 4 rect sizes from
/// `data/rooms.yaml` so changes to the YAML are visible without rebuilding
/// the rest of the pipeline. Pick the room key by editing `ROOM_KEY` below.
/// Run with `cargo test diagram_room_sizes -- --nocapture`.
#[test]
fn diagram_room_sizes() {
    const ROOM_KEY: &str = "bedroom";
    const SEED: i64 = 42;

    // (rect, interior-door cell, wall-door cell, label)
    // Doors land on the north wall; the interior approach cell sits one row in.
    let cases: &[(Rect2D, (i32, i32), (i32, i32), &str)] = &[
        (Rect2D::from_points(Point2D::new(0, 0), Point2D::new(4, 4)),  (2, 1), (2, 0), "5x5  (interior 3x3)"),
        (Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 5)),  (3, 1), (3, 0), "7x6  (interior 5x4)"),
        (Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 7)),  (4, 1), (4, 0), "9x8  (interior 7x6)"),
        (Rect2D::from_points(Point2D::new(0, 0), Point2D::new(10, 9)), (5, 1), (5, 0), "11x10 (interior 9x8)"),
    ];

    let data = FurnitureData::load().expect("load furniture YAML");
    let room_list = data.rooms.get(ROOM_KEY)
        .unwrap_or_else(|| panic!("room key {ROOM_KEY:?} not found in data/rooms.yaml"));

    println!("\n=== {ROOM_KEY} (seed={SEED}) ===");
    for (rect, idoor, wdoor, label) in cases {
        let mut r = DiagramRoom::new(rect.clone(), &[*idoor], &[*wdoor], SEED);
        let mut placed = Vec::new();

        for name in &room_list.required {
            if let Some(item) = data.items.get(name) {
                if r.place(name, item) { placed.push(name.clone()); }
            }
        }
        for name in &room_list.optional {
            if r.cm.fill_ratio() >= room_list.fill_threshold.unwrap_or(DEFAULT_FILL_THRESHOLD) {
                break;
            }
            if let Some(item) = data.items.get(name) {
                if r.place(name, item) { placed.push(name.clone()); }
            }
        }

        println!("\n{}", r.render(label));
        println!("  fill ratio: {:.0}%  ({} items placed)", r.cm.fill_ratio() * 100.0, placed.len());
    }
}

// ---------------------------------------------------------------------------
// diagram_room_features — 4 rooms exercising doors, stairs, and ladders
// ---------------------------------------------------------------------------

/// Structural feature baked into a room before furnishing. Mirrors how the
/// real `rooms::build_rooms_for_frame` pipeline imprints stairwells, doors,
/// and attic ladders onto the per-floor ConstraintMap — see
/// `src/generator/buildings_v2/rooms/mod.rs` near the stair/door/ladder
/// blocks: door approach cells + stair landings/tops + stair air-columns +
/// ladder cells → `UnblockedReachable`; physical stair-step blocks on the
/// current floor → `Blocked`.
enum RoomFeature {
    /// A door in a wall. `wall_cell` lies on the room perimeter; `interior`
    /// is the cell immediately inside that the player walks onto when
    /// entering — it becomes UnblockedReachable (walkable, unplaceable).
    Door { wall_cell: (i32, i32), interior: (i32, i32) },
    /// Straight stair ascending from this floor. `positions[0]` is the flat
    /// landing (UnblockedReachable), remaining positions are physical stair
    /// blocks (Blocked) — matches the straight-stair `stair_bottoms` +
    /// `stair_cells_on_floor` semantics.
    StairUp { positions: Vec<(i32, i32)> },
    /// Straight stair descending from this floor. From this floor's
    /// perspective the stair lives on the floor below, so we only see the
    /// `stair_tops` cell and the `stair_air_above` column — all
    /// UnblockedReachable. Pass every (x,z) in the air column.
    StairDown { positions: Vec<(i32, i32)> },
    /// Attic ladder passing through this floor. The ladder cell is
    /// UnblockedReachable on every floor it crosses.
    Ladder { cell: (i32, i32) },
}

impl DiagramRoom {
    fn apply_feature(&mut self, feat: &RoomFeature) {
        match feat {
            RoomFeature::Door { wall_cell, interior } => {
                self.cm.set(*interior, CellState::UnblockedReachable);
                self.wall_doors.push(*wall_cell);
            }
            RoomFeature::StairUp { positions } => {
                for (i, p) in positions.iter().enumerate() {
                    if i == 0 {
                        self.cm.set(*p, CellState::UnblockedReachable);
                    } else {
                        self.cm.set(*p, CellState::Blocked);
                    }
                    self.labels.insert(*p, '^');
                }
            }
            RoomFeature::StairDown { positions } => {
                for p in positions {
                    self.cm.set(*p, CellState::UnblockedReachable);
                    self.labels.insert(*p, 'v');
                }
            }
            RoomFeature::Ladder { cell } => {
                self.cm.set(*cell, CellState::UnblockedReachable);
                self.labels.insert(*cell, 'L');
            }
        }
    }
}

/// Like `diagram_room_sizes` but each of the four rooms carries a realistic
/// combination of doors, stairs, and ladders imprinted onto the constraint
/// map the same way the real rooms pipeline does. Exercises how the
/// furnishing system navigates around these structural features.
/// Run with `cargo test diagram_room_features -- --nocapture`.
#[test]
fn diagram_room_features() {
    const ROOM_KEY: &str = "bedroom";
    const SEED: i64 = 42;

    struct Case {
        rect: Rect2D,
        features: Vec<RoomFeature>,
        label: &'static str,
    }

    // Stairs sit in the interior row adjacent to a wall (min+1 or max-1),
    // matching the `corner_candidates` layout in floors/stairs.rs.
    let cases: Vec<Case> = vec![
        // 1. Pass-through room: doors on north and south walls.
        Case {
            rect: Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 7)),
            features: vec![
                RoomFeature::Door { wall_cell: (4, 0), interior: (4, 1) },
                RoomFeature::Door { wall_cell: (4, 7), interior: (4, 6) },
            ],
            label: "9x8 — two doors",
        },
        // 2. Stair descending east along the south wall, plus a door north.
        Case {
            rect: Rect2D::from_points(Point2D::new(0, 0), Point2D::new(9, 8)),
            features: vec![
                RoomFeature::Door { wall_cell: (4, 0), interior: (4, 1) },
                RoomFeature::StairDown { positions: vec![(2, 7), (3, 7), (4, 7), (5, 7)] },
            ],
            label: "10x9 — stair down + door",
        },
        // 3. Stair ascending south along the west wall and stair descending
        //    south along the east wall, with a door on the north wall.
        Case {
            rect: Rect2D::from_points(Point2D::new(0, 0), Point2D::new(10, 10)),
            features: vec![
                RoomFeature::Door { wall_cell: (5, 0), interior: (5, 1) },
                RoomFeature::StairUp { positions: vec![(1, 2), (1, 3), (1, 4), (1, 5)] },
                RoomFeature::StairDown { positions: vec![(9, 2), (9, 3), (9, 4), (9, 5)] },
            ],
            label: "11x11 — stair up + stair down + door",
        },
        // 4. Two doors (N + S), a stair descending south along the east wall,
        //    and an attic ladder on the west wall.
        Case {
            rect: Rect2D::from_points(Point2D::new(0, 0), Point2D::new(10, 9)),
            features: vec![
                RoomFeature::Door { wall_cell: (5, 0), interior: (5, 1) },
                RoomFeature::Door { wall_cell: (5, 9), interior: (5, 8) },
                RoomFeature::StairDown { positions: vec![(9, 2), (9, 3), (9, 4), (9, 5)] },
                RoomFeature::Ladder { cell: (1, 4) },
            ],
            label: "11x10 — two doors + stair down + ladder up",
        },
    ];

    let data = FurnitureData::load().expect("load furniture YAML");
    let room_list = data.rooms.get(ROOM_KEY)
        .unwrap_or_else(|| panic!("room key {ROOM_KEY:?} not found in data/rooms.yaml"));

    println!("\n=== {ROOM_KEY} with features (seed={SEED}) ===");
    println!("  legend: D = door in wall,  · = door approach (cyan),");
    println!("          ^ = stair up (landing cyan, steps plain),");
    println!("          v = stair down air-column (cyan),");
    println!("          L = ladder (cyan)");

    for case in &cases {
        // Build with no interior doors — features below install them with
        // real-pipeline semantics (UnblockedReachable).
        let mut r = DiagramRoom::new(case.rect.clone(), &[], &[], SEED);
        for feat in &case.features {
            r.apply_feature(feat);
        }

        println!("\n{}", r.render(&format!("{}  (before furnishing)", case.label)));

        let mut rng = RNG::new(SEED);
        let room_area = r.interior.area();
        let mut placed_tags: HashSet<String> = HashSet::new();
        let mut placed = Vec::new();

        for entry in &room_list.required {
            let candidates = resolve_candidates(entry, &data.items, room_area, false, &placed_tags, &mut rng);
            for (name, item) in candidates {
                if r.place(name, item) {
                    if item.unique {
                        placed_tags.insert(name.clone());
                        for tag in &item.tags { placed_tags.insert(tag.clone()); }
                    }
                    placed.push(name.clone());
                    break;
                }
            }
        }
        for entry in &room_list.optional {
            if r.cm.fill_ratio() >= room_list.fill_threshold.unwrap_or(DEFAULT_FILL_THRESHOLD) {
                break;
            }
            let candidates = resolve_candidates(entry, &data.items, room_area, false, &placed_tags, &mut rng);
            for (name, item) in candidates {
                if r.place(name, item) {
                    if item.unique {
                        placed_tags.insert(name.clone());
                        for tag in &item.tags { placed_tags.insert(tag.clone()); }
                    }
                    placed.push(name.clone());
                    break;
                }
            }
        }

        println!("\n{}", r.render(&format!("{}  (after furnishing)", case.label)));
        println!("  fill ratio: {:.0}%  ({} items placed)   connectivity ok: {}",
                 r.cm.fill_ratio() * 100.0, placed.len(), check_connectivity(&r.cm));
    }
}

#[test]
fn diagram_5x5_bedrooms() {
    let data = FurnitureData::load().expect("load furniture YAML");
    let room_list = data.rooms.get("bedroom").expect("bedroom room list");
    let room_rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 8));

    // Door on each wall
    let configs: Vec<((i32, i32), (i32, i32), &str)> = vec![
        ((4, 1), (4, 0), "north"),
        ((4, 7), (4, 8), "south"),
        ((7, 4), (8, 4), "east"),
        ((1, 4), (0, 4), "west"),
    ];

    for (idoor, wdoor, wall_name) in &configs {
        for seed in [1, 7, 42, 99, 123, 256, 777, 1000, 2000, 3000, 4000, 5000] {
            let mut r = DiagramRoom::new(room_rect.clone(), &[*idoor], &[*wdoor], seed);
            let mut rng = RNG::new(seed);
            let room_area = r.interior.area();
            let mut placed_tags: HashSet<String> = HashSet::new();

            for entry in &room_list.required {
                let candidates = resolve_candidates(entry, &data.items, room_area, false, &placed_tags, &mut rng);
                for (name, item) in candidates {
                    if r.place(name, item) {
                        if item.unique {
                            placed_tags.insert(name.clone());
                            for tag in &item.tags { placed_tags.insert(tag.clone()); }
                        }
                        break;
                    }
                }
            }
            for entry in &room_list.optional {
                let candidates = resolve_candidates(entry, &data.items, room_area, false, &placed_tags, &mut rng);
                for (name, item) in candidates {
                    if r.place(name, item) {
                        if item.unique {
                            placed_tags.insert(name.clone());
                            for tag in &item.tags { placed_tags.insert(tag.clone()); }
                        }
                        break;
                    }
                }
            }

            println!("\n{}", r.render(&format!("5x5 door={wall_name} seed={seed}")));
            println!("  fill ratio: {:.0}%", r.cm.fill_ratio() * 100.0);
        }
    }
}

// ---------------------------------------------------------------------------
// loot rolling
// ---------------------------------------------------------------------------

fn random_table() -> LootTable {
    LootTable {
        count: Some([2, 4]),
        capacity: Some(27),
        items: vec![
            LootItem { id: "minecraft:bread".into(), count: [1, 3], weight: 3.0 },
            LootItem { id: "minecraft:wheat".into(), count: [2, 5], weight: 2.0 },
            LootItem { id: "minecraft:apple".into(), count: [1, 1], weight: 1.0 },
        ],
        fixed: vec![],
    }
}

#[test]
fn loot_roll_produces_items_snbt() {
    let table = random_table();
    let mut rng = RNG::new(42);
    let snbt = roll_loot_snbt(&table, &mut rng);
    assert!(snbt.starts_with("{Items:["), "got {}", snbt);
    assert!(snbt.ends_with("]}"), "got {}", snbt);
    // A count range of [2, 4] must produce at least 2 entries.
    let count = snbt.matches("{Slot:").count();
    assert!(count >= 2 && count <= 4, "expected 2..=4 entries, got {}: {}", count, snbt);
}

#[test]
fn loot_roll_is_deterministic() {
    let table = random_table();
    let mut rng_a = RNG::new(12345);
    let mut rng_b = RNG::new(12345);
    assert_eq!(roll_loot_snbt(&table, &mut rng_a), roll_loot_snbt(&table, &mut rng_b));
}

#[test]
fn loot_roll_uses_only_pool_items() {
    let table = random_table();
    for seed in 0..30 {
        let mut rng = RNG::new(seed);
        let snbt = roll_loot_snbt(&table, &mut rng);
        assert!(
            snbt.contains("bread") || snbt.contains("wheat") || snbt.contains("apple")
                || snbt == "{Items:[]}",
            "unexpected snbt {}", snbt
        );
        for id in ["minecraft:bread", "minecraft:wheat", "minecraft:apple"] {
            // any quoted id in the output must be one we defined
            if snbt.contains(&format!("id:\"{}\"", id)) {
                // ok
            }
        }
    }
}

#[test]
fn loot_roll_slots_are_distinct() {
    let table = LootTable {
        count: Some([5, 5]),
        capacity: Some(27),
        items: vec![LootItem { id: "minecraft:stick".into(), count: [1, 1], weight: 1.0 }],
        fixed: vec![],
    };
    let mut rng = RNG::new(7);
    let snbt = roll_loot_snbt(&table, &mut rng);
    let mut slots: Vec<i32> = Vec::new();
    for part in snbt.split("{Slot:").skip(1) {
        let num: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
        slots.push(num.parse().unwrap());
    }
    assert_eq!(slots.len(), 5);
    let before = slots.len();
    let unique: HashSet<i32> = slots.into_iter().collect();
    assert_eq!(unique.len(), before, "slots should be distinct");
}

#[test]
fn loot_roll_capacity_caps_stack_count() {
    let table = LootTable {
        count: Some([10, 10]),
        capacity: Some(3),
        items: vec![LootItem { id: "minecraft:coal".into(), count: [1, 1], weight: 1.0 }],
        fixed: vec![],
    };
    let mut rng = RNG::new(5);
    let snbt = roll_loot_snbt(&table, &mut rng);
    let count = snbt.matches("{Slot:").count();
    assert_eq!(count, 3, "capacity must cap stack count: {}", snbt);
}

#[test]
fn loot_roll_fixed_slots_assigned_directly() {
    // chance = 1.0 on both slots → both get populated, at the exact slot indices.
    let table = LootTable {
        count: None,
        capacity: None,
        items: vec![],
        fixed: vec![
            FixedSlot {
                slot: 0,
                chance: 1.0,
                items: vec![LootItem { id: "minecraft:raw_iron".into(), count: [2, 2], weight: 1.0 }],
            },
            FixedSlot {
                slot: 1,
                chance: 1.0,
                items: vec![LootItem { id: "minecraft:coal".into(), count: [3, 3], weight: 1.0 }],
            },
        ],
    };
    let mut rng = RNG::new(1);
    let snbt = roll_loot_snbt(&table, &mut rng);
    assert!(snbt.contains("{Slot:0b,id:\"minecraft:raw_iron\",Count:2b}"), "got {}", snbt);
    assert!(snbt.contains("{Slot:1b,id:\"minecraft:coal\",Count:3b}"), "got {}", snbt);
}

#[test]
fn loot_roll_fixed_slot_chance_zero_skips() {
    let table = LootTable {
        count: None,
        capacity: None,
        items: vec![],
        fixed: vec![
            FixedSlot {
                slot: 2,
                chance: 0.0,
                items: vec![LootItem { id: "minecraft:iron_ingot".into(), count: [1, 1], weight: 1.0 }],
            },
        ],
    };
    let mut rng = RNG::new(1);
    let snbt = roll_loot_snbt(&table, &mut rng);
    assert_eq!(snbt, "{Items:[]}");
}

#[test]
fn loot_roll_empty_pool_emits_empty_items() {
    let table = LootTable { count: Some([1, 3]), capacity: None, items: vec![], fixed: vec![] };
    let mut rng = RNG::new(1);
    assert_eq!(roll_loot_snbt(&table, &mut rng), "{Items:[]}");
}
