# Roof Module

Generates the roof on top of the frame. Works block-by-block using the footprint polygon
and frame height data, placing slabs, stairs, and full blocks to form the roof surface.

## Input
- `Frame` (footprint polygon + floor count + wall heights + base Y)

## Output
- `()` (modifies world directly via editor)

## Core Concept: Heightmap-Based Roof Generation

Every roof type is computed as a **heightmap** over the footprint's 2D bounding box. Each
(x, z) position gets a floating-point roof height. The heightmap drives block placement:
fractional heights map to slabs, stairs, or full blocks. This approach makes composite
roofs straightforward -- generate one heightmap per sub-roof and merge them with `max()`.

```
Heightmap for a gable roof (cross-section along X):

  height
    3 |       /\
    2 |      /  \
    1 |     /    \
    0 |____/      \____
      +-------------------> x
      overhang  footprint  overhang
```

Each cell in the heightmap stores: `f32` height above the top of the frame walls.

## Roof Types

### Gable Roof

Two sloping planes meeting at a central ridge that runs along the footprint's longer axis.

**Algorithm:**
1. Determine the ridge axis: the longer axis of the bounding rectangle.
2. The ridge runs centered along that axis at `ridge_y = (width / 2) * pitch`.
3. For each (x, z) in the heightmap:
   - `distance` = perpendicular distance from (x, z) to the nearest eave (the short-axis edges).
   - `height = distance * pitch`
   - Clamp to `ridge_y`.
4. The two gable ends (short-axis walls) are vertical triangles filled with wall material
   up to the ridge height.

```
  Top-down view (ridge runs along Z):
  +------------------+
  |  slope  | slope  |
  |   \     |     /  |
  |    \    |    /   |
  |     ridge line   |
  |    /    |    \   |
  |   /     |     \  |
  |  slope  | slope  |
  +------------------+

  Side view (gable end):
       /\
      /  \
     / wall\
    /material\
   /----------\
   |   wall   |
```

### Hip Roof

All four sides slope inward toward a ridge (or single peak for square footprints).

**Algorithm:**
1. For each (x, z) in the heightmap:
   - `distance` = minimum perpendicular distance from (x, z) to any footprint edge.
   - `height = distance * pitch`
2. This naturally creates 45-degree hip lines at corners (where two edges are equidistant).
3. For rectangular footprints, the ridge forms along the longer axis where two opposing
   edges produce equal distances.
4. For square footprints, all four hip lines meet at a single peak.

```
  Top-down view (hip lines shown as diagonals):
  +------------------+
  |\ slope / ridge \/ |
  | \     /        /  |
  |  \   /--------/   |
  |   \ /   ridge  \  |
  |   / \--------\  \ |
  |  /   \        \  \|
  | /slope \  slope \ |
  +------------------+
```

### Flat Roof

A simple horizontal surface one block above the top of the frame walls.

**Algorithm:**
1. For each (x, z) inside the footprint, set `height = 0` (or 1 block for parapet).
2. Optionally add a parapet: raise the heightmap along footprint edges by 1 block using
   wall material (fences, walls, or slabs).
3. Fill the interior with a flat surface (slabs or full blocks depending on style).

## Roof Pitch

Pitch controls how steeply the roof rises per horizontal block. Expressed as a ratio of
vertical rise per horizontal run.

| Pitch Name | Rise:Run | Blocks per horizontal | Typical use        |
|------------|----------|-----------------------|--------------------|
| Shallow    | 1:2      | slab every block      | Large buildings    |
| Medium     | 1:1      | stair every block     | Standard houses    |
| Steep      | 2:1      | full block + slab     | Towers, cottages   |

**Pitch-to-block mapping:**

The fractional height at each (x, z) determines which block to place:

- `frac == 0.0` -> full block (or nothing if height is 0)
- `frac == 0.5` -> top slab (half block)
- `frac > 0 && frac < 0.5` -> bottom slab at next Y level
- For stair blocks: when the slope direction is known, place a stair block oriented
  toward the downhill side.

In practice, at **medium pitch (1:1)**, each horizontal step inward raises the roof by
1 full block, which maps cleanly to stair blocks. At **shallow pitch (1:2)**, alternating
between slabs and stairs works well. At **steep pitch (2:1)**, stacking a full block plus
a slab per horizontal step is the approach.

**Stair orientation:** For each surface position, determine the slope direction (the
direction of steepest descent in the heightmap). Place the stair block facing downhill.
At hip lines where two slopes meet diagonally, the stair block faces the cardinal
direction closest to the true downhill direction (Minecraft stairs only face N/S/E/W).

## Composite Roofs

Non-rectangular footprints (L-shapes, T-shapes, U-shapes) are handled by decomposing
the footprint into overlapping rectangular sub-roofs, generating a heightmap for each,
and merging them.

**Decomposition algorithm:**
1. The footprint polygon is already composed of axis-aligned rectangular wings (from
   the footprint module).
2. Decompose into the minimal set of overlapping rectangles that cover the footprint.
   For an L-shape, this is two rectangles. For a T-shape, two rectangles. For a U-shape,
   three rectangles.
3. Each rectangle gets its own roof (gable or hip) with its own ridge axis along
   its longer dimension.

**Merging with max-heightmap:**
1. Initialize a heightmap over the full bounding box, all values set to `f32::NEG_INFINITY`.
2. For each sub-roof rectangle, compute its heightmap (gable or hip, with the chosen pitch).
3. For each (x, z), take `max(current, sub_roof_height)`.
4. The result: where two roofs overlap, the taller one wins. This naturally creates
   proper-looking intersections where one roof ridge meets another roof's slope.

