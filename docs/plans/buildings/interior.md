# Interior Module

Fills the inside of the building with room partitions, furniture, and details.

## Input
- `Frame` (footprint polygon + floor count + wall heights + base Y)
- `WallSegments` (door and window positions with their world coordinates and facing directions)
- `FloorPlan` (stairwell positions per floor, from the floors module)
- `BuildingType` (house, shop, tavern, etc. — drives room assignment and furniture)
- `Palette`
- `RNG`

The `WallSegments` tell us exactly which walls have exterior doors and windows, and in
which `Cardinal` direction. This is critical for:
- Knowing where the main entrance is (door closest to plot edge)
- Not placing furniture that blocks door openings or windows
- Orienting rooms so that windowed walls feel natural (living spaces toward windows)

The `FloorPlan` tells us where stairwells are so we do not partition through them or
place furniture on top of stairs.

## Output
- `()` (modifies world directly via `Editor`)

## Coordinate System

The interior module works in world coordinates. The `Frame` provides:
- `frame.footprint` — the 2D polygon. Interior space is everything inside it.
- `frame.floor_y(n)` — Y coordinate of each floor surface.
- `frame.ceiling_y(n)` — Y coordinate of each floor's ceiling.
- `frame.wall_height_for(n)` — air blocks per floor (default 4).

The interior space on a given floor is the set of (x, z) points inside the footprint
polygon, at Y levels from `floor_y + 1` to `ceiling_y`. Walls are 1 block thick along
the footprint edges, so the interior is inset by 1 block from the polygon boundary.

```
Floor interior (top-down, y-slice through a 10x8 footprint):
  W W W W W W W W W W      W = exterior wall (placed by walls module)
  W . . . . . . . . W      . = interior floor space
  W . . . . . . . . W
  W . . . . . . . . W      Height: wall_height blocks of air (default 4)
  W . . . . . . . . W
  W . . . . . . . . W
  W . . . . . . . . W
  W W W W W W W W W W
```

## Room Partitioning

### When to Partition

Not every building needs room partitions. Decision tree:
- **1 cell, 1 floor**: No partitions. Treat the entire interior as one room.
- **1 cell, 2+ floors**: Each floor is its own room. No horizontal partitions.
- **2+ cells, 1 floor**: Partition if building type wants multiple rooms (house: yes, shop: no).
- **2+ cells, 2+ floors**: Partition each floor independently.

### Algorithm: Rectangular Subdivision

The footprint polygon is decomposed into rectangles (via `footprint.to_rects()`).
Each rectangle on each floor becomes a candidate room. For small buildings (1 rectangle),
we may subdivide further. For multi-rectangle footprints (L/T/U shapes), each wing
is naturally its own zone.

1. **Get rectangles**: `frame.footprint.to_rects()` gives the rectangular sub-sections.
2. **Per floor, per rectangle**: For each floor level and each rect, create a room candidate.
3. **Identify fixed areas**: Mark stairwell positions (from `FloorPlan`) and entrance area
   (from `WallSegments` door positions) as pre-assigned.
4. **Subdivide large rectangles**: If a rectangle is larger than ~8x8, split it along its
   longer axis with an interior wall. This creates rooms of reasonable size.
5. **Assign room types**: Based on `BuildingType`, assign each room a type (kitchen,
   bedroom, etc.) following the rules below.

For a simple rectangular house (10x8):
```
  W W W W W W W W W W
  W Kitchen |Living  W     Interior wall splits the floor
  W         |        W     Door cell → Living room
  W         |        W
  W W W W W W W W W W
```

For an L-shaped house (two rects):
```
  W W W W W W W
  W  Bedroom  W
  W           W W W W W W
  W           | Entry    W     Wing rect → Entry
  W W W W W W W W W W W W     Main rect → Bedroom
```

Interior walls are 1 block thick, placed along one axis of the rectangle. A 1x2 air gap
in the wall serves as an interior doorway.

### Room Types by Building Type

**House**:
- Ground floor: Entry/living room (door cell), kitchen (1 cell), storage (optional)
- Upper floors: Bedrooms, study

**Shop**:
- Ground floor: Shopfront (door cell, open), back storage
- Upper floors: Living quarters (bedroom)

**Tavern**:
- Ground floor: Main hall (door cell + adjacent, multi-cell), kitchen, storage
- Upper floors: Guest rooms (1 cell each)

**Blacksmith**:
- Ground floor: Workshop (door cell, open), storage
- Upper floors: Living quarters

### Connectivity: Interior Doors

Every room must be reachable from the main entrance. Rules:
1. The entrance cell connects to the exterior door (already has an opening in the wall).
2. Adjacent rooms share a cell boundary. Place a 1-wide, 2-tall opening in the shared
   wall between them. Position the opening at the center of the shared edge.
