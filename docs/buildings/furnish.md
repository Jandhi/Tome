# Furnish Module

Places furniture in rooms based on room type. Reads YAML data files for furniture
piece definitions and per-room item lists, then runs connectivity-aware placement
that keeps doors and interactable items reachable.

## Input
- `RoomPlan` (mutable — each `Room.constraints` and `Room.furniture` updated as items are placed)
- `Frame` (`floor_y`, `ceiling_y`, and `roof_y` for attic ceilings)
- `BuildCtx` (provides editor, palette, materials, RNG, and loaded `FurnitureData`)

## Output
- `()` — places blocks via the editor and mutates each room's `ConstraintMap` + `furniture` list

## Data Model

All furniture and room lists are loaded once at startup via
`FurnitureData::load()`:

- `data/furniture/*.yaml` — every YAML file in the directory contributes
  furniture items (currently `house.yaml`, `industrial.yaml`, `storage.yaml`).
  Item keys must be unique across files.
- `data/rooms.yaml` — per-room-type required + optional item lists.

### Furniture (`data/furniture/*.yaml`)

```yaml
chest:
  unique: false             # if true, only one instance per room
  blocks:
    - block: "minecraft:chest"
      offset: [0, 0, 0]     # [along, y, away] in wall-relative space
      layer: ground         # ground | ceiling
      swap: none            # none | wood | color (palette substitution)
  constraints:
    - offset: [0, 0]        # [along, away]
      constraint: blocked_reachable   # wall | blocked_reachable | none
      facing: away_from_wall          # none | away_from_wall | toward_wall | perpendicular
```

- **Offsets** are wall-relative: `along` follows the wall (right when looking
  away from it), `away` points into the room. They get rotated into world
  coordinates per slot via `resolve_offset`.
- **`layer`** decides what grid the block claims. `ceiling` blocks land at
  `ceiling_y - 1 + dy` and only mark the ceiling grid; `ground` blocks land at
  `floor_y + dy` and mark the ground grid `Blocked`.
- **`swap`** runs after block parsing. `wood` looks up the palette's
  `PrimaryWood` material for the block's inferred form (stairs, trapdoor, sign,
  …); `color` recolors via `palette.primary_color` (bed, carpet, banner, …).
- **`constraints`** describe what the surrounding cells must look like at the
  anchor. They are evaluated before any block is placed and applied to the
  `ConstraintMap` after the placement check passes.

Cell-state mapping (see `rooms/constraints.rs`):

| YAML constraint      | CellState produced    | Used by                       |
|----------------------|-----------------------|-------------------------------|
| `wall`               | `Blocked` on a wall edge | bed head, bookshelves, banner |
| `blocked_reachable`  | `BlockedReachable`    | chest fronts, table tops      |
| `none`               | (no change)           | placeholder/anchor only       |

Facing modes (resolved against the chosen wall direction):

| FacingMode        | Result              | Example       |
|-------------------|---------------------|---------------|
| `none`            | no facing state     | barrel, bookshelf |
| `away_from_wall`  | `-wall_dir`         | chest, furnace, smoker |
| `toward_wall`     | `wall_dir`          | bed (foot points away from head wall) |
| `perpendicular`   | `wall_dir.rotate_right()` | anvil along a wall |

### Rooms (`data/rooms.yaml`)

```yaml
bedroom:
  required: [bed, lantern]
  optional: [chest, nightstand, bookshelf, ...]

storage:
  fill_threshold: 0.82      # cap on ground-cell fill ratio
  required: [lantern]
  optional: [crate, barrel_stack, ...]
```

The `required` list is processed once in order. The `optional` list is
processed in order until the fill threshold is hit. If `fill_threshold` is set
explicitly (storage, pantry), the optional list is replayed in passes until a
full pass places nothing — packing the room as densely as the data allows.
Default threshold is `DEFAULT_FILL_THRESHOLD = 0.75`.

`RoomType::furniture_key()` maps a room type onto a key in this file; rooms
without a matching key are skipped.

## Placement Algorithm

Per-room flow (`furnish_room` in `furnish/mod.rs`):

1. Build `interior_rect` (room rect shrunk by walls) and a shuffled list of
   `WallSlot { cell, wall_dir }` covering the interior edge. Corner cells
   appear twice — once per adjacent wall.
2. Build a shuffled list of all interior cells for freestanding placement.
3. Place required items in order, then loop the optional list under the
   `fill_threshold` cap.
4. Each `try_place_item` call dispatches by item shape:
   - **Ceiling-only** (`is_ceiling_item`) — anchored at the interior midpoint
     via `try_place_ceiling`.
   - **Wall-bound** (`needs_wall`: any `wall` constraint or non-`none` facing)
     — try each wall slot until one fits, via `try_place_at_wall_slot`.
   - **Freestanding** — try every interior cell × 4 rotations, via
     `try_place_freestanding`. Block facings are rotated through `rotate_block`
     so e.g. stair tables retain their relative orientation.
5. On success, the resulting `ResolvedBlock`s get palette-swapped, written to
   the editor, and the constraint map is updated. The placed item is recorded
   on `Room.furniture` as `PlacedFurniture { name, cells }` so the blueprint
   dump can render it.

### Connectivity

Reserved cells (`BlockedReachable`) and explicit ground-block cells must not
strand any other reserved cell. `placement_keeps_connectivity`:

1. Saves the current state of every cell about to change.
2. Applies the proposed `Blocked`, `BlockedReachable`, and ground-block-cell
   updates in place. Block cells are applied last so they override any
   `BlockedReachable` on the same cell (the bed foot has both a BR constraint
   and an explicit block).
3. `check_connectivity` flood-fills from a walkable neighbor of any reserved
   cell and confirms every reserved cell has at least one walkable neighbor in
   the reached set.
4. Restores the saved states regardless of outcome — the caller mutates only
   if the check passes.

### Bed special-case

Beds use `unique: true` and a single `[part=foot]` block with
`facing: toward_wall`. Minecraft's `BedBlock.setPlacedBy` runs on the next
block update and creates the head block on the cell the foot points at — which
is the anchor cell that the wall constraint already claims. The `wall`
constraint cell isn't in the explicit block list, so `try_place_item` adds it
to the returned `cells` after placement so blueprints render the full bed.

### Attic exceptions

Rooms with `RoomRole::Attic` use `frame.roof_y(rect_index)` as their ceiling
and skip ceiling-only items entirely — the roof module handles attic lanterns
+ chains itself.

## File Layout

```
src/generator/buildings_v2/furnish/
    mod.rs   — placement algorithm, connectivity, room loop
    data.rs  — YAML structs (FurnitureData, Furniture, RoomFurnitureList) and PaletteSwap
    test.rs  — placement and connectivity tests

data/
    furniture/
        house.yaml       — domestic items (bed, chest, table, carpets, decor, …)
        industrial.yaml  — workstation items (anvil, furnace, smoker, loom, cauldron)
        storage.yaml     — bulk-storage stacks (crate, barrel_stack, hay_pile, …)
    rooms.yaml           — per-room required + optional lists
```
