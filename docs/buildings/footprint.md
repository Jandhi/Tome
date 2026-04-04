# Footprint Module

Determines the building's 2D shape and position within the plot's usable area.

## Input
- `Plot` — axis-aligned bounding rect + boolean mask for non-buildable cells (world coords)
- `SizeClass` — target area range, min side length, wing count range
- `RNG` (seeded randomness)

## Output
- `Option<Footprint>` — `None` if no valid building fits

## Entry Point

`generate_footprint(rng, plot, size_class) -> Option<Footprint>`

Wires the full pipeline: find candidate area, generate layouts, filter and select,
merge into polygon. A `debug_assert!` verifies all filled points are usable.

## Types

```rust
struct Footprint {
    /// Clockwise-ordered vertices in world coordinates.
    /// Every edge is axis-aligned (shares either x or z with the next vertex).
    vertices: Vec<Point2D>,
    /// The original rectangles (core + wings) that form this footprint.
    /// Core is always rects[0].
    rects: Vec<Rect2D>,
}

impl Footprint {
    fn bounds(&self) -> Rect2D;
    fn edges(&self) -> impl Iterator<Item = (Point2D, Point2D)>;
    fn contains(&self, point: Point2D) -> bool;
    fn filled_points(&self) -> Vec<Point2D>;
    fn vertices(&self) -> &[Point2D];
    fn rects(&self) -> &[Rect2D];
}
```

### Why a polygon instead of a cell set or bitmap

Downstream modules need geometric structure, not just a point cloud:
- **Walls** need ordered edges with lengths and facing directions.
- **Roof** needs rectangular sub-sections to build gable/hip sections that join at valleys.
- **Frame** needs corners and spans.

A polygon with axis-aligned edges gives all of this cheaply. `rects()` returns the
original rectangles for modules that think in terms of rectangular spans.
`filled_points()` gives the flat set when that's all you need (foundation, claiming).

## Size Classes

The size class is determined by building type and wealth level. It sets a target total
area for the footprint, and the algorithm tries to hit that area by placing a core
rectangle and optionally attaching wings.

| Class   | Target area | Min side | Wings | Floors | Driven by                          |
|---------|-------------|----------|-------|--------|------------------------------------|
| Cottage | 45–80       | 5        | 0-1   | 1      | Outskirts, rural, small buildings  |
| House   | 80–130      | 5        | 1-2   | 1–2    | Standard town buildings            |
| Hall    | 130–200     | 7        | 2-3   | 2–3    | Craftsmen, taverns, shops          |
| Manor   | 280–450     | 9        | 2-4   | 2–3    | Elite, landmarks                   |

Dimensions are biased toward odd numbers (5, 7, 9, 11...) so buildings have a clear
center block for doors, windows, and roof ridgelines.

## Algorithm

### Step 1: Find the largest usable rectangle (`maximal_rect.rs`)

Scan the `usable` grid using the histogram-based O(rows * cols) maximal rectangle algorithm:

1. Build a height map: for each cell `(x, z)`, count consecutive `true` cells upward
   (decreasing z). This gives a histogram per row.
2. For each row, run the largest-rectangle-in-histogram algorithm using a stack.
3. Track the globally largest rectangle found.

This rectangle becomes the **candidate area** — the biggest contiguous rectangle
we could build in. If it's smaller than `min_side x min_side`, return `None`.

### Step 2: Generate layout candidates (`generate.rs`)

Generate multiple complete layouts (core + wings) and score them as whole units rather
than greedily picking a core and then greedily picking wings. This produces better
overall results because a slightly worse core position might allow much better wings.

**Generate K core variants (K = 5):**

For each core variant:
1. Pick core dimensions that hit the target core area:
   - Core uses 50-100% of target area (100% if max wings is 0, otherwise scaled down
     by wing count: 1 wing → 50-65%, 2 wings → 40-55%, 3 wings → 35-50%).
   - Choose a random aspect ratio between 1:1 and 2:1.
   - Compute width and depth from area and ratio.
   - Snap width and depth to odd numbers (preferred) when possible.
   - Clamp to the candidate area bounds and the size class min side.
2. Pick a random position within the candidate area (any valid x, z where the core fits).

**For each core variant, generate W wing configurations (W = 4):**

For each wing configuration, iteratively attach wings up to the size class max:

1. **Compute remaining area budget.** `remaining = target_area - current_total_area`.
   If remaining < min_side^2 (25) and minimum wing count is met, stop.

2. **Pick a wing:**
   a. Pick a random edge of the core. Edges track occupied spans to allow multiple
      wings per side with gaps between them.
   b. Size the wing:
      - Wing area: 30-70% of core area, but at least min_side^2 if budget allows.
      - Wing length along shared edge: random, capped at 80% of edge to keep a notch.
      - Wing depth perpendicular: computed from area / length.
      - Snap to odd numbers. Enforce min side of 5 blocks.
   c. Align the wing on the edge:
      - 70% chance: **corner-flush** — flush with one end of the core edge (random end).
      - 30% chance: **centered** — centered in the available gap.
   d. **Validate** — must stay within the candidate area. If invalid, skip.

3. Repeat for remaining wing slots.

This produces K * W total layout candidates (e.g. 5 cores * 4 wing configs = 20 layouts).

```
Core only (Cottage):      Core + 1 wing (House):      Core + 2 wings (Hall):
+-------+                 +-------+                    +-------+---+
|       |                 |       |                    |       |   |
|  core |                 |  core +---+                |  core |   |
|       |                 |       |   |                |       +---+
|       |                 |       |   |                |       |
+-------+                 +-------+---+                +--+----+
                                                          |    |
                                                          +----+
```

### Step 3: Score and select (`generate.rs`)

