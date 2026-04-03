# Foundation Module

Prepares the terrain under the footprint for building placement. Bridges the gap between
natural (uneven) terrain and a flat building base.

## Input
- `Footprint` — 2D polygon (set of `Point2D`) defining the building outline
- `&Editor` — access to world heightmaps and block placement
- `&LoadedData` — access to `Materials` for block placement via `Placer`
- `&Palette` — material selection (PrimaryStone, SecondaryStone)

## Output
- `i32` — the chosen `base_y` so the frame module knows where the building starts
- Side effect: modifies world via `Editor` (fills, cuts, places foundation course)
- Side effect: updates `World` heightmap via `set_heights` so later modules see the leveled ground

## Core Problem

Minecraft terrain is rarely flat. A building footprint might span a 6-block height difference.
The foundation module must decide a single base Y for the building, then make the world match:
fill in low spots and cut high spots.

## Structs

```rust
/// Result of analyzing the terrain under a footprint.
struct TerrainProfile {
    /// Height at each footprint point (Point2D -> y).
    heights: HashMap<Point2D, i32>,
    min_height: i32,
    max_height: i32,
    /// The chosen Y level for the building floor.
    base_y: i32,
}

/// Describes what to do at each column under the footprint.
enum ColumnAction {
    /// Terrain is at or above base_y. Cut down to base_y (place air above, surface block at base_y - 1).
    Cut { terrain_y: i32 },
    /// Terrain is below base_y. Fill with blocks up to base_y.
    Fill { terrain_y: i32 },
}
```

## Implementation Steps

### Step A: Terrain sampling + base_y selection
Create `src/generator/buildings_v2/foundation/mod.rs` with:
- `TerrainProfile` struct
- `fn analyze_terrain(footprint: &Footprint, world: &World) -> TerrainProfile`
  - Samples `get_ocean_floor_height_at` for each `footprint.filled_points()`
  - Computes min, max, slope
  - Chooses `base_y` via median (slope <= 3) or 75th percentile (slope >= 4)
- Wire up `pub mod foundation;` in `buildings_v2/mod.rs`
- Pure computation, no block placement — just reads from `World`.

### Step B: Fill and cut
Add to foundation module:
- `ColumnAction` enum (Cut / Fill)
- `fn classify_columns(profile: &TerrainProfile) -> HashMap<Point2D, ColumnAction>`
- `async fn execute_columns(...)` — places blocks:
  - **Cut:** place air from `base_y` to `terrain_y`, copy surface block to `base_y - 1`
  - **Fill:** fill solid from `terrain_y` to `base_y - 1` using palette stone

### Step C: Foundation course + edge stairs
Add `async fn place_foundation_course(...)`:
- Compute edge set of footprint points (points with a neighbor outside the footprint)
- Interior points: `PrimaryStone` full block at `base_y - 1`
- Edge points: `SecondaryStone` upside-down stair at `base_y - 1`, facing outward
- Determine outward facing by checking which neighbor is outside the footprint

### Step D: Heightmap update + public API
- After all placement, build `HashSet<Point3D>` with `(x, base_y, z)` for every
  footprint point, call `world.set_heights(&points)`
- Wire up the public entry point:
  ```rust
  pub async fn place_foundation(
      editor: &Editor, footprint: &Footprint,
      data: &LoadedData, palette: &Palette, rng: &mut RNG,
  ) -> i32
  ```
  Calls A → B → C → heightmap update, returns `base_y`.

## Algorithm

### Step 1: Sample terrain heights

For every `Point2D` in the footprint, query `world.get_ocean_floor_height_at(point)` to get
the solid ground level (ignoring water). Then pass through `get_non_tree_height` logic to
skip tree canopies. Record all heights.

```
heights = footprint.filled_points().iter()
    .map(|p| (p, world.get_ocean_floor_height_at(p)))
    .collect()
min_height = heights.values().min()
max_height = heights.values().max()
slope = max_height - min_height
```

### Step 2: Choose base Y

The base Y is the level the building floor sits on. Options ranked by preference:

1. **Flat or gentle slope (slope <= 3):** Use the **median height** of the footprint points.
   The median is better than the mean because it avoids being pulled by outlier columns
   (e.g. one corner dipping into a ravine). Rounds to the nearest integer.

2. **Moderate to steep slope (slope >= 4):** Use the **75th percentile height**.
   This biases toward the higher side so the building "cuts into" the hill rather than
   leaving a large gap on the low side. Less fill material is needed.

```
base_y = match slope {
    0..=3 => median(heights),
    _ => percentile(heights, 75),
}
```

### Step 3: Classify each column

For each footprint point, determine the action:

```
for (point, terrain_y) in heights {
    let diff = base_y - terrain_y;
    action = match diff {
        ..=-1 => ColumnAction::Cut { terrain_y },   // terrain is above base
        _ => ColumnAction::Fill { terrain_y },       // terrain is at or below base
    }
}
```

