# Walls Module

Generates wall segments with openings (doors, windows) and places wall blocks along
the footprint edges of a building.

## Input
- `Frame` (footprint polygon + floor count + wall heights + base Y)
- `Plot` (needed to score door placement — segments facing the nearest plot edge / road get priority)
- `Palette` (material role assignments for this building)
- `RNG`

## Output
- `WallSegments` -- wall definitions with openings, consumed by interior module

## Data Structures

### WallSegment

A single straight run of wall between two footprint vertices, at one floor level.

```rust
struct WallSegment {
    /// Start vertex (world coords, inclusive)
    start: IVec2,
    /// End vertex (world coords, exclusive -- next segment starts here)
    end: IVec2,
    /// Which direction the wall faces outward (Cardinal)
    facing: Cardinal,
    /// Floor index (0 = ground)
    floor: u32,
    /// Y position of the floor base in world coords
    base_y: i32,
    /// Wall height in blocks for this floor
    height: u32,
    /// Openings cut into this segment
    openings: Vec<Opening>,
}
```

Segments are always axis-aligned (the footprint is a rectilinear polygon).
The `facing` direction points outward from the building interior -- this determines
which side gets the exterior texture and which side faces inward.

### WallSegments

Collection returned from the walls module. The interior module uses this to know
where doors connect rooms and where windows exist.

```rust
struct WallSegments {
    segments: Vec<WallSegment>,
}
```

Provides helpers:
- `doors()` -- iterator over all door openings with their world positions
- `windows()` -- iterator over all window openings
- `segments_on_floor(floor: u32)` -- filter by floor
- `segment_at(pos: IVec2, floor: u32)` -- find the segment containing a world position

### Opening

```rust
struct Opening {
    /// Type of opening
    kind: OpeningKind,
    /// Offset along the segment (in blocks from segment start)
    offset: u32,
    /// Width in blocks
    width: u32,
    /// Height in blocks
    height: u32,
    /// Vertical offset from floor base (usually 0 for doors, 1 for windows)
    y_offset: u32,
}
```

### OpeningKind

```rust
enum OpeningKind {
    Door(DoorStyle),
    Window(WindowStyle),
}

enum DoorStyle {
    Single,   // 1 wide, 2 tall -- standard wooden door
    Double,   // 2 wide, 2 tall -- double doors for larger buildings
    Archway,  // 2-3 wide, 3 tall -- open arch, no door block, uses stairs for the arch top
}

enum WindowStyle {
    Small,  // 1 wide, 1 tall -- single pane, good for tight walls
    Tall,   // 1 wide, 2 tall -- common residential window
    Wide,   // 2 wide, 2 tall -- larger buildings, upper floors
}
```

## Algorithm

### Step 1: Build Segments per Floor

Because wings can be shorter than the core, the building outline can change per
floor. For each floor:

1. Get the active rects via `frame.active_rects(floor)`.
2. Compute the outline polygon of the union of those rects (same merge algorithm
   used by the footprint module — rasterize active rects onto a grid and walk
   the boundary).
3. Walk the outline vertices in clockwise order. Each consecutive pair defines a
   wall segment. The outward-facing direction is to the right of the walk direction.

Each segment inherits:
- `start` / `end` from the outline edge
- `floor` from iteration
- `base_y` from `frame.floor_y(floor)`
- `height` from `frame.wall_height()`
- `facing` computed from edge direction (walk direction rotated 90 degrees clockwise)

On the ground floor the outline matches the full footprint polygon. On upper floors
where a wing drops out, the outline shrinks and the edges where the wing met the
core become new exterior wall segments automatically.

The segment length in blocks is `|end - start|`. Very short segments (< 3 blocks) get
no openings.

### Step 2: Place Doors

**Goal:** At least one door on the ground floor. Prefer edges facing the road or
nearest plot boundary.

Algorithm:
1. Collect all ground-floor segments (floor == 0).
2. Score each segment by two factors:
   a. **Plot edge proximity:** how close its outward face is to a plot edge. The edge of
      the plot closest to the segment midpoint gets a high score. If road direction info
      is available from the Plot, segments facing the road get a bonus.
   b. **Terrain height match:** sample 3-5 points one block outside the segment (at
      evenly spaced positions along its length). Compute the average height difference
      from `base_y`. Lower difference = higher score. This penalizes doors that would
      face a cliff or drop-off created by the foundation.
   Combine scores: `score = edge_proximity_score - terrain_diff_penalty`.
3. Pick the highest-scoring segment. Place one door near the center of that segment.
4. For larger buildings (footprint area > 100 blocks), optionally place a second door
   on a different segment with the next highest score, so the building has a back door.