3. For rooms on upper floors, the stair cell provides vertical connectivity.
4. Validate: flood-fill from the entrance cell. Every room must be reachable. If not,
   add a door between the disconnected room and its nearest connected neighbor.

Interior doors are simpler than exterior doors -- just a 1x2 air gap in the interior wall.
No actual door block is needed (open archways work well for procedural buildings).

```
  Interior door placement between two cells:

  Cell A          Cell B
  . . . . . W . . . . .
  . . . . . W . . . . .
  . . . . .   . . . . .    <- door (air, y+0)
  . . . . .   . . . . .    <- door (air, y+1)
  . . . . . W . . . . .
  . . . . . W . . . . .
```

## Furniture Placement

### Strategy

Furniture is placed per-room after partitions are finalized. Each room type has a
**furniture list** with required and optional items. Placement uses a simple wall-hugging
algorithm:

1. **Identify wall-adjacent positions**: For each interior block adjacent to a wall or
   interior partition, record the position and which direction faces the wall.
2. **Place required furniture first**: Iterate the required list in priority order. For
   each item, find a valid wall-adjacent position that does not overlap existing furniture
   or block doors/windows. Place it.
3. **Fill with optional furniture**: Same process with optional items until the room feels
   furnished or positions run out.
4. **Center items**: Some items (tables, rugs) go in the room center rather than against
   walls. Place these after wall items.

### Clearance Rules

- Never place furniture in the 1-block-wide path in front of a door (exterior or interior).
- Never place tall furniture (2 blocks) directly in front of a window.
- Leave at least a 1-block-wide walkable path through the room.

### Furniture by Room Type

**Bedroom**:
- Required: Bed (2 blocks long, against wall, head toward wall)
- Optional: Chest, crafting table, bookshelf, carpet/rug, flower pot

**Kitchen**:
- Required: Furnace/smoker, crafting table
- Optional: Barrel, cauldron (water), flower pot, chest

**Living Room / Entry**:
- Required: None
- Optional: Table (fence + pressure plate or carpet), chairs (stairs facing table),
  bookshelf, flower pot, painting (wall), carpet

**Storage**:
- Required: Chest or barrel
- Optional: More chests, barrels, cobwebs (if basement)

**Shop Front**:
- Required: Counter (slab or stairs line), display (item frames on wall or chests)
- Optional: Barrel, sign

**Tavern Main Hall**:
- Required: Tables (fence + pressure plate, multiple), chairs (stairs blocks)
- Optional: Bar counter (slab line along one wall), barrel behind bar, cake, flower pots

**Workshop (Blacksmith)**:
- Required: Blast furnace or furnace, anvil, cauldron (lava)
- Optional: Chest, smithing table, grindstone, barrel

### Minecraft Furniture Blocks Reference

These are actual Minecraft blocks used to simulate furniture:

| Furniture       | Blocks Used                                                   | Placement Notes                          |
|----------------|---------------------------------------------------------------|------------------------------------------|
| Bed            | `*_bed` (color variants)                                      | 2 blocks, directional, needs floor       |
| Table          | `oak_fence` + `*_pressure_plate` or `*_carpet` on top         | 1x1, place fence then plate on top       |
| Chair          | `*_stairs` (facing direction of table)                        | Directional, set `facing` property       |
| Chest          | `chest`                                                       | Directional, faces player on placement   |
| Barrel         | `barrel`                                                      | Directional, can face any way            |
| Furnace        | `furnace` / `smoker` / `blast_furnace`                        | Directional                              |
| Crafting Table | `crafting_table`                                              | No direction                             |
| Anvil          | `anvil`                                                       | Directional                              |
| Bookshelf      | `bookshelf` / `chiseled_bookshelf`                            | No direction, place against wall         |
| Flower Pot     | `potted_*` (e.g. `potted_poppy`)                              | Place on top of solid block              |
| Painting       | N/A -- use item frames or banners on wall                     | Wall-mounted entity, may skip for now    |
| Counter        | `*_slab[type=bottom]` or `*_stairs[half=top]` in a line       | Place at y+1, looks like a counter       |
| Cauldron       | `water_cauldron` / `lava_cauldron`                            | No direction                             |
| Rug/Carpet     | `*_carpet` (color variants)                                   | Place on floor, 0 height                 |
| Lantern        | `lantern` / `soul_lantern`                                    | Floor or ceiling (`hanging=true`)        |
| Smithing Table | `smithing_table`                                              | No direction                             |
| Grindstone     | `grindstone`                                                  | Directional, wall/floor/ceiling          |
| Cake           | `cake`                                                        | No direction, place on solid block       |
| Sign           | `*_wall_sign` / `*_sign`                                      | Directional, use for shop labels         |

### Block Placement Details

