# Walls Module

Generates wall segments along the building outline, places doors and windows,
then fills wall blocks and timber framing. Operates in distinct phases that
run at different points in the pipeline.

## Input
- `Frame` (footprint polygon + floor count + wall heights + base Y)
- `Plot` (bounds used to score door placement — segments near the plot edge get priority)
- `Palette` (PrimaryWall, PrimaryWood, WoodPillar material roles)
- `RNG`

## Output
- `WallSegments` — segment definitions with openings, consumed by floors, rooms, and furnish

## Pipeline Order

The walls module is split into phases that interleave with other modules:

```
build_segments()           ← creates WallSegment structs (no block placement)
place_doors()              ← picks door positions on ground floor
place_windows()            ← distributes windows across all segments
  ↓
[floors::place_floors]     ← needs wall_segs for stair wall avoidance
  ↓
place_wall_infill()        ← fills wall bodies with PrimaryWall blocks
place_frame()              ← timber frame: crossbeams + corner posts (overwrites infill)
place_openings()           ← doors (Door form) and windows (Fence form)
  ↓
[roof::place_roof]
[rooms::build_rooms]       ← uses wall_segs for entry detection
[furnish::furnish_rooms]
```

## Data Structures

### WallSegment

A single straight run of wall between two outline vertices, at one floor level.

```rust
struct WallSegment {
    /// Start vertex (dual-grid coords)
    start: Point2D,
    /// End vertex (dual-grid coords)
    end: Point2D,
    /// Extra cell prepended at a concave start corner
    extra_start: Option<Point2D>,
    /// Extra cell appended at a concave end corner
    extra_end: Option<Point2D>,
    /// Which direction the wall faces outward (Cardinal)
    facing: Cardinal,
    /// Floor index (0 = ground)
    floor: u32,
    /// Y position of the floor surface in world coords
    base_y: i32,
    /// Wall height in blocks of air for this floor
    height: u32,
    /// Length of this segment in blocks (includes extra cells)
    length: i32,
    /// Openings cut into this segment
    openings: Vec<Opening>,
}
```

### WallSegments

```rust
struct WallSegments {
    segments: Vec<WallSegment>,
}
```

Provides:
- `segments_on_floor(floor)` — filter by floor
- `doors()` — iterator over `(&WallSegment, &Opening)` for all doors
- `windows()` — same for windows

### Opening / OpeningKind

```rust
struct Opening {
    kind: OpeningKind,
    offset: u32,      // blocks from segment start
    width: u32,
    height: u32,
    y_offset: u32,    // 0 for doors, 1 for windows
}

enum OpeningKind { Door(DoorStyle), Window(WindowStyle) }
enum DoorStyle { Single, Double, Archway }
enum WindowStyle { Small, Tall, Wide }
```

## Dual-Grid Coordinate System

Wall segments use the **dual grid** — vertices sit at cell corners, not cell centers.
The outline from `frame.outline_at_floor()` returns these corner vertices in clockwise order.

```
  Dual-grid vertices (corners):     Actual cells (offsets depend on facing):

  v0 ─────── v1                     c0  c1  c2
  │          │                      ┌───┬───┬───┐
  │  cell 0  │  cell 1              │   │   │   │  ← wall cells
  │          │          ...         └───┴───┴───┘
  v3 ─────── v2
```

The `walk_edge_cells()` function converts a dual-grid edge (start→end) into actual
cell positions using direction-based offsets:

| Walk direction | Cell offset from vertex |
|---------------|------------------------|
| +x (East)     | (0, 0)                |
| +z (South)    | (-1, 0)               |
| -x (West)     | (-1, -1)              |
| -z (North)    | (0, -1)               |

The outward-facing direction is 90° clockwise from the walk direction:

| Walking | Facing outward |
|---------|---------------|
| +x      | South (+z)    |
| +z      | West (-x)     |
| -x      | North (-z)    |
| -z      | East (+x)     |

## Concave Corner Handling

At L/T-shaped buildings, inner (concave) corners need special treatment.
Without it, a gap forms at the corner cell that no segment claims.

```
  Convex corner (outside angle):    Concave corner (inside angle):

       ┌────                            ────┐
       │                                    │
       │                             ───────┘
  segments share                     both segments extend
  the corner cell                    by 1 extra cell into
  naturally                          the corner gap
```

At a concave corner:
- The **ending** segment gets `extra_end`: one cell using the current edge's offset
  applied at the end vertex
- The **starting** segment gets `extra_start`: one cell using the previous edge's
  offset applied at the start vertex

This creates an intentional **1-cell overlap** at the corner — both segments claim it.
This is correct: it ensures no gap in the wall.

Detection uses the cross product of consecutive edge directions:
```rust
fn is_concave_corner(prev, curr, next) -> bool {
    let cross = dx1 * dz2 - dz1 * dx2;
    cross < 0  // negative cross = right turn = concave in CW polygon
}
```