5. Door style selection:
   - `Single` by default.
   - `Double` if the building footprint area exceeds 150 blocks or style tags include
     "grand" / "civic".
   - `Archway` for buildings tagged "market", "temple", or if the wall height >= 5.

Placement constraints:
- Doors must be at least 2 blocks from a segment endpoint (leave room for corner posts).
- Doors always sit at y_offset = 0 (flush with the floor).
- If a segment is too short for a door (< door width + 4), skip it and try the next
  scored segment.

### Step 3: Place Windows

**Goal:** Distribute windows across all floors with even spacing. Upper floors get more
windows than the ground floor.

Algorithm per segment:
1. Compute available length = segment length - (2 * corner_margin), where
   corner_margin = 1 block reserved at each end for corner posts.
2. Skip if available length < 3 (no room for any window).
3. Determine window density for this floor:
   - Ground floor: 1 window per 5-7 blocks of available length.
   - Upper floors: 1 window per 4-5 blocks of available length.
4. Compute window count = clamp(available_length / spacing, 0, max_windows).
5. Distribute windows evenly across the available length. Compute a stride =
   available_length / (count + 1), then place each window at offset
   corner_margin + stride * i for i in 1..=count.
6. Window style selection:
   - `Small` if wall height <= 3 or segment is very short (< 5 blocks).
   - `Tall` as the default for residential buildings.
   - `Wide` on upper floors of larger buildings or if segment length > 10.
7. Nudge windows to avoid overlapping with any door already placed on this segment.
   If a window position is within (door.offset - 1)..(door.offset + door.width + 1),
   skip that window.

**Symmetry:** For segments on opposite sides of the building (parallel edges of
the same length), mirror the window offsets so the building looks balanced from
any angle.

### Step 4: Place Corner Posts

Corner posts go at every outline vertex, extruded vertically for the floors where
they appear. A vertex that only exists on the ground floor outline (because it
belongs to a shorter wing) stops at that wing's roof height. A vertex that appears
on all floors extends to the core's roof height.

For each floor, the outline from Step 1 gives the vertex set. For each vertex,
track the highest floor it appears on and extrude from `frame.base_y()` to
`frame.floor_y(max_floor) + frame.wall_height()`.

1. Block type: `WoodPillar` role from the palette, using the `Pillar` / `Log` block form.
   Falls back to `PrimaryWood` if `WoodPillar` is not defined.
2. For stone buildings (palette has no wood pillar, or style tags include "stone"),
   use `StonePillar` role instead.
3. Corner posts are 1x1 in footprint. They occupy the exact vertex position.

At concave corners (interior angles > 180 degrees, which happen in L-shapes and
T-shapes), the corner post still fills the vertex column but no wall face is generated
into the building interior at that point.

### Step 5: Fill Wall Blocks

Iterate over every wall segment. For each block position in the segment that is not
inside an opening:

1. Determine the material role:
   - Default: `PrimaryWall`.
   - Bottom row (y == base_y): optionally `SecondaryWall` or `PrimaryStone` for a
     stone foundation strip (if style calls for it).
   - Top row (y == base_y + height - 1): optionally `Accent` for a trim band.
   - Blocks adjacent to openings (the frame around doors/windows): `SecondaryWall`
     or `WoodPillar` for a visible frame.
2. Look up the block from the palette:
   `palette.get_block(role, &BlockForm::Block, materials, rng)`.
3. Place the block via the editor.

For openings themselves:
- **Windows:** Place glass panes (hardcoded `minecraft:glass_pane` or from palette if
  a glass role is added later). Optionally place trapdoors as shutters on the exterior
  using `PrimaryWood` role with `Trapdoor` form.
- **Doors:** Place door blocks using `PrimaryWood` role with `Door` form. Double doors
  place two door blocks side by side. Archways place stairs blocks (`PrimaryStone`,
  `Stairs` form) forming the arch top, with air below.

## Interaction with Existing System

The current v1 system uses `WallComponent` structs loaded from NBT structure files,
selected per grid cell face. The v2 approach described here works at a finer
granularity -- individual blocks along a polygon edge rather than a grid cell face.

Key differences:
- v1: One structure template stamped per cell face. Opening type baked into the template.
- v2: Wall segments span arbitrary lengths. Openings are positioned algorithmically,
  then blocks placed individually with material palette lookups.

The `Palette` and `MaterialRole` systems are shared. The same palette that drives v1
walls drives v2 walls -- only the placement logic changes.

## Resolved Questions

- **Door positions:** Scored by proximity to plot edge / road. See Step 2.
- **Window distribution:** Even spacing per segment per floor, density varies by floor.
  See Step 3.
- **Corner posts:** Separate from wall fill, placed at footprint vertices. See Step 4.
- **Wall thickness:** Always 1 block. Interior module can add interior wall surfaces
  if needed for room partitions.
