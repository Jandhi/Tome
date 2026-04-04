use std::collections::HashMap;
use crate::geometry::{Cardinal, Point2D, Rect2D};
use crate::minecraft::Block;
use crate::noise::RNG;
use crate::generator::buildings_v2::RoomType;
use crate::generator::buildings_v2::rooms::{FloorCell, FloorMap, Room, RoomRole};
use super::{
    interior_rect, wall_slots, flood_fill, check_connectivity,
    placement_keeps_connectivity, shuffle, fill_ratio, needs_wall,
    resolve_offset, constraint_to_floor_cell, try_place_at_wall_slot,
    CellConstraint, FacingMode, BlockLayer, OccupancyMap,
    FurnitureItem, PlacedBlock, PlacedConstraint,
};
use super::data::{FurnitureItemDef, RoomFurnitureDef, resolve_furniture_list};

fn make_room(rect: Rect2D, floor_map: FloorMap) -> Room {
    Room {
        rect,
        rect_index: 0,
        floor: 0,
        role: RoomRole::Upper,
        room_type: RoomType::Bedroom,
        floor_map,
    }
}

fn open_map(interior: &Rect2D) -> FloorMap {
    interior.iter().map(|c| ((c.x, c.y), FloorCell::Open)).collect()
}

fn map_with_entrances(interior: &Rect2D, entrances: &[(i32, i32)]) -> FloorMap {
    let mut map = open_map(interior);
    for &e in entrances { map.insert(e, FloorCell::ReachableOpen); }
    map
}

/// Build a bed item for testing (same as what bed.json produces).
fn test_bed() -> FurnitureItem {
    FurnitureItem {
        name: "bed".into(),
        unique: true,
        blocks: vec![
            PlacedBlock {
                block: Block::new("minecraft:red_bed".into(),
                    Some([("part".into(), "head".into())].into()), None),
                offset: (0, 0, 0),
                layer: BlockLayer::Ground,
            },
            PlacedBlock {
                block: Block::new("minecraft:red_bed".into(),
                    Some([("part".into(), "foot".into())].into()), None),
                offset: (0, 1, 0),
                layer: BlockLayer::Ground,
            },
        ],
        constraints: vec![
            PlacedConstraint { offset: (0, 0), constraint: CellConstraint::Wall, facing: FacingMode::AwayFromWall },
            PlacedConstraint { offset: (0, 1), constraint: CellConstraint::Accessible, facing: FacingMode::AwayFromWall },
        ],
    }
}

fn test_chest() -> FurnitureItem {
    FurnitureItem {
        name: "chest".into(),
        unique: false,
        blocks: vec![
            PlacedBlock {
                block: Block::from_id("minecraft:chest".into()),
                offset: (0, 0, 0),
                layer: BlockLayer::Ground,
            },
        ],
        constraints: vec![
            PlacedConstraint { offset: (0, 0), constraint: CellConstraint::Accessible, facing: FacingMode::AwayFromWall },
        ],
    }
}

fn test_lantern() -> FurnitureItem {
    FurnitureItem {
        name: "lantern".into(),
        unique: true,
        blocks: vec![
            PlacedBlock {
                block: Block::new("minecraft:lantern".into(),
                    Some([("hanging".into(), "true".into())].into()), None),
                offset: (0, 0, 0),
                layer: BlockLayer::Ceiling,
            },
        ],
        constraints: vec![],
    }
}

fn test_bookshelf() -> FurnitureItem {
    FurnitureItem {
        name: "bookshelf".into(),
        unique: false,
        blocks: vec![
            PlacedBlock {
                block: Block::from_id("minecraft:bookshelf".into()),
                offset: (0, 0, 0),
                layer: BlockLayer::Ground,
            },
        ],
        constraints: vec![
            PlacedConstraint { offset: (0, 0), constraint: CellConstraint::Wall, facing: FacingMode::None },
        ],
    }
}

// ---------------------------------------------------------------------------
// interior_rect
// ---------------------------------------------------------------------------

#[test]
fn interior_rect_normal() {
    let room = make_room(
        Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 6)),
        FloorMap::new(),
    );
    let interior = interior_rect(&room).unwrap();
    assert_eq!(interior.min(), Point2D::new(1, 1));
    assert_eq!(interior.max(), Point2D::new(5, 5));
}

#[test]
fn interior_rect_too_small() {
    let room = make_room(
        Rect2D::from_points(Point2D::new(0, 0), Point2D::new(1, 1)),
        FloorMap::new(),
    );
    assert!(interior_rect(&room).is_none());
}