## Algorithm

### Phase 1: build_segments()

Creates `WallSegments` from the frame. No block placement.

For each floor:
1. Get outline via `frame.outline_at_floor(floor)` (dual-grid vertices, CW order)
2. For each consecutive vertex pair (edge), create a `WallSegment`:
   - Compute `facing` from edge direction
   - Detect concave corners at start/end, set `extra_start`/`extra_end`
   - `length` = edge walk length + extra cells
   - `openings` starts empty

### Phase 2: place_doors()

Places doors on ground-floor segments. Scores by proximity to plot edge.

1. Determine door style: `Double` if footprint area > 150, else `Single`
2. Score all ground-floor segments by `distance_to_plot_edge(midpoint, plot_bounds)`
3. Sort ascending (closest to edge = best)
4. Place primary door centered on the best segment
5. For buildings with area > 100: place a secondary `Single` door on the opposite-
   facing segment (or any non-adjacent facing). Skip segments whose cells overlap
   with interior boundary cells (from `boundary_cell_set()`)

```
  Plot boundary
  ┌─────────────────────┐
  │                     │
  │   ┌─────────┐       │
  │   │  bldg   D ← primary door (closest to east plot edge)
  │   │         │       │
  │   D         │       │  ← secondary door (opposite side)
  │   └─────────┘       │
  │                     │
  └─────────────────────┘
```

### Phase 3: place_windows()

Distributes windows evenly across all segments. Upper floors get denser windows.

Per segment:
1. `available = length - 2 * corner_margin` (corner_margin = 1)
2. Style: `Small` (1×1) if available < 5, else `Tall` (1×2)
3. Spacing: ground floor = 5 blocks, upper floors = 4 blocks
4. Count = available / spacing, distributed at even stride
5. Skip positions that overlap with existing doors (±1 block margin)
6. All windows have `y_offset = 1` (raised 1 block above floor)

### Phase 4: place_wall_infill() (async)

Fills wall bodies with `PrimaryWall` blocks. Runs BEFORE frame and openings
so they can overwrite.

For each segment, for each cell (via `segment_cells()`), for each Y from
`base_y` to `base_y + height - 1`: place a block unless the position falls
inside an opening.

### Phase 5: place_frame() (async)

Places timber frame: horizontal crossbeams and vertical corner posts.
Uses `WoodPillar` role (falls back to `PrimaryWood`).

**Crossbeams**: For every wall cell on every segment:
- Place log at `floor_y - 1` (floor level, beam axis along wall)
- Place log at `ceiling_y` (ceiling level, beam axis along wall)
- Beam axis is perpendicular to facing: E/W-facing → Z axis, N/S-facing → X axis

**Corner posts**: For every unique vertex (first cell of each segment):
- Track the highest floor each vertex appears on
- Extrude a Y-axis log column from `base_y` to the top ceiling
- Posts use `place_block_forced` to override crossbeams at intersections

```
  Cross-section through a wall at a corner:

  ║ P ║                P = corner post (Y-axis log, full height)
  ║ P ║ B  B  B  B     B = crossbeam (horizontal log at floor/ceiling)
  ║ P ║ W  W  W  W     W = wall infill (PrimaryWall blocks)
  ║ P ║ W  W  W  W
  ║ P ║ W  W  W  W
  ║ P ║ B  B  B  B
  ║ P ║ W  W  [D] W    D = door opening (air, then Door block)
  ║ P ║ W  W  [D] W
  ║ P ║ B  B  B  B     ← ground floor crossbeam
  ═════════════════     ← foundation
```

### Phase 6: place_openings() (async)

Places actual door and window blocks into the openings.

**Doors**: `PrimaryWood` with `Door` form.
- Lower half: `facing=<outward>, half=lower, hinge=right|left`
- Upper half: `facing=<outward>, half=upper, hinge=right|left`
- First block gets `hinge=right`, second gets `hinge=left` (for double doors)

**Windows**: `PrimaryWood` with `Fence` form (fence posts connecting to
adjacent blocks create a window-pane look).

## Helper Functions

- `segment_cells(seg)` — returns all cell positions for a segment including
  extra_start/extra_end. Used by floors, rooms, and furnish modules.
- `boundary_cell_set(rects)` — collects all interior boundary cells (from
  `footprint::find_boundaries`). Used to prevent secondary doors from
  overlapping interior archway positions.
- `facing_from_edge(start, end)` — outward normal for a CW polygon edge.
- `edge_offset(dx, dz)` — dual-grid vertex to cell offset for a walk direction.

## Interaction with Other Modules

- **floors**: receives `&WallSegments` to avoid placing stairs against door walls
- **rooms**: uses `wall_segs.doors()` to find entry rect, uses `segment_cells()`
  to locate door positions for floor map entrances
- **furnish**: indirectly via rooms' floor maps (doors → ReachableOpen cells)
