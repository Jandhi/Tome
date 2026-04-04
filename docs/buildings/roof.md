# Roof Module

Generates gable roofs using a heightmap-based approach. Groups rects by roof level,
generates per-rect gable heightmaps, merges with max, then places blocks.

## Input
- `Frame` (footprint rects, floor counts, roof_y per rect)
- `GablePitch` (Slab, Stairs, or Double — one pitch for the whole building)
- `Palette` (PrimaryRoof for surface, PrimaryWall for gable triangles)
- `RNG` (ridge axis tiebreak when rect is square)

## Output
- `()` (modifies world directly via editor)

## Data Structures

```rust
enum GablePitch {
    Slab,    // 0.5 rise per horizontal block
    Stairs,  // 1.0 rise per horizontal block
    Double,  // 2.0 rise per horizontal block
}

enum RidgeAxis {
    X,  // ridge runs along X, slopes fall off in Z
    Z,  // ridge runs along Z, slopes fall off in X
}

struct RoofHeightmap {
    min_x: i32, min_z: i32,
    width: usize, depth: usize,
    heights: Vec<f32>,   // row-major, NEG_INFINITY = no roof
}
```

RoofHeightmap methods: `new`, `get(x,z)`, `set(x,z,v)`, `merge_max(&other)`.

## Algorithm

### Step 1: Group Rects by Roof Level

```rust
groups: BTreeMap<i32, Vec<usize>>  // roof_y → rect indices (sorted by height)
```

Rects at the same `frame.roof_y(i)` share a heightmap and get merged. BTreeMap
ensures lower roofs are processed first.

### Step 2: Pick Ridge Axes + Extend Wing Rects

**Ridge axis**: longer dimension of the rect. If square, random via RNG.

**Wing extension**: Wings with a ridge axis perpendicular to the core's are extended
inward toward the core's ridge centerline. This makes the wing's roof heightmap
seamlessly merge with the core's, avoiding a gap at the junction.

```
  Before extension:               After extension:

  ┌────────────────┐              ┌────────────────┐
  │    core        │              │    core        │
  │   ridge ═══════│              │   ridge ═══════│
  │                │              │                │
  └──────┬───┬─────┘              └──────┬───┬─────┘
         │ W │                           │ W │
         │ I │                     ┌─────┤ I ├─────┐
         │ N │                     │     │ N │     │  extended
         │ G │                     │     │ G │     │  toward core
         └───┘                     └─────┴───┴─────┘  ridge line
```

Only extends wings whose ridge axis is perpendicular to the core's. Same-axis
wings merge naturally without extension.

### Step 3: Generate Per-Rect Gable Heightmaps

For each rect in a group, using the extended rect:

1. Compute bounding box = rect ± 1 block overhang
2. For each (x, z) in the heightmap bounds:
   - `dist` = signed distance to nearest eave edge (short-axis edges)
   - `height = dist * pitch_value`
   - Negative distances (overhang zone) produce negative heights

```
  Cross-section of gable heightmap (perpendicular to ridge):

  height
    4 |         .
    3 |       .   .           Stairs pitch (1.0)
    2 |     .       .         each step = 1 block
    1 |   .           .
    0 | .               .
   -1 .                   .   ← overhang (h < 0)
      ├─┤                ├─┤
      overhang    rect    overhang
```

### Step 4: Merge Heightmaps per Group

For all rects in the same roof_y group, `merge_max` into a combined heightmap.
The max operation makes the taller roof "poke through" the shorter at intersections,
creating natural valley/ridge lines.

### Step 5: Place Gable Walls

At each rect's two gable ends (short-axis edges), fill a triangle with PrimaryWall:

```
  Gable end view:

       ╱╲
      ╱  ╲            PrimaryWall blocks fill
     ╱ ## ╲           from roof_y up to the
    ╱ #### ╲          surface height at each position
   ╱ ###### ╲
  ════════════         top of wall
```

Wall height per column depends on pitch and distance from eave. Extra blocks added
for Stairs pitch (+1) and Slab pitch at half-steps (+1).

**Wall precedence**: skips positions inside higher-floor rects to prevent a lower
roof's gable from clipping into upper-floor rooms.

### Step 6: Place Roof Blocks

For each (x, z) in the merged heightmap where height > NEG_INFINITY:

**Pitch adjustment**: Stairs pitch lowers roof_y by 1 so the first stair step
sits where the wall top ends (seamless transition).

**Block selection by pitch and position**:

```
  Pitch 0.5 (Slab):
  dist:     0     1     2     3     4(ridge)
  height:  -0.5   0.0   0.5   1.0   1.5
  block:   b.slab b.slab t.slab stair t.slab(ridge)

  Pitch 1.0 (Stairs):
  dist:     0     1     2     3     4(ridge)
  height:   0.0   1.0   2.0   3.0   4.0
  block:   stair stair stair stair slab(ridge)

  Pitch 2.0 (Double):
  dist:     0     1     2     3(ridge)
  height:   0.0   2.0   4.0   6.0
  block:   stair stair stair slab(ridge)
  fill:    none  1blk  3blk  5blk
```

**Stair facing**: computed from the heightmap gradient — stairs face the direction
of steepest ascent (toward the ridge). Uses `stair_facing()` which samples 4 neighbors.

**Ridge detection**: `is_ridge()` checks if no neighbor has a strictly higher value.
Ridges get a bottom slab cap instead of a stair.

**Fill blocks**: Inside the footprint (not overhang), fill blocks from `roof_y - 1`
up to below the surface block. Double pitch fills extra blocks below each stair.

**Overhang behavior**: Only surface blocks are placed in the overhang zone — no fill
underneath. Overhang brackets add visual support:

```
  Overhang brackets by pitch:

  Slab:    Stairs:         Double:
  ─        ─╲              ─╲
  ═         ═              ═█
            (inv stair)     ═
                           (inv stair)
```

| Pitch   | Overhang bracket                              |
|---------|----------------------------------------------|
| Slab    | Bottom slab + top slab below                  |
| Stairs  | Upside-down stair below the surface stair     |
| Double  | Block + upside-down stair below (3 blocks)    |

## Wall Precedence Rule

When placing any roof block for a group at roof_y:
- Check all rects in groups with a HIGHER roof_y
- If (x, z) is inside any such rect, skip entirely
- Prevents a 1-floor wing's roof from filling into a 2-floor core's walls/rooms

## File Structure

```
src/generator/buildings_v2/roof/
    mod.rs        — place_roof entry point, grouping, rect extension, wall precedence
    heightmap.rs  — RoofHeightmap struct (new/get/set/merge_max)
    gable.rs      — GablePitch, RidgeAxis, gable_heightmap(), place_gable_walls()
    blocks.rs     — place_roof_blocks(), stair_facing(), is_ridge()
    test.rs       — heightmap and placement tests
```

## Future Work

- **Hip roofs**: Use min-distance-to-any-edge instead of min-distance-to-eave.
  The heightmap infrastructure supports this — just a different distance function.
- **Flat roofs**: Heightmap with constant value + parapet ring.
- **Chimneys**: Vertical column punching through the heightmap.
- **Decorative gable trim**: Stair blocks tracing the gable triangle edge.
- **Ridge decorations**: Slabs or fences along the ridge line.
