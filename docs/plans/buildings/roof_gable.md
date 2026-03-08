# Gable Roof Implementation Plan

## Input
- `Frame` (footprint rects, floor counts, roof_y per rect, wall_height)
- `Palette` (PrimaryRoof for surface, PrimaryWall for gable triangles)
- `RNG` (ridge axis tiebreak when rect is square)

## Data Structures

### GablePitch
```rust
enum GablePitch {
    Slab,    // 0.5 rise per horizontal block
    Stairs,  // 1.0 rise per horizontal block
    Double,  // 2.0 rise per horizontal block
}
```

### RoofHeightmap
```rust
struct RoofHeightmap {
    min_x: i32,
    min_z: i32,
    width: usize,
    depth: usize,
    heights: Vec<f32>,   // row-major [x][z], NEG_INFINITY = no roof
}
```

Methods:
- `new(min_x, min_z, width, depth)` — init all to NEG_INFINITY
- `get(x, z) -> f32`
- `set(x, z, value)`
- `merge_max(&mut self, other: &RoofHeightmap)` — per-cell max, expanding bounds if needed

### RoofAxis
```rust
enum RoofAxis {
    X,  // ridge runs along X (slopes fall off in Z)
    Z,  // ridge runs along Z (slopes fall off in X)
}
```

## Algorithm

### Step 1: Group Rects by Roof Level

```
groups: HashMap<i32, Vec<usize>>  // roof_y -> rect indices
```

Rects with the same `frame.roof_y(i)` share a heightmap and get merged.

### Step 2: Per-Rect Gable Heightmap

For each rect in a group:

1. **Pick ridge axis:** longer dimension. If equal, random via RNG.
2. **Compute dimensions:**
   - `short_width` = rect size along the axis perpendicular to ridge
   - `half_width` = short_width / 2 (integer division)
   - `ridge_height` = half_width as f32 * pitch
3. **Heightmap bounds:** rect expanded by 1 (overhang) in all directions.
4. **Fill heights:** For each (x, z) in bounds:
   - `dist` = perpendicular distance from nearest eave edge (the two edges parallel to ridge)
   - If outside the rect along the ridge axis: `dist` = perpendicular distance, but capped
     to create the overhang slope at gable ends too
   - `height = dist as f32 * pitch`, clamped to `ridge_height`
   - For positions in the overhang along the gable ends (past the rect along the ridge axis):
     the height continues at the same slope as the main roof at that perpendicular distance

### Step 3: Merge Heightmaps per Group

For all rects in the same roof_y group:
1. Start with a combined heightmap covering the union of all per-rect bounds.
2. For each rect's heightmap, `merge_max` into the combined one.

### Step 4: Place Roof Blocks

For each (x, z) in the merged heightmap where height > NEG_INFINITY:

1. `h = heightmap.get(x, z)`
2. `y_base = roof_y` (the group's shared roof_y)
3. **Wall check:** if (x, z) is inside any rect from a DIFFERENT group with a higher
   roof_y, and `y_base + h` would be below that rect's roof_y, skip. Walls win.
4. **Higher-floor check:** if (x, z) is inside any rect from a HIGHER group
   (i.e., a rect whose roof_y > this group's roof_y), skip entirely.
   This prevents a lower roof from filling into upper-floor rooms.
5. **Determine if overhang:** (x, z) is outside all rects in this group.
6. **Place fill blocks** (only inside footprint, not overhang):
   - Full blocks from `y_base` up to `y_base + floor(h) - 1`
7. **Place surface block** at `y_base + floor(h)`:
   - Determine slope direction (the cardinal direction of steepest descent).
   - If `frac(h) == 0.0` and at ridge: top slab
   - If `frac(h) == 0.0` and not ridge: stair facing downhill
   - If `frac(h) == 0.5`: top slab at `y_base + floor(h)`
   - For pitch 2.0: two blocks per step. At each position place a stair at the
     appropriate Y, with a full block below it if needed.

#### Block placement by pitch

**Pitch 0.5 (Slab):**
```
dist from eave:  0     1     2     3     4(ridge)
height:          0.0   0.5   1.0   1.5   2.0
surface block:   stair slab  stair slab  slab(ridge)
y offset:        0     0     1     1     2
```
Alternates between stairs (at integer heights) and slabs (at half heights).

**Pitch 1.0 (Stairs):**
```
dist from eave:  0     1     2     3     4(ridge)
height:          0.0   1.0   2.0   3.0   4.0
surface block:   stair stair stair stair slab(ridge)
y offset:        0     1     2     3     4
```
One stair per step. Clean and regular.

**Pitch 2.0 (Double):**
```
dist from eave:  0     1     2     3(ridge)
height:          0.0   2.0   4.0   6.0
surface block:   stair stair stair slab(ridge)
y offset:        0     2     4     6
fill below:      none  1blk  3blk  5blk
```
Stair at the top of each 2-block step, full blocks filling below.

### Step 5: Place Gable Walls

For each rect, at the two gable ends (the short-axis edges):

1. The gable triangle spans from roof_y up to roof_y + ridge_height.
2. At each (x, z) along the gable edge, the height at that position defines
   the triangle boundary.
3. Fill the triangle with PrimaryWall material (full blocks).
4. The gable wall is 1 block thick, placed at the rect's short-axis edge.

## Wall Precedence Rule

When placing any roof block for a group at roof_y:
- Check all rects in groups with a HIGHER roof_y
- If (x, z) is inside any such rect, skip the position entirely
- This prevents a 1-floor wing's roof from clipping into a 2-floor core's walls/rooms

## File Structure

```
src/generator/buildings_v2/roof/
    mod.rs        — place_roof entry point, grouping logic, wall precedence check
    heightmap.rs  — RoofHeightmap struct and merge
    gable.rs      — gable_heightmap generation + gable wall placement
    blocks.rs     — heightmap-to-block conversion and placement
```

## Implementation Order

1. `heightmap.rs` — RoofHeightmap with new/get/set/merge_max
2. `gable.rs` — single-rect gable heightmap generation
3. `blocks.rs` — convert heightmap to placed blocks (stairs, slabs, fill)
4. `mod.rs` — group rects, merge per group, place, gable walls
5. Tests — single rect gable, L-shape composite, wall precedence check
