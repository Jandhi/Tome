# Frame Module

Defines the 3D skeleton of the building -- floors, wall heights, overall vertical extent.
Takes the 2D footprint and turns it into a 3D volume that every downstream module
(walls, floors, roof, interior) reads from.

## Input
- `Footprint` -- polygon outline (vertices in clockwise order)
- `base_y: i32` -- the Y level the building sits on (from foundation)
- `SizeClass` -- Cottage, House, Hall, Manor (shared with footprint module)
- `RNG` -- for randomized decisions

## Output
- `Frame` -- 3D volume (footprint + per-rect floor counts + wall height + base Y)

## Frame Struct

```rust
struct Frame {
    footprint: Footprint,
    base_y: i32,
    floor_counts: Vec<u32>, // per rect, parallel to footprint.rects()
    wall_height: u32,       // interior wall height per floor (blocks of air), uniform
}
```

`floor_counts[i]` is the number of floors for `footprint.rects()[i]`. The core
(`rects[0]`) gets the full floor count from the size class. Each wing gets the
same count or one fewer, chosen randomly.

`wall_height` is the number of air blocks between a floor surface and the ceiling
surface above it. The actual block distance from one floor surface to the next is
`wall_height + 1` (the ceiling block is shared with the floor block of the story
above). All floors across all rects use the same `wall_height`.

## Floor Count Decision

The core floor count is driven by `SizeClass` and randomness. Wing floor counts
are derived from the core.

### Core floor count

| SizeClass | Floors | Notes                              |
|-----------|--------|------------------------------------|
| Cottage   | 1      | Always single-story                |
| House     | 1–2    | Standard town buildings            |
| Hall      | 2–3    | Craftsmen, taverns, shops          |
| Manor     | 2–3    | Elite, landmarks                   |

### Wing floor counts

Each wing gets the core's floor count or one fewer (randomly, ~50/50), with a
minimum of 1. This means a 2-floor core can have 1- or 2-floor wings, and a
3-floor core can have 2- or 3-floor wings. Single-floor cores always have
single-floor wings.

```
let core_floors = rng.range(size_class.floor_range());
let wing_floors = |rng: &mut RNG| -> u32 {
    if core_floors > 1 && rng.chance(0.5) {
        core_floors - 1
    } else {
        core_floors
    }
};
```

When a wing is shorter than the core, the upper floors of the core don't extend
into that wing's area. This creates a step-down in the building volume:

```
Top view, floor 1 only:        Top view, floor 0:
+-------+                      +-------+---+
|       |                      |       |   |
|  core |                      |  core |   | <- wing (1 floor)
|       |                      |       |   |
+-------+                      +-------+---+
```

## Wall Heights

| Value                | Blocks | Explanation                                    |
|----------------------|--------|------------------------------------------------|
| `wall_height`        | 3      | Standard interior clearance (3 air blocks)     |
| Floor slab           | 1      | The floor/ceiling block itself                 |

A standard single-story building with `wall_height = 3`:
```
Y+4  ---- roof starts here (top of walls)
Y+3  |  | air
Y+2  |  | air
Y+1  |  | air   <- 3 blocks of air (wall_height)
Y+0  ==== floor surface (base_y)
```

A two-story building:
```
Y+8  ---- roof starts here
Y+7  |  | air
Y+6  |  | air
Y+5  |  | air   <- floor 1 wall_height (3)
Y+4  ==== floor 1 surface (also ceiling of floor 0)
Y+3  |  | air
Y+2  |  | air
Y+1  |  | air   <- floor 0 wall_height (3)
Y+0  ==== floor 0 surface (base_y)
```

### Height calculations

```rust
impl Frame {
    /// Height in blocks for a given rect.
    fn rect_height(&self, rect_index: usize) -> u32 {
        self.floor_counts[rect_index] * (self.wall_height + 1)
    }

    /// Max floor count across all rects (the core's count).
    fn max_floors(&self) -> u32 {
        self.floor_counts[0]
    }
}
```

