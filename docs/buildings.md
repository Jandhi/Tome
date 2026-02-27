# Building System

The building system generates and places individual structures within urban districts. It handles everything from footprint placement to interior construction.

## Overview

Buildings are procedurally generated within urban districts using a multi-stage pipeline that creates foundations, walls, floors, roofs, and interiors.

## Data Structures

### BuildingID

```rust
pub struct BuildingID(pub usize);
```

Unique identifier for each building.

### BuildingData

```rust
pub struct BuildingData {
    pub id: BuildingID,
    pub origin: Point3D,           // Bottom corner
    pub bounds: Rect3D,            // 3D bounding box
    pub super_district: SuperDistrictID,
    pub building_type: BuildingType,
    pub floors: Vec<FloorData>,
    pub entrance: Point3D,
}
```

### BuildingType

Buildings can be classified by their function:
- Residential
- Commercial
- Religious
- Civic
- Industrial

### FloorData

```rust
pub struct FloorData {
    pub level: i32,               // Floor number (0 = ground)
    pub height: i32,              // Ceiling height
    pub layout: FloorLayout,      // Room arrangement
}
```

## Generation Pipeline

### 1. City Block Creation

Urban districts are subdivided into city blocks using secondary Voronoi partitioning:

```rust
create_city_blocks(district) -> Vec<CityBlock>
```

City blocks group buildings that share common access and infrastructure.

### 2. Outer/Inner Point Separation

Each city block is analyzed to separate:
- **Outer Points** - Edge blocks facing streets
- **Inner Points** - Interior blocks for building placement

### 3. Grid Placement

Buildings are placed on a grid system within city blocks:

```rust
place_on_grid(city_block, building_templates) -> Vec<BuildingData>
```

**Grid Rules:**
- Minimum spacing between buildings
- Street frontage requirements
- Size constraints from templates

### 4. Foundation Construction

```rust
build_foundation(building, editor) -> ()
```

Prepares the ground for construction:
- Levels terrain to building origin height
- Places foundation blocks
- Creates basement if below ground level
- Handles sloped terrain with stepped foundations

### 5. Floor Construction

```rust
build_floor(building, floor_num, editor) -> ()
```

Creates each floor level:
- Places floor blocks (wood planks, stone, etc.)
- Defines room boundaries
- Reserves space for stairs
- Applies floor palette materials

### 6. Wall Construction

```rust
build_walls(building, editor) -> ()
```

Constructs exterior and interior walls:

**Exterior Walls:**
- Primary material from palette
- Window placement
- Door openings
- Decorative trim

**Interior Walls:**
- Room dividers
- Doorway connections
- Support columns

### 7. Roof Construction

```rust
build_roof(building, editor) -> ()
```

Caps the building with appropriate roof style:

**Roof Types:**
- Flat
- Gabled
- Hipped
- Mansard

Roof materials come from the palette's roof material.

### 8. Stair Construction

```rust
build_stairs(building, editor) -> ()
```

Adds vertical circulation between floors:

**Stair Types:**
- Straight run
- L-shaped
- U-shaped (switchback)
- Spiral (for towers)

**Algorithm:**
1. Find valid stair locations on each floor
2. Ensure vertical alignment between floors
3. Reserve landing space
4. Place stair blocks with correct orientation

## Build Claims

Buildings register their footprint in the world's build claim system:

```rust
pub enum BuildClaim {
    None,
    Building(BuildingID),
    Path,
    Wall,
}
```

**Purpose:**
- Prevents overlapping structures
- Guides path routing around buildings
- Identifies building boundaries

## Building Templates

Templates define building parameters:

```json
{
    "name": "small_house",
    "min_size": [5, 5],
    "max_size": [8, 8],
    "floors": [1, 2],
    "roof_type": "gabled",
    "required_features": ["door", "window"]
}
```

Templates are loaded from JSON via the `Loadable` trait.

## Building Sets

Related templates are grouped into building sets:

```rust
pub struct BuildingSet {
    pub id: String,
    pub templates: Vec<BuildingTemplate>,
    pub weights: Vec<f32>,  // Selection probability
}
```

Building sets allow biome or style-specific building selection.

## Material Assignment

Buildings receive materials from palettes:

```rust
pub enum MaterialRole {
    Primary,      // Main wall material
    Secondary,    // Accent walls
    Floor,        // Floor surfaces
    Roof,         // Roof covering
    Trim,         // Decorative elements
}
```

## Entrance Placement

Each building has a designated entrance:

```rust
find_entrance(building, city_block) -> Point3D
```

**Rules:**
- Must face a street or path
- At ground level
- Not blocked by terrain
- Connected to building interior

## Multi-Story Buildings

Tall buildings have additional considerations:

- Consistent stair placement across floors
- Structural support columns
- Floor-to-floor height variation
- Roof access (flat roofs)

## Example Usage

```rust
// Place all buildings in urban districts
for super_district in urban_super_districts {
    let city_blocks = create_city_blocks(&super_district);

    for block in city_blocks {
        let buildings = place_on_grid(&block, &templates);

        for building in buildings {
            build_foundation(&building, &mut editor).await;

            for floor in 0..building.floors.len() {
                build_floor(&building, floor, &mut editor).await;
            }

            build_walls(&building, &mut editor).await;
            build_roof(&building, &mut editor).await;
            build_stairs(&building, &mut editor).await;
        }
    }
}
```

## Configuration

Building generation parameters:

- **Min/Max Size** - Building footprint range
- **Floor Heights** - Ceiling heights per floor
- **Window Frequency** - Window placement density
- **Door Width** - Single or double doors
- **Roof Overhang** - Eaves extension

## Walls System

The settlement can have defensive walls:

```rust
generate_walls(urban_super_districts, editor) -> ()
```

**Wall Components:**
- Wall segments along urban perimeter
- Towers at corners and intervals
- Gates for entry points
- Walkways on top

Gates are positioned based on district border analysis and path routing needs.