#[test]
fn interior_rect_minimum_3x3() {
    let room = make_room(
        Rect2D::from_points(Point2D::new(0, 0), Point2D::new(2, 2)),
        FloorMap::new(),
    );
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
    let (dx, dz, dy) = resolve_offset((1, 2, 3), Cardinal::North);
    assert_eq!(dx, 1);
    assert_eq!(dz, 2);
    assert_eq!(dy, 3);
}

#[test]
fn resolve_offset_east_wall() {
    let (dx, dz, _) = resolve_offset((1, 2, 0), Cardinal::East);
    assert_eq!(dx, -2);
    assert_eq!(dz, 1);
}

#[test]
fn resolve_offset_south_wall() {
    let (dx, dz, _) = resolve_offset((1, 2, 0), Cardinal::South);
    assert_eq!(dx, -1);
    assert_eq!(dz, -2);
}

#[test]
fn resolve_offset_west_wall() {
    let (dx, dz, _) = resolve_offset((1, 2, 0), Cardinal::West);
    assert_eq!(dx, 2);
    assert_eq!(dz, -1);
}

#[test]
fn resolve_offset_zero() {
    let (dx, dz, dy) = resolve_offset((0, 0, 0), Cardinal::North);
    assert_eq!((dx, dz, dy), (0, 0, 0));
}

// ---------------------------------------------------------------------------
// needs_wall
// ---------------------------------------------------------------------------

#[test]
fn bed_needs_wall() {
    assert!(needs_wall(&test_bed()));
}

#[test]
fn chest_needs_wall() {
    assert!(needs_wall(&test_chest()));
}

#[test]
fn lantern_does_not_need_wall() {
    assert!(!needs_wall(&test_lantern()));
}

#[test]
fn bookshelf_needs_wall() {
    assert!(needs_wall(&test_bookshelf()));
}

// ---------------------------------------------------------------------------
// occupancy map
// ---------------------------------------------------------------------------

#[test]
fn occupancy_same_layer_collides() {
    let mut occ = OccupancyMap::new();
    occ.insert((3, 3), BlockLayer::Ground);
    assert!(occ.is_occupied((3, 3), BlockLayer::Ground));
}

#[test]
fn occupancy_different_layer_no_collision() {
    let mut occ = OccupancyMap::new();
    occ.insert((3, 3), BlockLayer::Ground);
    assert!(!occ.is_occupied((3, 3), BlockLayer::Ceiling));
}

#[test]
fn chest_blocked_by_occupied_ground() {
    let interior = Rect2D::from_points(Point2D::new(1, 1), Point2D::new(5, 5));
    let floor_map = map_with_entrances(&interior, &[(3, 1)]);
    let slots = wall_slots(&interior);

    let mut occ = OccupancyMap::new();
    for slot in &slots { occ.insert((slot.cell.x, slot.cell.y), BlockLayer::Ground); }

    let item = test_chest();
    let result = slots.iter()
        .find_map(|slot| try_place_at_wall_slot(&item, slot, &interior, &floor_map, &occ, 64));
    assert!(result.is_none());
}

// ---------------------------------------------------------------------------
// constraint_to_floor_cell
// ---------------------------------------------------------------------------

#[test]
fn wall_constraint_is_blocked() {
    assert_eq!(constraint_to_floor_cell(CellConstraint::Wall), Some(FloorCell::Blocked));
}

#[test]
fn accessible_constraint_is_reachable_blocked() {
    assert_eq!(constraint_to_floor_cell(CellConstraint::Accessible), Some(FloorCell::ReachableBlocked));
}

#[test]
fn none_constraint_is_none() {
    assert_eq!(constraint_to_floor_cell(CellConstraint::None), None);
}

// ---------------------------------------------------------------------------
// flood_fill
// ---------------------------------------------------------------------------

#[test]
fn flood_fill_open_grid() {
    let interior = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(2, 2));
    let map = open_map(&interior);
    let reached = flood_fill((0, 0), &map);
    assert_eq!(reached.len(), 9);
}

#[test]
fn flood_fill_wall_splits_grid() {
    let interior = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(4, 2));
    let mut map = open_map(&interior);
    for z in 0..=2 { map.insert((2, z), FloorCell::Blocked); }
    let reached = flood_fill((0, 0), &map);
    assert_eq!(reached.len(), 6);
    assert!(!reached.contains(&(3, 0)));
}

#[test]
fn flood_fill_cannot_walk_through_furniture() {
    let interior = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(2, 0));
    let mut map = open_map(&interior);
    map.insert((1, 0), FloorCell::ReachableBlocked);
    let reached = flood_fill((0, 0), &map);
    assert_eq!(reached.len(), 1);
}

