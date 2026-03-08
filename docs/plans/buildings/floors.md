# Floors Module

Places floor and ceiling surfaces for each story. Handles vertical circulation (stairs/ladders).

## Input
- `Frame` (footprint polygon + floor count + wall heights + base Y)

## Output
- `()` (modifies world directly)

## Floor Surface Placement

Each floor level gets a solid layer of blocks filling the interior of the footprint polygon.

**Ground floor (floor 0):** The surface sits at `base_y`, which is the Y the foundation prepared.
The foundation already placed blocks below this level, so the ground floor surface is the
first walkable layer.

**Upper floors (floor 1+):** The surface sits at `frame.floor_y(floor)`. This layer
doubles as the ceiling of the floor below — there is no separate ceiling. A single shared slab
avoids the awkward 1-block gap that separate floor/ceiling layers create, and matches how
real Minecraft builds look.

```
  floor 2 surface / floor 1 ceiling ──→  ████████████   y = base_y + 2 * wall_height
                                          |          |
                                          | floor 1  |   wall_height blocks of air
                                          |          |
  floor 1 surface / floor 0 ceiling ──→  ████████████   y = base_y + wall_height
                                          |          |
                                          | floor 0  |   wall_height blocks of air
                                          |          |
  ground floor surface ──────────────→   ████████████   y = base_y
  foundation ────────────────────────→   ▓▓▓▓▓▓▓▓▓▓▓▓
```

The floor surface is placed at the Y coordinate for that level. The walkable air space spans
`wall_height - 1` blocks above the surface (the surface block itself is one of the `wall_height`
blocks). The walls module fills the perimeter columns for the same vertical extent, so their
tops line up with the next floor surface.

**Placement iteration:** For each floor level, iterate over all blocks inside the footprint
polygon at that level's Y. Use the footprint's interior fill (not just the perimeter) to get
the set of (x, z) positions. Place a block at each position. Where two cells share an edge,
the existing code already extends the floor through the shared wall — this continues unchanged.

**Top ceiling:** The topmost floor has no floor above it. The roof module handles the top
surface. The floors module does not place anything above the highest floor's air space.

## Stair Placement

Stairs connect adjacent floors. Each staircase occupies a rectangular stairwell area on the
floor it starts from and cuts a matching opening in the floor above.

### Choosing stair position

1. **Wall-adjacent placement.** Stairs go against an interior wall, not in the center of the
   room. Pick a wall segment that is not an exterior door wall. Prefer walls on the longer axis
   of the footprint so the stair run fits naturally.

2. **Corner bias for small buildings.** For single-cell or small footprints, place stairs in
   a corner. This wastes the least usable floor area.

3. **Consistent column across floors.** Multi-story buildings should stack stairs vertically
   so they don't wander around the floor plan. Pick the stair column once (on the ground
   floor) and reuse it for every floor transition. The direction can alternate (L-shaped
   switchback) if wall_height demands more run length than a straight stair provides.

### Stair geometry

A straight stair needs `wall_height` horizontal blocks of run (one step per Y level,
the top step lands on the upper floor surface). With the default `wall_height = 4`, that is
4 blocks of run in one horizontal direction.

```
  Upper floor  ████S S S S████
                       ╱  ← stair blocks (ascending)
               ████  ╱    ████
                   ╱
  Lower floor  ████████████████
                   ^ stair start
```

Each step is a stair block (`BlockForm::Stairs`) facing the ascent direction. Below the stair
blocks, upside-down stair blocks fill the underside so it looks solid from below (matching
the existing `build_stair` implementation).

### Stairwell openings

The floor surface of the upper level must have a hole where the stairs arrive. The opening
is the stair's horizontal footprint — typically 1 block wide and `wall_height - 1` blocks
long. When placing the upper floor's surface, skip any (x, z) positions that fall within a
stairwell opening.

Implementation: before placing floor blocks for a given level, collect a `HashSet<(i32, i32)>`
of stairwell opening positions. When a stair from the level below arrives at this floor,
add its landing positions to the set. Skip those positions during floor block placement.

### Ladders (single-cell buildings)

When the footprint is too small for a proper staircase (the stair run would consume most of
the floor area), use a ladder instead. A ladder occupies a single (x, z) column and a 1x1
opening in the floor above. Place the ladder blocks on a wall face, ascending through the
opening. This is a fallback — prefer stairs when space allows.

## Material Selection

Floor materials come from the building's `Palette` via `MaterialRole`.

| Floor level | Material role | Rationale |
|---|---|---|
| Ground floor | `PrimaryStone` | Stone/cobble ground level, looks like a proper foundation surface |
| Upper floors | `SecondaryWood` | Wooden plank upper floors, standard Minecraft building style |
| Stairs | `SecondaryWood` | Stair blocks match upper floor planks |
| Ladders | hardcoded `"ladder"` | Ladders are always the vanilla ladder block |

This matches the existing `build_floor` code which already uses `SecondaryWood`. The ground
floor distinction is new — it gives the building a heavier base. If the palette lacks
`PrimaryStone`, the fallback chain in `MaterialRole::backup_role` handles it.

Future option: a `Floor` material role could be added to `MaterialRole` if we want palettes
to control floor material independently, but the two-role approach above is good enough to
start.

## Interaction with Frame

The Frame provides everything the floors module needs:

- **`footprint`** — the 2D polygon. The floors module fills its interior at each level.
- **`floor_count`** — how many floor surfaces to place (including ground).
- **`wall_height`** — vertical distance between floor surfaces. Determines stair run length.
- **`base_y`** — Y coordinate of the ground floor surface.
- **Floor Y helper** — `frame.floor_y(n)` returns the world Y for that floor (accounts for
  ground_wall_height vs wall_height). The floors module calls this for each level.

The floors module does **not** modify the Frame. It reads it and places blocks.

### Ordering with other modules

```
foundation::place  →  frame::generate  →  walls::generate  →  floors::place  →  roof::place
```

Floors run after walls so that stair placement can reference wall positions (stairs go against
walls). Floors run before roof so the topmost floor surface is in place when the roof module
starts. Floors run before interior so room partitioning can account for stairwell positions.

### Data passed to interior

The interior module needs to know where stairwells are so it does not place furniture or
partitions there. The floors module should record stairwell positions and pass them along.
Options:
- Return a `FloorResult` struct containing stairwell rectangles per floor, instead of `()`.
- Or write stairwell positions into a shared context the interior module can read.

The first option (returning data) is cleaner and matches the pipeline pattern. This would
change the signature to:

```
floors::place(Frame) → FloorPlan
```

where `FloorPlan` contains the stairwell openings per level. This is a minor pipeline change
worth making.
