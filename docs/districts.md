# District System

The district system divides the build area into logical zones for urban planning. It uses Voronoi partitioning to create natural, organic region boundaries.

## Overview

Districts are the fundamental organizational unit of settlement generation. Each district is classified by type and contains metadata about its terrain, biomes, and neighbors.

## Data Structures

### DistrictID

```rust
pub struct DistrictID(pub usize);
```

A unique identifier for each district.

### District

```rust
pub struct District {
    pub id: DistrictID,
    pub data: DistrictData<DistrictID>,
}
```

### DistrictData

```rust
pub struct DistrictData<ID> {
    pub points: HashSet<Point3D>,           // All blocks in district
    pub points_2d: HashSet<Point2D>,        // 2D projection
    pub edges: HashSet<Point3D>,            // Border blocks
    pub district_adjacency: HashMap<ID, usize>, // Neighbor counts
    pub is_border: bool,                    // Touches build area edge
    pub origin: Point3D,                    // Center point
    pub district_type: DistrictType,        // Classification
}
```

### DistrictType

```rust
pub enum DistrictType {
    Urban,      // Buildings and streets
    Rural,      // Farms, outskirts
    OffLimits,  // Water, cliffs, protected
}
```

## Generation Pipeline

The district generation process follows these steps:

### 1. Spawn Districts

```rust
spawn_districts(world, count) -> Vec<District>
```

Creates random seed points within the build area. These become the initial district centers.

### 2. Bubble Out (Voronoi Fill)

```rust
bubble_out(districts, world) -> Vec<District>
```

Expands districts from seed points using Voronoi partitioning. Each point in the build area is assigned to the nearest district center.

**Algorithm:**
- Uses BFS from each seed point
- Points are claimed by the first district to reach them
- Creates natural cell-like boundaries

### 3. Recenter Districts

```rust
recenter_districts(districts) -> Vec<District>
```

Improves district shapes using Lloyd relaxation:

1. Calculate centroid of all points in district
2. Move district origin to centroid
3. Re-run Voronoi partitioning
4. Repeat (typically 3 iterations)

This produces more regular, evenly-sized districts.

### 4. Analyze Adjacency

```rust
analyze_adjacency(districts) -> ()
```

Builds the adjacency graph by counting shared edge points between districts.

**Output:**
- `district_adjacency: HashMap<DistrictID, usize>` - Maps neighbor ID to shared edge count
- Used for pathfinding and district merging

### 5. Classify Districts

```rust
classify_districts(districts, world) -> ()
```

Assigns district types based on terrain analysis:

**Urban Classification:**
- Low water content
- Moderate height variation
- Not on build area border

**Rural Classification:**
- Borders urban districts
- Suitable terrain for farming

**OffLimits Classification:**
- High water content (ocean, river)
- Extreme height variation (cliffs)
- On build area border

### 6. Analyze District

```rust
analyze_district(district, world) -> DistrictAnalysis
```

Collects statistics for each district:
- Biome distribution
- Average and range of heights
- Water percentage
- Tree density

## Super-Districts

Super-districts group multiple districts of the same type for large-scale operations.

```rust
pub struct SuperDistrictID(pub usize);

pub struct SuperDistrict {
    pub id: SuperDistrictID,
    pub districts: Vec<DistrictID>,
    pub district_type: DistrictType,
}
```

**Uses:**
- Wall placement around urban super-districts
- Path routing between super-districts
- Settlement-level statistics

## World Integration

Districts are stored in the World as 2D maps:

```rust
// In World struct
pub district_map: Vec<Vec<Option<DistrictID>>>,
pub super_district_map: Vec<Vec<Option<SuperDistrictID>>>,
```

**Access Pattern:**
```rust
let district_id = world.district_map[x as usize][z as usize];
```

## Merge Operations

Small or irregular districts can be merged:

```rust
merge_down(districts, min_size) -> Vec<District>
```

Districts below `min_size` are merged into their largest neighbor.

## Gates and Borders

Districts on the build area edge are marked with `is_border = true`.

Gate generation uses border information:
- Gates placed on urban district edges that touch OffLimits
- Multiple gates per settlement for access

## Example Usage

```rust
// Generate districts
let districts = generate_districts(world, &mut rng);

// Access district at coordinate
let district_id = world.get_district(x, z);
let district = &districts[district_id.0];

// Check district type
match district.data.district_type {
    DistrictType::Urban => place_buildings(),
    DistrictType::Rural => place_farms(),
    DistrictType::OffLimits => skip(),
}
```

## Configuration

District generation can be tuned via parameters:

- **District Count** - More districts = smaller regions
- **Recenter Iterations** - More iterations = more regular shapes
- **Urban Threshold** - Water/height limits for urban classification
- **Minimum Size** - Threshold for merge operations

## Visualization

Districts can be visualized by:
1. Assigning colors to each district
2. Placing colored wool blocks at ground level
3. Edge blocks show boundaries

This is useful for debugging district generation.