#[test]
fn flood_fill_unreachable_start() {
    let map: FloorMap = [((1, 1), FloorCell::Open)].into();
    let reached = flood_fill((0, 0), &map);
    assert_eq!(reached.len(), 0);
}

// ---------------------------------------------------------------------------
// check_connectivity
// ---------------------------------------------------------------------------

#[test]
fn connectivity_no_entrances() {
    let map: FloorMap = [((0, 0), FloorCell::Open)].into();
    assert!(check_connectivity(&map));
}

#[test]
fn connectivity_single_entrance() {
    let map: FloorMap = [
        ((0, 0), FloorCell::ReachableOpen),
        ((1, 0), FloorCell::Open),
    ].into();
    assert!(check_connectivity(&map));
}

#[test]
fn connectivity_two_entrances_connected() {
    let map: FloorMap = [
        ((0, 0), FloorCell::ReachableOpen),
        ((1, 0), FloorCell::Open),
        ((2, 0), FloorCell::ReachableOpen),
    ].into();
    assert!(check_connectivity(&map));
}

#[test]
fn connectivity_two_entrances_disconnected() {
    let map: FloorMap = [
        ((0, 0), FloorCell::ReachableOpen),
        ((1, 0), FloorCell::Blocked),
        ((2, 0), FloorCell::ReachableOpen),
    ].into();
    assert!(!check_connectivity(&map));
}

#[test]
fn connectivity_furniture_adjacent_to_reached() {
    let map: FloorMap = [
        ((0, 0), FloorCell::ReachableOpen),
        ((1, 0), FloorCell::ReachableBlocked),
    ].into();
    assert!(check_connectivity(&map));
}

#[test]
fn connectivity_furniture_not_adjacent_to_reached() {
    let map: FloorMap = [
        ((0, 0), FloorCell::ReachableOpen),
        ((1, 0), FloorCell::Blocked),
        ((2, 0), FloorCell::ReachableBlocked),
    ].into();
    assert!(!check_connectivity(&map));
}

#[test]
fn connectivity_furniture_reachable_via_open() {
    let map: FloorMap = [
        ((0, 0), FloorCell::ReachableOpen),
        ((1, 0), FloorCell::Open),
        ((2, 0), FloorCell::ReachableBlocked),
    ].into();
    assert!(check_connectivity(&map));
}

// ---------------------------------------------------------------------------
// placement_keeps_connectivity
// ---------------------------------------------------------------------------

#[test]
fn placement_blocks_corridor() {
    let map: FloorMap = [
        ((0, 0), FloorCell::ReachableOpen),
        ((1, 0), FloorCell::Open),
        ((2, 0), FloorCell::ReachableOpen),
    ].into();
    assert!(!placement_keeps_connectivity(&[((1, 0), FloorCell::Blocked)], &map));
}

#[test]
fn placement_furniture_with_adjacency() {
    let map: FloorMap = [
        ((0, 0), FloorCell::ReachableOpen),
        ((1, 0), FloorCell::Open),
        ((2, 0), FloorCell::Open),
    ].into();
    assert!(placement_keeps_connectivity(&[((2, 0), FloorCell::ReachableBlocked)], &map));
}

// ---------------------------------------------------------------------------
// try_place_at_wall_slot — bed
// ---------------------------------------------------------------------------

#[test]
fn bed_placement_basic() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 6));
    let interior = rect.shrink(1);
    let floor_map = map_with_entrances(&interior, &[(3, 1)]);

    let slots = wall_slots(&interior);
    let item = test_bed();
    let result = slots.iter()
        .find_map(|slot| try_place_at_wall_slot(&item, slot, &interior, &floor_map, &OccupancyMap::new(), 64));
    assert!(result.is_some());
    assert_eq!(result.unwrap().len(), 2);
}

#[test]
fn bed_impossible_in_1x1_interior() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(2, 2));
    let interior = rect.shrink(1);
    let floor_map = open_map(&interior);

    let slots = wall_slots(&interior);
    let item = test_bed();
    let result = slots.iter()
        .find_map(|slot| try_place_at_wall_slot(&item, slot, &interior, &floor_map, &OccupancyMap::new(), 64));
    assert!(result.is_none());
}

#[test]
fn bed_avoids_disconnecting_entrances() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(4, 2));
    let interior = rect.shrink(1);
    let floor_map = map_with_entrances(&interior, &[(1, 1), (3, 1)]);

    let slots = wall_slots(&interior);
    let item = test_bed();
    let result = slots.iter()
        .find_map(|slot| try_place_at_wall_slot(&item, slot, &interior, &floor_map, &OccupancyMap::new(), 64));
    assert!(result.is_none());
}

