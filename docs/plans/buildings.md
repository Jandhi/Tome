# Building Generation

The building generation system accepts a plot of land and places a complete building on it.
It is composed of a series of modular stages, each with a clear interface so implementations
can be swapped independently.

## Pipeline

```
Plot + BuildingType + Palette + RNG
  → footprint::generate(Plot) → Footprint
  → foundation::place(Footprint) → base_y
  → frame::generate(Footprint, base_y, BuildingType) → Frame
  → walls::generate(Frame, Plot) → WallSegments
  → floors::place(Frame) → FloorPlan
  → roof::place(Frame) → ()
  → interior::generate(Frame, WallSegments, FloorPlan) → ()
  → exterior::generate(Plot, Footprint, WallSegments) → ()
```

## Plot

The input to the building generation system. A plot is a rectangular region with a mask
indicating which cells are usable for building.

```rust
struct Plot {
    bounds: Rect2D,
    usable: Vec<Vec<bool>>,  // 2D grid indexed [x][z] relative to bounds.min(), true = buildable
}
```

The rectangular bounds keep layout math simple and match Minecraft's block grid. The usable
mask allows upstream systems (parcels, terrain analysis) to mark out obstacles like water,
cliffs, trees, or neighboring buildings without needing to compute a clean polygon boundary.

```
  Plot (min to max)
  +-----------------+
  | . . . . . . . . |    . = usable
  | . . . . . . . . |    x = unusable (water, tree, etc.)
  | . . . . x x x . |
  | . . . . x x x . |    The footprint module fits a building
  | . . . . . . . . |    shape within the usable cells.
  | . . . . . . . . |
  | x x . . . . . . |
  | x x . . . . . . |
  +-----------------+

  After footprint generation:
  +-----------------+
  | . . . . . . . . |    # = building footprint
  | # # # # # . . . |
  | # # # # # x x . |
  | # # # # # x x . |
  | # # # # # . . . |
  | . . . . . . . . |
  | x x . . . . . . |
  | x x . . . . . . |
  +-----------------+
```

The footprint module is responsible for fitting a building shape within the usable area.

## Modules

### footprint
Determines the building's 2D shape and position within the plot. Outputs a polygon outline.
This is where shape decisions happen — rectangles, L-shapes, T-shapes, etc.

### foundation
Prepares the terrain under the footprint. Handles slopes, stilts, basements, or fill
depending on the terrain. Bridges the gap between natural terrain and a flat building base.

### frame
Defines the 3D skeleton: number of floors, wall height per floor, overall vertical extent.
Turns a 2D footprint into a 3D volume.

### walls
Generates wall segments with openings (doors, windows). Decides placement, spacing, and
types of openings. Fills in the wall blocks.

### floors
Places floor and ceiling surfaces for each story. Handles stairs between floors.

### roof
Generates the roof on top of the frame. Supports hip, gable, flat, and composite roofs
for non-rectangular shapes.

### interior
Room partitions, furniture, indoor lighting, fireplaces, etc.

### exterior
Gardens, fences, paths to the door, signs, outdoor lighting. Makes the plot feel complete.

## Size Classes

Building size is driven by a `SizeClass` that sets the target area, minimum side length,
and wing count range. The footprint module uses these to generate appropriately-sized shapes.

| Class   | Target area | Min side | Wings | Typical use                        |
|---------|-------------|----------|-------|------------------------------------|
| Cottage | 45–80       | 5        | 0-1   | Outskirts, rural, small buildings  |
| House   | 80–130      | 5        | 1-2   | Standard town buildings            |
| Hall    | 130–200     | 7        | 2-3   | Craftsmen, taverns, shops          |
| Manor   | 280–450     | 9        | 2-4   | Elite, landmarks                   |

## Shared Types

- **Plot** — `Rect2D` bounds + `Vec<Vec<bool>>` usable mask, input to the whole pipeline
- **Footprint** — 2D polygon (clockwise `Vec<Point2D>` vertices) + component `Vec<Rect2D>` rectangles
- **SizeClass** — target area range, min side, wing count range
- **Frame** — 3D volume combining a Footprint with vertical extent (floors, wall heights)
- **WallSegments** — Wall definitions with their openings (doors/windows), used by interior
  and exterior to understand door/window positions
- **FloorPlan** — Stairwell positions per floor, output of floors module, used by interior
  to avoid placing furniture/partitions in stairwells

## Notes

- Each module consumes outputs from earlier stages and shared context (materials, rng, editor)
- The existing materials/palette system handles block selection and feeds into most modules
- Coordinate types: `Point2D`/`Point3D` (integer), `Rect2D`/`Rect3D` (origin + size)
- RNG: `noise::RNG` — seed-based deterministic. Use `rng.derive()` for child RNGs