### Step 4: Execute column actions

**Cut (terrain above base_y):**
- Place air from `base_y` up to `terrain_y` (clears terrain above the building floor).
- Place the original surface block (grass, dirt, etc.) at `base_y - 1` so the floor
  sits on natural-looking ground.

**Fill (terrain below base_y):**
- Get the surface block at the current terrain level.
- Fill solid blocks from `terrain_y` up to `base_y - 1`.
- Use the surface block for the top layer, dirt/stone below (matching the existing
  `foundation.rs` pattern of using `PrimaryStone` / `SecondaryStone` from the palette).
- Edge columns use `SecondaryStone` with upside-down stairs facing outward (same as v1).

### Step 5: Place foundation course

A single layer of stone at `base_y - 1` under the entire footprint. This is the visible
"foundation ring" that most Minecraft builds have.

- Interior footprint points: palette `PrimaryStone` full blocks.
- Edge footprint points: palette `SecondaryStone` with upside-down stairs facing outward
  (same pattern as existing `foundation.rs`).
- Outer edge (1 block beyond footprint): optional upside-down stair lip for visual detail.

### Step 6: Update heightmap

After all block placement, call `world.set_heights(&points)` with a `HashSet<Point3D>`
where each point has `(x, base_y, z)` for every footprint column. This ensures later
pipeline stages (frame, walls, roof) see the leveled ground.

```
Terrain cross-section (slope, with fill):

        natural terrain
           /
    ______/
   |######|              <- foundation course at base_y - 1
   |######|              <- filled columns
   |######|
   --------
   footprint
```

### Future: Terrain smoothing (deferred)

The terrain just outside the footprint may have sharp cliffs where we cut or filled.
A future pass could smooth this with a transition zone: collect points in a margin ring,
linearly interpolate heights toward `base_y`, and apply with `force_height`.

## Interaction with Editor / World

- **Reading terrain:** `world.get_ocean_floor_height_at(Point2D)` for solid ground height
  (ignoring water), `world.get_block(Point3D)` to sample surface block type.
- **Placing blocks:** `editor.place_block(&block, point)` for normal placement,
  `editor.place_block_forced(&block, point)` to overwrite existing blocks (needed for cut).
- **Clearing terrain:** Place `air` blocks above `base_y` inside the footprint using
  `place_block_forced`.
- **Updating heightmap:** After all placement, call `world.set_heights(&points)` with a
  `HashSet<Point3D>` where `point.y` is the new height at `(point.x, point.z)`.
- **Materials:** Get `PrimaryStone` and `SecondaryStone` from the `Palette` via
  `MaterialPlacer` (same pattern as existing v1 code — needs `&LoadedData` for `Placer::new`).

## Edge Cases

- **Footprint over water:** `get_ocean_floor_height_at` already handles this — it returns
  the solid ground level below water. Fill from the ocean floor up, building a solid platform.
- **Footprint on cliff edge:** The 75th-percentile base_y biases toward the high side.
  The low side gets filled solid. Large drops may look heavy but are correct for now.
- **Tiny height difference (0-1 blocks):** Skip fill/cut. Just place the foundation course.
- **Trees on the footprint:** Use `get_non_tree_height` logic so tree canopies don't
  inflate the terrain sample. Clear tree blocks during the cut step.

## Function Signature

```rust
pub async fn place_foundation(
    editor: &Editor,
    footprint: &Footprint,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
) -> i32  // returns base_y so frame module knows where the building starts
```

Takes a `&Footprint` (the polygon struct) and uses `footprint.filled_points()` internally
to get the set of points to operate on. Needs `&LoadedData` for `Placer::new` when
creating `MaterialPlacer`s. Returns `base_y` so the frame module knows where the building
floor starts.

## Door Accessibility

The foundation picks `base_y` purely from terrain analysis — it does not consider door
placement. Instead, door placement adapts to the foundation result downstream:

1. **Foundation** chooses `base_y` based on terrain profile (this module).
2. **Walls** scores door placement by two factors:
   - Proximity to plot edge / road (existing logic).
   - **Terrain height match:** sample a few points just outside each candidate wall segment
     and compare to `base_y`. Segments where the outside terrain is close to `base_y` score
     higher. A door facing a 4-block drop is penalized; a door facing level ground is ideal.
3. **Exterior** handles any remaining gap. If the best door still has a 1-3 block height
   difference from outside terrain, the exterior module places a short staircase or sloped
   path from the door down to natural grade.

This keeps each module focused: foundation levels the site, walls pick the best door given
the result, exterior patches the last-mile connection. No single door warps the entire
building's level.

## Future Work

- **Stilts:** For steep slopes (e.g. slope > 6), use pillars at corners/edges instead of
  solid fill. Place horizontal beams connecting stilt tops. Leave interior open underneath.
- **Terrain smoothing:** Smooth the terrain in a margin ring around the footprint to avoid
  sharp cliffs at cut/fill boundaries.