// ---------------------------------------------------------------------------
// try_place_at_wall_slot — single items
// ---------------------------------------------------------------------------

#[test]
fn chest_placement_basic() {
    let interior = Rect2D::from_points(Point2D::new(1, 1), Point2D::new(5, 5));
    let floor_map = map_with_entrances(&interior, &[(1, 3)]);
    let slots = wall_slots(&interior);
    let item = test_chest();
    let result = slots.iter()
        .find_map(|slot| try_place_at_wall_slot(&item, slot, &interior, &floor_map, &OccupancyMap::new(), 64));
    assert!(result.is_some());
    assert_eq!(result.unwrap().len(), 1);
}

#[test]
fn placement_skips_blocked_cell() {
    let map: FloorMap = [((1, 1), FloorCell::Blocked)].into();
    let interior = Rect2D::from_points(Point2D::new(1, 1), Point2D::new(1, 1));
    let slots = wall_slots(&interior);
    let item = test_chest();
    let result = slots.iter()
        .find_map(|slot| try_place_at_wall_slot(&item, slot, &interior, &map, &OccupancyMap::new(), 64));
    assert!(result.is_none());
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
// fill_ratio
// ---------------------------------------------------------------------------

#[test]
fn fill_ratio_empty_room() {
    let interior = Rect2D::from_points(Point2D::new(1, 1), Point2D::new(4, 4));
    let map = open_map(&interior);
    assert!(fill_ratio(&map) < 0.01);
}

#[test]
fn fill_ratio_half_filled() {
    let mut map: FloorMap = HashMap::new();
    for x in 0..4 {
        map.insert((x, 0), FloorCell::Open);
        map.insert((x, 1), FloorCell::ReachableBlocked);
    }
    assert!((fill_ratio(&map) - 0.5).abs() < 0.01);
}

#[test]
fn fill_ratio_empty_map() {
    let map: FloorMap = HashMap::new();
    assert!(fill_ratio(&map) < 0.01);
}

// ---------------------------------------------------------------------------
// data loading — resolve_furniture_list
// ---------------------------------------------------------------------------

#[test]
fn resolve_furniture_from_defs() {
    let mut items: HashMap<String, FurnitureItemDef> = HashMap::new();
    items.insert("bed".into(), FurnitureItemDef {
        name: "bed".into(),
        unique: true,
        blocks: vec![super::data::PlacedBlockDef {
            block: "minecraft:red_bed[part=head]".into(),
            offset: [0, 0, 0],
            layer: BlockLayer::Ground,
        }],
        constraints: vec![super::data::PlacedConstraintDef {
            offset: [0, 0],
            constraint: CellConstraint::Wall,
            facing: FacingMode::AwayFromWall,
        }],
    });
    items.insert("lantern".into(), FurnitureItemDef {
        name: "lantern".into(),
        unique: true,
        blocks: vec![super::data::PlacedBlockDef {
            block: "minecraft:lantern[hanging=true]".into(),
            offset: [0, 0, 0],
            layer: BlockLayer::Ceiling,
        }],
        constraints: vec![],
    });

    let mut rooms: HashMap<String, RoomFurnitureDef> = HashMap::new();
    rooms.insert("bedroom".into(), RoomFurnitureDef {
        room_type: "bedroom".into(),
        required: vec!["bed".into()],
        optional: vec!["lantern".into()],
    });

    let fl = resolve_furniture_list("bedroom", &rooms, &items);
    assert_eq!(fl.required.len(), 1);
    assert_eq!(fl.required[0].name, "bed");
    assert_eq!(fl.optional.len(), 1);
    assert_eq!(fl.optional[0].name, "lantern");
}

#[test]
fn resolve_missing_room_returns_empty() {
    let items: HashMap<String, FurnitureItemDef> = HashMap::new();
    let rooms: HashMap<String, RoomFurnitureDef> = HashMap::new();
    let fl = resolve_furniture_list("nonexistent", &rooms, &items);
    assert!(fl.required.is_empty());
    assert!(fl.optional.is_empty());
}

#[test]
fn resolve_skips_unknown_items() {
    let items: HashMap<String, FurnitureItemDef> = HashMap::new();
    let mut rooms: HashMap<String, RoomFurnitureDef> = HashMap::new();
    rooms.insert("test".into(), RoomFurnitureDef {
        room_type: "test".into(),
        required: vec!["nonexistent_item".into()],
        optional: vec![],
    });
    let fl = resolve_furniture_list("test", &rooms, &items);
    assert!(fl.required.is_empty());
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
