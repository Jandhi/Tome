# Rooms Module

Partitions the building interior into rooms, assigns types by SizeClass, places
interior walls with archway doors, and builds walkability grids (FloorMaps) for
the furnish module.

## Input
- `Frame` (footprint + floor counts + wall height + base Y)
- `WallSegments` (door positions for entry detection)
- `FloorPlan` (stairwell positions from floors module)
- `has_attic: bool`
- `SizeClass`
- `LoadedData`, `Palette`, `RNG`

## Output
- `RoomPlan` — collection of `Room` structs with walkability grids

## Data Structures

### Room

```rust
struct Room {
    rect: Rect2D,           // footprint rect this room corresponds to
    rect_index: usize,      // index into footprint.rects()
    floor: u32,             // 0 = ground
    role: RoomRole,         // Entry, Main, Secondary, Upper, Attic
    room_type: RoomType,    // Common, Hearth, Kitchen, Bedroom, etc.
    floor_map: FloorMap,    // walkability grid for furniture placement
}
```

### RoomRole

Structural role, assigned by position relative to the entrance:

```
  ┌──────────────────┬─────────┐
  │                  │Secondary│    Ground floor:
  │     Entry        │  (wing) │    - Entry = has exterior door
  │     (core)       │         │    - Main = largest non-entry rect
  │         D ←door  │         │    - Secondary = remaining rects
  └──────────────────┴─────────┘

  ┌──────────────────┐
  │                  │              Upper floors:
  │     Upper        │              - All rooms are Upper
  │     (core)       │
  └──────────────────┘              Attic: floor >= rect's floor_count
```

### FloorCell & FloorMap

```rust
enum FloorCell {
    Open,              // empty, walkable, furniture can go here
    ReachableOpen,     // door/entrance — must be reachable via BFS
    ReachableBlocked,  // furniture (bed foot, chest) — must be adjacent to reachable
    Blocked,           // stairwells, wall remnants, bed head — impassable
}

type FloorMap = HashMap<(i32, i32), FloorCell>;
```

The floor map is the **key interface** between rooms and furnish. Furniture placement
modifies cell states and checks connectivity after each placement.

```
  FloorMap example (7x5 interior):

  . . . . . . .      . = Open
  . . . . . . .      E = ReachableOpen (entrance from door)
  E . . . . . .      B = Blocked (stairwell)
  . . B B . . .
  . . B B . . .
```

## Algorithm

### Phase 1: Assign Room Types

`assign_room_types(frame, size_class, has_attic, rng)` returns a list of
`(rect_index, floor, RoomType)` for every room including attics.

Room assignment varies by SizeClass:

**Cottage** — simple:
```
  Core: Common (all floors)
  Wings: Storage
  Attic: Storage
```

**House** — residential:
```
  1 floor, 1 rect:  Common
  1 floor, multi:   Core=Hearth, Wings=Bedroom
  Multi-floor:      Floor 0: Core=Hearth, Wings=Storage
                    Floor 1+: All Bedroom
  Attic: Storage
```

**Hall** — larger buildings:
```
  Floor 0: Core=GreatRoom
           Wings by size rank: Kitchen → Pantry → Storage
  Floor 1+: Core=MultiBedroom
            Wings by rank: MasterBedroom → Study → random(Bedroom|Storage)
  Attic: Storage
```

**Manor** — grand:
```
  Floor 0: Core=Hearth
           Wings: 50% Dining (once), else Storage
  Floor 1+: First upper=Bedroom, then random once each:
            Library(1/5), Studio(1/5), Armory(1/5), Study(1/4)
            Remaining: Bedroom
  Attic: Storage
```

### Phase 2: Place Interior Walls

For each floor, for each `RectBoundary` where both rects are active:

1. Filter boundary wall_cells that overlap the exterior perimeter (skip those)
2. Find archway position: center of interior cells, or offset if center conflicts
   with a stairwell (searches from the corner furthest from stairs)
3. Place `SecondaryWood` blocks for full wall height, leaving a 1×2 air gap
   at the archway position

```
  Interior wall between core and wing:

  ██████████████         ██ = SecondaryWood wall blocks
  ██████  ██████           = 1×2 archway gap
  ██████  ██████
  ██████████████
  ██████████████
```

### Phase 3: Build Floor Maps

For each room (rect_index, floor, room_type):

1. Shrink rect by 1 (interior only — walls are on the edge)
2. Initialize all interior cells as `Open`
3. Mark stairwell cells as `Blocked`
4. Mark interior door cells as `ReachableOpen` (clamped to interior rect)
5. Mark exterior door cells as `ReachableOpen` (step 1 block inward from door)

Entry detection: `find_entry_rect()` searches ground-floor doors, steps 1 block
inward from the door cell, and finds which rect contains that point.

## RoomType Labels

Each RoomType has a 3-letter `.label()` for ASCII diagrams:

| Type | Label | Typical location |
|------|-------|-----------------|
| Common | Com | Cottage core (all-in-one) |
| Hearth | Hrt | Ground floor core |
| GreatRoom | Grt | Hall ground core |
| Bedroom | Bed | Upper floors |
| MultiBedroom | MBd | Hall upper core |
| MasterBedroom | Mst | Hall upper wing |
| Study | Std | Upper wing |
| Storage | Sto | Wings, attic |
| Dining | Din | Manor ground wing |
| Kitchen | Kit | Hall ground wing |
| Pantry | Pnt | Hall ground wing |
| Library | Lib | Manor upper |
| Studio | Art | Manor upper |
| Armory | Arm | Manor upper |

## File Structure

```
src/generator/buildings_v2/rooms/
    mod.rs   — Room, RoomRole, FloorCell, FloorMap, RoomPlan, build_rooms(),
               assign_room_types(), interior wall placement
    test.rs  — assignment and connectivity tests
```