**Filter:** Layouts are rejected if:
- Total area is below `min_side^2`
- Bounding box aspect ratio exceeds 2.5:1

**Score each remaining layout (0.0 to 1.0):**

```rust
const WEIGHT_AREA_MATCH: f32 = 1.0;
const WEIGHT_PROPORTION: f32 = 0.6;
const WEIGHT_BALANCE: f32 = 0.2;
const WEIGHT_COMPLEXITY: f32 = 0.8;
```

- **Area match** — how close total area is to target. 1.0 = exact, drops off linearly.
- **Proportion** — wings should be noticeably smaller than the core. Penalize wings
  that approach core size.
- **Balance** — slight preference for placement near the center of the candidate area.
- **Complexity** — reward having wings (0 wings = 0.0, 1 = 0.6, 2 = 0.8, 3+ = 1.0).

**Select:** Scores are squared to bias toward better layouts, then one is chosen
via weighted random.

### Step 4: Merge into polygon (`merge.rs`)

Given the set of rectangles (core + wings), produce the clockwise polygon outline:

1. **Rasterize** all rectangles onto a local boolean grid.
2. **Walk the boundary** on the dual (corner) grid using the right-hand rule:
   - Start at the top-left corner of the top-left filled cell, facing right.
   - At each corner, check the cells to the right and left of the edge ahead:
     - Right cell empty → **convex corner**: record vertex, turn right.
     - Right filled, left empty → **straight edge**: step forward.
     - Both filled → **concave corner**: record vertex, turn left, step forward.
   - Stop when returning to the start position and direction.
3. Remove collinear vertices (points along straight runs).
4. Convert local corner coordinates back to world coordinates.

The right-cell and left-cell for each direction:

| Facing | Right cell (interior) | Left cell (exterior) |
|--------|----------------------|---------------------|
| Right  | (cx, cz)             | (cx, cz-1)          |
| Down   | (cx-1, cz)           | (cx, cz)            |
| Left   | (cx-1, cz-1)         | (cx-1, cz)          |
| Up     | (cx, cz-1)           | (cx-1, cz-1)        |

This always produces a valid axis-aligned polygon. The grid is small
(building-scale, ~30x30 at most) so performance is not a concern.

## Edge Cases

- **Candidate area too small**: if the largest usable rectangle is smaller than
  min_side x min_side, return `None`.
- **Irregular usable area**: the maximal rectangle algorithm handles L-shaped or
  fragmented usable regions naturally — it just finds the biggest rectangle that fits.
- **Plot is entirely usable**: common case. The candidate area is the entire plot.
- **Wing doesn't fit**: if a wing extends outside the candidate area, skip it.
  The core alone is always a valid building.
- **Target area exceeds candidate area**: scale down the target. The size class is
  a goal, not a guarantee.
- **Extreme aspect ratio**: layouts with bounding box ratio > 2.5:1 are filtered out.

## Interior Boundaries

After footprint generation, adjacent rects need interior walls between them. The
`RectBoundary` struct and `find_boundaries()` function handle this.

```rust
/// A boundary between two adjacent rects where an interior wall goes.
struct RectBoundary {
    rect_a: usize,
    rect_b: usize,
    /// Cell positions where wall blocks are placed.
    wall_cells: Vec<Point2D>,
}
```

`find_boundaries(rects)` scans all rect pairs for adjacency (shared edge with 1-block
gap). The wall is placed on the **inside edge of the core rect** (index 0) so wings keep
their full interior space. For wing-to-wing boundaries, the wall goes on the lower-indexed
rect's edge.

```
  Core rect (idx 0)        Wing rect (idx 1)
  ┌─────────────┬─────────────┐
  │             │W│           │
  │    core     │W│   wing    │
  │             │W│           │
  └─────────────┴─────────────┘
                 ↑
         wall_cells on core's
         east edge (max x of core)
```

This is consumed by:
- `walls::boundary_cell_set()` — prevents side doors from overlapping interior walls
- `rooms::build_rooms()` — places interior wall blocks + archway doors at boundaries

## File Layout

- `mod.rs` — `Plot`, `Footprint`, `SizeClass`, `RectBoundary`, `find_boundaries()`, `generate_footprint()` entry point
- `maximal_rect.rs` — histogram-based largest rectangle algorithm (Step 1)
- `generate.rs` — `Layout`, core/wing generation, scoring, selection (Steps 2-3)
- `merge.rs` — rasterize + boundary walk to produce polygon (Step 4)
- `test.rs` — integration tests, ASCII rendering, Minecraft visualization

## Design Decisions

- **Generate-and-score**: multiple complete layouts (core + wings) are generated and
  scored as whole units, then selected via weighted random. Avoids greedy decisions where
  a good core leaves no room for wings.
- **Emergent shapes**: wings are attached iteratively rather than picked from templates.
  This produces more varied, organic-looking buildings.
- **Wings attach to core only**: no wing-on-wing chaining. Keeps shapes manageable for
  roof and interior modules.
- **Core is always largest**: wings are 30-70% of core area (floored at min_side^2).
- **Edge alignment bias**: 70% corner-flush, 30% centered. Corner-flush gives
  classic L/T shapes, centered gives occasional variety.
- **Odd number bias**: width and depth preferentially snap to odd numbers for centered
  doors, windows, and ridgelines.
- **Axis-aligned only**: no rotation. Minecraft's block grid is axis-aligned.
- **Min side 5 blocks**: smallest viable building. Enough for a door, window, and
  usable interior.
- **Max aspect ratio 2.5:1**: prevents awkwardly long buildings from wing placement.
- **Validation via debug_assert**: all filled points are checked against the usable
  mask in debug builds. The candidate area constraint makes violations impossible
  in practice, so this is a safety net rather than a runtime check.