```
  L-shaped building decomposed into two rectangles:

  +--------+
  |  Roof  |
  |   A    |
  |        +--------+
  |        | Overlap|
  +--------+  Roof  |
             |  B   |
             +------+

  In the overlap zone, max(A_height, B_height) produces a clean valley/ridge line
  where the two roofs meet. No special-case geometry needed.
```

**Why this works:** The max operation means the higher roof always "pokes through" the
lower one. At the intersection line where the heights are equal, a natural valley or
ridge forms. This matches how real roofs intersect.

## Overhang

The roof extends beyond the footprint edges to create eaves. Overhang is specified in
blocks (typically 1-2).

**Implementation:**
1. Expand the heightmap computation area by `overhang` blocks beyond the footprint
   bounding box on all sides.
2. The slope formulas already work outside the footprint -- they just continue the
   slope downward.
3. When placing blocks in the overhang zone, only place roof surface blocks (slabs,
   stairs), not the fill underneath. The overhang should be open air beneath.
4. At gable ends, the overhang can optionally be suppressed (no overhang on gable walls)
   or maintained (overhang wraps around all sides).

```
  Cross-section showing overhang:

         /==========\          = = roof surface blocks
        / ............\        . = air (open overhang)
       / ..############\       # = interior fill / attic
      /################  \
  ---|==================|---   frame wall tops
     |    wall          |
```

**Overhang support blocks:** For overhangs wider than 1 block, the outermost row may
need a support block underneath (a fence or wall block) to look structurally sound.
This is a style option, not structural necessity.

## Decorative Elements

### Gable Decorations

The triangular gable wall at each end of a gable roof can have decorative patterns:

- **X-decoration:** Fence or stick blocks placed in an X pattern across the gable
  triangle. Iterate over the triangle area, place a fence block where
  `|x_offset - z_offset| <= 1` or `|x_offset + z_offset - width| <= 1` (the two
  diagonals of the triangle).
- **Window:** A small 1x1 or 2x1 window centered in the gable triangle, placed when
  the triangle is tall enough (ridge height >= 3).
- **Trim:** Outline the gable triangle edge with a contrasting block (stairs oriented
  to follow the slope).

### Chimneys

A vertical column of blocks (brick, stone brick) that extends from the roof surface
upward.

**Placement rules:**
1. Pick a position on the roof, biased toward the ridge or an interior location (not
   at the edge or overhang).
2. The chimney base starts at the roof surface height at that (x, z).
3. Extend upward by 2-4 blocks above the ridge height so it visually pokes out.
4. Cap with a slab or stair block.
5. Optionally place a campfire or soul campfire inside the top for smoke particles.
6. The chimney column should punch through the roof surface -- when placing roof blocks,
   skip positions occupied by the chimney.

**Chimney sizing:** 1x1 for small houses, 2x1 or 2x2 for larger buildings.

### Ridge Decorations

Along the ridge line of gable or hip roofs:
- **Ridge caps:** A row of slabs or stairs along the ridge peak.
- **Finials:** At ridge endpoints, a decorative block (fence post, end rod, lightning rod).

## Block Selection

The roof surface uses directional blocks from the material palette:

- **Stairs** for the main slope surface (oriented downhill).
- **Slabs** for the ridge cap and half-step positions.
- **Full blocks** for steep pitches and interior fill beneath the roof surface.
- **Wall material** for gable triangles (matches the building's wall palette).
- **Accent material** for trim and decorations.

## Implementation Sketch

```rust
/// Height at every (x, z) position in the roof's bounding box.
struct RoofHeightmap {
    min: IVec2,          // world-space min corner
    width: usize,
    depth: usize,
    heights: Vec<f32>,   // row-major, width * depth
}

impl RoofHeightmap {
    fn get(&self, x: i32, z: i32) -> f32 { ... }
    fn set(&mut self, x: i32, z: i32, value: f32) { ... }

    /// Merge another heightmap using max(self, other) at each position.
    fn merge_max(&mut self, other: &RoofHeightmap) { ... }
}

enum RoofStyle { Gable, Hip, Flat }

struct RoofPlan {
    style: RoofStyle,
    pitch: f32,           // rise per horizontal block
    overhang: i32,        // blocks beyond footprint
    heightmap: RoofHeightmap,
}

/// Top-level entry point.
fn place_roof(frame: &Frame, editor: &Editor, palette: &Palette, rng: &mut RNG) {
    let rects = decompose_footprint(&frame.footprint);
    let mut combined = RoofHeightmap::new(/* bounding box of all rects + overhang */);

    for rect in &rects {
        let sub_roof = compute_heightmap(rect, style, pitch, overhang);
        combined.merge_max(&sub_roof);
    }

    place_blocks(&combined, frame, editor, palette);
    place_gable_walls(&combined, &rects, frame, editor, palette);
    place_decorations(&combined, frame, editor, palette, rng);
}
```

## Edge Cases

- **1-wide footprint wings:** When a rectangle is only 1 block wide, the roof is just
  a ridge with no slope. Treat as a flat-topped wall or a peaked 1-wide gable.
- **Very small buildings (3x3 or smaller):** The roof may only be 1-2 blocks tall. Skip
  decorations, use simple hip or flat roof.
- **Tall chimneys on low roofs:** Clamp chimney height to look proportional. Minimum 1
  block above ridge, maximum 4.
- **Overhang clipping:** If the overhang extends beyond the plot boundary, clip it to
  the plot edge.
