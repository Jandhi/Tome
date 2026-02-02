# Composite Roof Implementation Plan

## Overview

Support hip roofs for composite (non-rectangular) footprints created by combining two or more rectangles. The current implementation has issues with L-shaped and other concave polygons.

---

## Current Problems

### 1. Inner Corner Handling
At the concave (inner) corner of an L-shape, the distance-to-edge algorithm breaks down:
- A point near the inner corner is equidistant to two perpendicular edges
- The algorithm arbitrarily picks one, causing inconsistent stair facing
- No roof blocks fill the inner corner's overhang area

```
Problem area marked with ?:
    +------+
    |  B   |
+---+??+   |   <- Inner corner has no overhang
| A    |   |      and confused stair directions
+------+---+
```

### 2. Stair Facing Inconsistency
Near corners and edges, `find_closest_edge_facing()` picks whichever edge happens to be marginally closer, leading to:
- Stairs facing different directions in adjacent blocks
- Especially problematic at the L-junction where two roof sections meet

### 3. Separate Peaks
The two sections of the L create independent peaks rather than forming a unified roof:
- The taller section rises to its own peak
- The horizontal section rises to a separate peak
- No valley or ridge connects them

### 4. Missing Overhang at Inner Corner
The `is_within_distance()` check doesn't extend overhang into the concave corner because:
- The point is outside the polygon
- It's not within overhang distance of any single edge (it's in the corner)

---

## Proposed Solutions

### Approach A: Two Overlapping Rectangular Roofs (Recommended)

Instead of treating the L as a single polygon, place two separate rectangular hip roofs that overlap:

```rust
pub async fn place_composite_hip_roof(
    roof: &Roof,
    rect_a: &Footprint,  // Original rectangle A
    rect_b: &Footprint,  // Original rectangle B
    editor: &Editor,
    // ...
) {
    // Place roof for rect_a
    place_hip_roof(roof, rect_a, editor, ...).await;

    // Place roof for rect_b (will overwrite overlapping area)
    place_hip_roof(roof, rect_b, editor, ...).await;

    // Optionally: create valley where they meet
    place_valley(rect_a, rect_b, editor, ...).await;
}
```

**Pros:**
- Simple to implement
- Each rectangular section gets a proper hip roof
- Natural-looking intersection

**Cons:**
- Need to track original rectangles, not just merged footprint
- Overlapping area gets built twice
- Valley creation adds complexity

### Approach B: Distance Field with Corner Handling

Improve the distance calculation to handle concave corners:

1. **Detect concave vertices** - vertices where the interior angle > 180 degrees
2. **For points near concave vertices** - use distance to the vertex itself, not edges
3. **Create diagonal "virtual edges"** at concave corners for facing calculation

```rust
fn distance_to_polygon(footprint: &Footprint, point: Point2D) -> (i32, Cardinal) {
    let edge_dist = footprint.distance_to_edge(point);

    // Check distance to concave vertices
    for vertex in footprint.concave_vertices() {
        let vertex_dist = chebyshev_distance(point, vertex);
        if vertex_dist < edge_dist {
            // Near a concave corner - face diagonally toward it
            return (vertex_dist, diagonal_facing(point, vertex));
        }
    }

    (edge_dist, find_closest_edge_facing(footprint, point))
}
```

**Pros:**
- Works with the merged polygon directly
- Handles arbitrary concave shapes

**Cons:**
- More complex distance calculations
- Diagonal stairs may look odd
- Still doesn't solve the "two peaks" problem

### Approach C: Heightmap-Based Roof

Pre-compute a heightmap for the entire roof area:

1. Initialize heightmap with 0 for all positions
2. For each rectangular section, compute its individual hip roof heights
3. Take the maximum height at each position
4. Place blocks based on final heightmap

```rust
struct RoofHeightmap {
    heights: HashMap<Point2D, i32>,
    facings: HashMap<Point2D, Cardinal>,
}

fn compute_composite_heightmap(
    rectangles: &[Footprint],
    overhang: i32,
    pitch: RoofPitch,
) -> RoofHeightmap {
    let mut map = RoofHeightmap::new();

    for rect in rectangles {
        let rect_heights = compute_hip_heights(rect, overhang, pitch);
        map.merge_max(rect_heights);  // Take max height at each point
    }

    map
}
```

**Pros:**
- Clean separation of height calculation and block placement
- Easy to debug (can visualize heightmap)
- Naturally handles overlapping roofs

**Cons:**
- Memory overhead for heightmap
- Need to determine facings after merging heights

---

## Implementation Plan

### Phase 1: Refactor to Track Source Rectangles

Modify the L-shaped building test to keep references to the original rectangles:

```rust
struct CompositeFootprint {
    merged: Footprint,           // The L-shaped outline
    source_rects: Vec<Footprint>, // Original rectangles
}
```

### Phase 2: Implement Overlapping Roof Placement

Create `place_composite_hip_roof()` that:
1. Places a hip roof for each source rectangle
2. Uses max-height logic where roofs overlap
3. Determines stair facing based on the rectangle that "owns" that position

### Phase 3: Valley Creation (Optional)

Where two roof sections meet at different heights, create a valley:
1. Detect the intersection line between rectangles
2. Place valley blocks (inverted stairs or slabs) along this line
3. Ensure water drainage path

### Phase 4: Generalize to N Rectangles

Support buildings made of 3+ intersecting rectangles:
- T-shapes (3 rectangles)
- + shapes (2 rectangles, already works)
- More complex compositions

---

## Data Structure Changes

### Option 1: Extend Footprint

```rust
pub struct Footprint {
    pub vertices: Vec<Point2D>,
    pub source_rects: Option<Vec<Footprint>>,  // If composite
}

impl Footprint {
    pub fn from_union(a: Footprint, b: Footprint) -> Self {
        let outer = a.outer_edges_with(&b);
        let vertices = outer.iter().map(|(s, _)| *s).collect();
        Self {
            vertices,
            source_rects: Some(vec![a, b]),
        }
    }
}
```

### Option 2: Separate CompositeFootprint Type

```rust
pub struct CompositeFootprint {
    pub outline: Footprint,
    pub components: Vec<Footprint>,
}

impl CompositeFootprint {
    pub fn from_rectangles(rects: Vec<Footprint>) -> Self {
        // Merge all rectangles into outline
        // Keep components for roof calculation
    }
}
```

---

## Test Cases

1. **L-shape** - Two rectangles forming an L
2. **T-shape** - Three rectangles forming a T
3. **+ shape** - Two rectangles forming a cross
4. **Stacked** - Two rectangles where one is fully inside the other (should just use outer)
5. **Adjacent** - Two rectangles sharing an edge but not overlapping

---

## References

- Current implementation: `src/generator/buildings_v2/roof/hip.rs`
- Footprint union: `src/generator/buildings_v2/footprint.rs::outer_edges_with()`
- Test: `src/generator/buildings_v2/test.rs::place_l_shaped_building()`