All furniture blocks are placed using `editor.place_block()` or
`editor.place_block_forced()`. Directional blocks need the `facing` property set via
the block's state string (e.g. `oak_stairs[facing=west,half=bottom]`).

Beds require two blocks placed in sequence: the foot and head parts, with the `part`
and `facing` properties set correctly.

Multi-block furniture (tables = fence + plate) requires placing blocks bottom-up.

## Interior Lighting

### Strategy

Every room needs at least one light source. The goal is to prevent mob spawning inside
buildings (light level 0 = mob spawns in Java 1.18+, so any light source works).

### Placement Rules

1. **One light per room minimum**: Place a lantern or torch in every room.
2. **Prefer ceiling lanterns**: Place `lantern[hanging=true]` on the ceiling (y = floor_y + 4).
   Center it in the room if possible.
3. **Wall torches as backup**: If ceiling placement is blocked (e.g. by stairs above),
   place `wall_torch[facing=*]` on a wall at y+2 above floor.
4. **Fireplaces for large rooms**: In living rooms or tavern halls with an exterior wall,
   optionally build a fireplace:
   - 3 blocks wide, 2 blocks tall niche in the wall
   - `campfire` or `soul_campfire` at the base
   - `brick` or `stone_brick` surround
   - Only on ground floor, only on exterior walls (check `WallPlacement` to avoid
     putting a fireplace where a window or door is)

### Light Level Check

After placing all lights, no interior block at floor level should be more than 6 blocks
(Manhattan distance) from a light source. If it is, add another light. This is a simple
validation pass -- iterate all floor blocks, check distance to nearest light, add torches
where needed.

## Using WallSegments and FloorPlan

**WallSegments** provides:
- `doors()` — iterator over all door openings with world positions and facing directions.
- `windows()` — iterator over all window openings with world positions.
- `segments_on_floor(floor)` — all wall segments for a given floor level.

How interior uses this:
1. **Door positions** identify "entry" rooms. Furniture must not block the doorway.
2. **Window positions** inform furniture placement — do not place tall furniture (bookshelves)
   on wall faces that have windows. Prefer placing desks or counters under windows.

**FloorPlan** provides:
- Stairwell rectangles per floor level, output from the floors module.

How interior uses this:
1. **Stairwell areas** are reserved — no partitions or furniture placed there.
2. The stair's floor determines which room on the upper floor connects vertically.

## Implementation Sketch

```rust
pub async fn build_interior(
    editor: &Editor,
    frame: &Frame,
    wall_segments: &WallSegments,
    floor_plan: &FloorPlan,
    building_type: BuildingType,
    palette: &Palette,
    rng: &mut RNG,
) {
    let rects = frame.footprint.to_rects();

    for floor in frame.floors() {
        let floor_y = frame.floor_y(floor);
        let height = frame.wall_height_for(floor);
        let stairwells = floor_plan.stairwells_on_floor(floor);

        // 1. Subdivide rects into rooms, avoiding stairwells
        let rooms = assign_rooms(&rects, floor, building_type, wall_segments, &stairwells);

        // 2. Place interior walls between adjacent rooms
        for (room_a, room_b) in room_boundaries(&rooms) {
            place_interior_wall(editor, room_a, room_b, floor_y, height, palette, rng).await;
        }

        // 3. Place interior doors (1x2 air gaps)
        for (room_a, room_b) in room_boundaries(&rooms) {
            place_interior_door(editor, room_a, room_b, floor_y).await;
        }

        // 4. Place furniture per room
        for room in &rooms {
            let furniture_list = get_furniture_list(room.room_type, building_type);
            place_furniture(editor, room, floor_y, &furniture_list, palette, rng).await;
        }

        // 5. Place lighting
        for room in &rooms {
            place_lighting(editor, room, floor_y, height, palette, rng).await;
        }
    }
}
```

## Data-Driven Furniture Sets

Like wall components and roof sets, furniture should be data-driven. Define furniture sets
per style in JSON under `data/buildings/interiors/`:

```
data/buildings/interiors/
  furniture/
    medieval_bedroom.json      -> list of furniture items for medieval bedrooms
    medieval_kitchen.json
    medieval_tavern_hall.json
    ...
```

Each JSON file specifies required and optional items with weights, block IDs, placement
rules (wall-adjacent, center, corner), and size (1x1, 2x1, etc.). This keeps the Rust
code generic and lets us add new furniture layouts without recompiling.

## Execution Order

Interior runs after walls, floors, and roof in the pipeline. By this point:
- All exterior walls are placed (with doors and windows recorded in `WallSegments`)
- Floor surfaces are placed at each level (with stairwells recorded in `FloorPlan`)
- Stairs are placed between floors by the floors module
- The roof is done

Interior only adds blocks inside the existing shell. It never modifies exterior walls
or structural elements.
