# Furnish Module

Places furniture in rooms based on room type. Uses connectivity-aware placement
to ensure all doors and interactable items remain reachable.

## Input
- `RoomPlan` (mutable — floor maps are updated as furniture is placed)
- `Frame` (floor_y and ceiling_y for block placement heights)
- `RNG`

## Output
- `()` (modifies world via Editor, updates RoomPlan floor maps in place)

## Furniture System

### FurnitureItem

Each piece of furniture is defined by its block, how it occupies space, and how
its facing relates to the wall:

```rust
struct FurnitureItem {
    block_id: &'static str,       // e.g. "minecraft:chest"
    placement: PlacementKind,
    facing: FacingMode,
}

enum PlacementKind {
    Bed,         // 2-block: head against wall (Blocked), foot inward (ReachableBlocked)
    WallSingle,  // 1-block against a wall (ReachableBlocked)
    Ceiling,     // hangs from ceiling center — no floor impact
}

enum FacingMode {
    None,            // no facing state (barrel, bookshelf, crafting table)
    AwayFromWall,    // faces inward (chest, furnace, smoker, loom)
    Perpendicular,   // faces along wall (anvil)
}
```

### FurnitureRequest

```rust
struct FurnitureRequest {
    item: FurnitureItem,
    required: bool,   // required items placed first; optional fill remaining space
}
```

**Fill threshold**: stops placing optional items once room is 40% occupied.

### Furniture Lists by RoomType

```
  Common:     BED* CRAFT* FURNACE* CHEST* lantern
  Hearth:     FURNACE* CRAFT* chest barrel lantern
  GreatRoom:  LANTERN* chest bookshelf
  Bedroom:    BED* chest lantern
  MasterBed:  BED* chest chest bookshelf lantern
  Storage:    barrel barrel chest chest barrel
  Kitchen:    FURNACE* SMOKER* cauldron barrel lantern
  Pantry:     barrel barrel chest barrel
  Dining:     LANTERN* crafting_table chest
  Study:      BOOKSHELF* BOOKSHELF* lantern bookshelf crafting_table
  Library:    BOOKSHELF*3 bookshelf×2 lantern
  Studio:     LOOM* crafting_table lantern
  Armory:     ANVIL* chest lantern
  MultiBed:   lantern

  * = required, lowercase = optional
```

## Placement Algorithm

### Wall Slots

Wall-adjacent positions are found from the interior rect (room rect shrunk by 1).
Cells on the edge of the interior rect are adjacent to a wall. **Corner cells
appear twice** — once for each adjacent wall direction.

Slots are **shuffled** with RNG so furniture arrangement varies between rooms.

```
  Interior rect with wall slots:

  W W W W W W W W     W = wall-adjacent (has slot)
  W . . . . . . W     . = interior (no slot)
  W . . . . . . W
  W W W W W W W W
```

### Connectivity Checking

Before placing any furniture, the algorithm verifies the placement won't break
room connectivity using BFS flood-fill:

1. **Tentative change**: apply the proposed cell state changes to a cloned FloorMap
2. **Flood fill** from the first `ReachableOpen` cell through walkable
   (Open + ReachableOpen) cells
3. **Validate**:
   - All `ReachableOpen` cells must be in the reached set
   - All `ReachableBlocked` cells must be adjacent to at least one reached cell
4. If validation fails, skip this placement and try the next slot

This prevents furniture from:
- Blocking a doorway
- Cutting off part of the room from the entrance
- Making interactable items unreachable

### Bed Placement (2 blocks)

```
  Wall slot found:

  ████████████████        Head goes against wall (Blocked)
    [HEAD][FOOT]          Foot extends inward (ReachableBlocked)
                          Player interacts from adjacent to foot
```

1. Find a wall slot where the cell is `Open`
2. Compute foot position: 1 block away from wall (into room)
3. Foot must be inside interior rect and `Open`
4. Check connectivity with changes: head→Blocked, foot→ReachableBlocked
5. Place head block (`part=head, facing=wall_dir`) and foot block (`part=foot`)
   at `floor_y`

### WallSingle Placement (1 block)

1. Find a wall slot where the cell is `Open`
2. Check connectivity with change: cell→ReachableBlocked
3. Place block with facing derived from wall direction:
   - `AwayFromWall`: facing = opposite of wall_dir (chest opens toward room)
   - `Perpendicular`: facing = wall_dir rotated 90° right (anvil along wall)
   - `None`: no facing state

### Ceiling Placement

1. Compute room center from interior rect midpoint
2. Place block at `(center.x, ceiling_y - 1, center.y)` with `hanging=true`
3. No floor map impact (ceiling items don't occupy floor space)

## Block Construction

`FurnitureItem` provides helper methods:
- `make_block(wall_dir)` → `Block` with appropriate facing states
- `make_bed_blocks(facing)` → `(head_block, foot_block)` with part + facing states

All blocks are placed via `editor.place_block()` at `floor_y` (floor level).

## File Structure

```
src/generator/buildings_v2/furnish/
    mod.rs   — FurnitureItem, PlacementKind, FacingMode, FurnitureRequest,
               wall_slots(), connectivity (flood_fill, check_connectivity),
               find_bed_placement(), find_single_placement(),
               furnish_room(), furnish_rooms()
    test.rs  — placement and connectivity tests
```