## Helper Methods

```rust
impl Frame {
    /// Y level of the floor surface for a given story (0-indexed).
    fn floor_y(&self, floor: u32) -> i32 {
        self.base_y + floor as i32 * (self.wall_height as i32 + 1)
    }

    /// Y level of the ceiling for a given story.
    fn ceiling_y(&self, floor: u32) -> i32 {
        self.floor_y(floor) + self.wall_height as i32
    }

    /// Y level where the roof starts for a given rect.
    fn roof_y(&self, rect_index: usize) -> i32 {
        self.base_y + self.rect_height(rect_index) as i32
    }

    /// Which rects are active (have floors) at a given story.
    /// Returns indices into footprint.rects().
    fn active_rects(&self, floor: u32) -> Vec<usize> {
        self.floor_counts.iter().enumerate()
            .filter(|(_, &count)| floor < count)
            .map(|(i, _)| i)
            .collect()
    }

    /// The 2D points that have a floor at a given story.
    /// Union of all active rects' filled points.
    fn filled_points_at_floor(&self, floor: u32) -> Vec<Point2D> {
        let rects = self.footprint.rects();
        self.active_rects(floor).iter()
            .flat_map(|&i| rects[i].filled_points())
            .collect()
    }

    /// All floor indices (0 to max_floors).
    fn floors(&self) -> impl Iterator<Item = u32> {
        0..self.max_floors()
    }
}
```

## How Frame Feeds Into Downstream Modules

### walls

Walls iterate `frame.floors()`. For each floor, `frame.active_rects(floor)` tells
which rects exist at that height. Exterior walls are placed along the outer edges
of the active rects' union. **Transition walls** appear where a taller rect is
adjacent to a shorter one — the upper floors of the taller rect have an exposed
wall face where the shorter rect's roof begins. These transition walls are found
by checking which rects were active on the previous floor but aren't on this one.

### floors

For each floor, `frame.filled_points_at_floor(floor)` gives the 2D points to
place floor surface blocks at `frame.floor_y(floor)`. Upper floors may cover
less area than the ground floor when wings are shorter. Stairs between floors
should be placed within rects that are active on both floors.

### roof

Each rect has its own `frame.roof_y(rect_index)`. The roof module builds a
separate roof section per rect at its respective height. Where rects meet at
different heights, the shorter rect's roof abuts the taller rect's wall.

### interior

Interior uses `frame.filled_points_at_floor(floor)` and `frame.ceiling_y(floor)`
to know the bounds of each floor. Upper floors in a multi-rect building may be
smaller than the ground floor.

### foundation

Foundation runs before frame and only needs the footprint and `base_y`.
Frame does not feed into foundation, but both share `base_y`.

## Construction

```rust
fn generate_frame(
    footprint: Footprint,
    base_y: i32,
    size_class: SizeClass,
    rng: &mut RNG,
) -> Frame {
    let core_floors = rng.range(size_class.floor_range());
    let mut floor_counts = vec![core_floors];

    for _ in 1..footprint.rects().len() {
        let wing_floors = if core_floors > 1 && rng.chance(0.5) {
            core_floors - 1
        } else {
            core_floors
        };
        floor_counts.push(wing_floors);
    }

    Frame {
        footprint,
        base_y,
        floor_counts,
        wall_height: 3,
    }
}
```

## Migration from Current System

The current system uses a `Grid` + `BuildingShape` with cell-based coordinates
(`Point3D` where Y is the floor index). The new Frame replaces this with:
- `Footprint` polygon instead of `Grid` cells for the horizontal extent
- Explicit `floor_count` + `wall_height` instead of encoding floors as Y offsets
  in the cell list
- Direct world-coordinate helpers instead of grid-to-world conversion

The grid approach works well for NBT-based buildings with fixed cell sizes. The new
frame approach is for procedurally generated buildings where the footprint is a
free-form polygon and wall/window placement is computed per-block rather than
per-cell.
