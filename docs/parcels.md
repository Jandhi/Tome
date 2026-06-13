# Parcel System

The parcel system divides the build area into logical zones for urban planning. It uses Voronoi partitioning to create natural, organic region boundaries.

## Overview

Parcels are the fundamental organizational unit of settlement generation. Each parcel is classified by type and contains metadata about its terrain, biomes, and neighbors.

## Data Structures

### ParcelID

```rust
pub struct ParcelID(pub usize);
```

A unique identifier for each parcel.

### Parcel

```rust
pub struct Parcel {
    pub id: ParcelID,
    pub data: ParcelData<ParcelID>,
}
```

### ParcelData

```rust
pub struct ParcelData<ID> {
    pub points: HashSet<Point3D>,           // All blocks in parcel
    pub points_2d: HashSet<Point2D>,        // 2D projection
    pub edges: HashSet<Point3D>,            // Border blocks
    pub parcel_adjacency: HashMap<ID, usize>, // Neighbor counts
    pub is_border: bool,                    // Touches build area edge
    pub origin: Point3D,                    // Center point
    pub parcel_type: ParcelType,        // Classification
}
```

### ParcelType

```rust
pub enum ParcelType {
    Urban,      // Buildings and streets
    Rural,      // Farms, outskirts
    OffLimits,  // Water, cliffs, protected
}
```

## Generation Pipeline

The parcel generation process follows these steps:

### 1. Spawn Parcels

```rust
spawn_parcels(world, count) -> Vec<Parcel>
```

Creates random seed points within the build area. These become the initial parcel centers.

### 2. Bubble Out (Voronoi Fill)

```rust
bubble_out(parcels, world) -> Vec<Parcel>
```

Expands parcels from seed points using Voronoi partitioning. Each point in the build area is assigned to the nearest parcel center.

**Algorithm:**
- Uses BFS from each seed point
- Points are claimed by the first parcel to reach them
- Creates natural cell-like boundaries

### 3. Recenter Parcels

```rust
recenter_parcels(parcels) -> Vec<Parcel>
```

Improves parcel shapes using Lloyd relaxation:

1. Calculate centroid of all points in parcel
2. Move parcel origin to centroid
3. Re-run Voronoi partitioning
4. Repeat (typically 3 iterations)

This produces more regular, evenly-sized parcels.

### 4. Analyze Adjacency

```rust
analyze_adjacency(parcels) -> ()
```

Builds the adjacency graph by counting shared edge points between parcels.

**Output:**
- `parcel_adjacency: HashMap<ParcelID, usize>` - Maps neighbor ID to shared edge count
- Used for pathfinding and parcel merging

### 5. Classify Parcels

```rust
classify_parcels(parcels, world) -> ()
```

Assigns parcel types based on terrain analysis:

**Urban Classification:**
- Low water content
- Moderate height variation
- Not on build area border

**Rural Classification:**
- Borders urban parcels
- Suitable terrain for farming

**OffLimits Classification:**
- High water content (ocean, river)
- Extreme height variation (cliffs)
- On build area border

### 6. Analyze Parcel

```rust
analyze_parcel(parcel, world) -> ParcelAnalysis
```

Collects statistics for each parcel:
- Biome distribution
- Average and range of heights
- Water percentage
- Tree density

## Super-Parcels

Super-parcels group multiple parcels of the same type for large-scale operations.

```rust
pub struct DistrictID(pub usize);

pub struct District {
    pub id: DistrictID,
    pub parcels: Vec<ParcelID>,
    pub parcel_type: ParcelType,
}
```

**Uses:**
- Wall placement around urban super-parcels
- Path routing between super-parcels
- Settlement-level statistics

## World Integration

Parcels are stored in the World as 2D maps:

```rust
// In World struct
pub parcel_map: Vec<Vec<Option<ParcelID>>>,
pub district_map: Vec<Vec<Option<DistrictID>>>,
```

**Access Pattern:**
```rust
let parcel_id = world.parcel_map[x as usize][z as usize];
```

## Merge Operations

Small or irregular parcels can be merged:

```rust
merge_down(parcels, min_size) -> Vec<Parcel>
```

Parcels below `min_size` are merged into their largest neighbor.

## Gates and Borders

Parcels on the build area edge are marked with `is_border = true`.

Gate generation uses border information:
- Gates placed on urban parcel edges that touch OffLimits
- Multiple gates per settlement for access

## Example Usage

```rust
// Generate parcels
let parcels = generate_parcels(world, &mut rng);

// Access parcel at coordinate
let parcel_id = world.get_parcel(x, z);
let parcel = &parcels[parcel_id.0];

// Check parcel type
match parcel.data.parcel_type {
    ParcelType::Urban => place_buildings(),
    ParcelType::Rural => place_farms(),
    ParcelType::OffLimits => skip(),
}
```

## Configuration

Parcel generation can be tuned via parameters:

- **Parcel Count** - More parcels = smaller regions
- **Recenter Iterations** - More iterations = more regular shapes
- **Urban Threshold** - Water/height limits for urban classification
- **Minimum Size** - Threshold for merge operations

## Visualization

Parcels can be visualized by:
1. Assigning colors to each parcel
2. Placing colored wool blocks at ground level
3. Edge blocks show boundaries

This is useful for debugging parcel generation.
