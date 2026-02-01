# Buildings V2 Implementation Plan

## Overview

Polygon-based building generation system using vertex-defined footprints instead of the current cell-grid approach. Start with rectangles but design for arbitrary polygons.

**Goal**: Eventually replace the current `buildings/` cell-grid system.

**Approach**: Code-only for now (no JSON data files).

---

## Core Data Structures

### Footprint (`footprint.rs`)

```rust
pub struct Footprint {
    pub vertices: Vec<Point2D>,  // Clockwise winding, closed polygon
}
```

- `rectangle(origin, width, depth)` - Create rectangular footprint
- `edges()` - Get (start, end) vertex pairs
- `contains(point)` - Point-in-polygon test
- `area()` - Shoelace formula
- `bounds()` - Axis-aligned bounding box

### Frame (`frame.rs`)

```rust
pub struct Frame {
    pub footprint: Footprint,
    pub base_y: i32,        // Ground level
    pub wall_height: i32,   // Per floor
    pub floors: u32,
}
```

- `floor_y(floor)` - Y coordinate at floor level
- `corners_at_floor(floor)` - 3D corner positions
- `wall_segments()` - Convert edges to WallSegments
- `bounds()` - 3D bounding box

### WallSegment (`wall.rs`)

```rust
pub struct WallSegment {
    pub start: Point2D,
    pub end: Point2D,
    pub openings: Vec<Opening>,
}

pub struct Opening {
    pub kind: OpeningKind,  // Door, Window, Archway
    pub position: i32,      // Distance along wall
    pub width: i32,
    pub height: i32,
    pub y_offset: i32,
}
```

- `length()` - Wall length in blocks
- `direction()` - Unit direction vector
- `is_axis_aligned()` - Parallel to X or Z axis
- `positions()` - All block positions along wall
- `is_opening_at(pos, y)` - Check if position is an opening

---

## Module Structure

```
src/generator/buildings_v2/
├── mod.rs              # Module exports
├── footprint.rs        # Footprint polygon       [DONE]
├── frame.rs            # Frame skeleton          [DONE]
├── wall.rs             # WallSegment + Opening   [DONE]
├── building.rs         # BuildingV2 composite    [TODO]
├── placement.rs        # Block placement         [DONE]
└── generate.rs         # Door generation         [DONE]
```

---

## Implementation Phases

### Phase 1: Core Types [DONE]
- [x] `mod.rs` with module structure
- [x] `Footprint` with rectangle constructor and edge iteration
- [x] `Frame` with floor/corner accessors
- [x] `WallSegment` with length/direction/openings

### Phase 2: Block Placement [DONE]
- [x] `placement.rs` with:
  - Place corner posts (vertical columns)
  - Fill wall segments (handling openings)
  - Place floor surface within footprint
- [x] Integrate with `Editor` for block placement
- [x] Integrate with `Palette`/`Material` for block selection

### Phase 3: Doors [DONE]
- [x] Define door types in `wall.rs`:
  - Single, Double, Archway variants
- [x] Door placement in `placement.rs`:
  - Place door frames and blocks
  - Handle lintels above doors
- [x] Door generation in `generate.rs`:
  - Ensure at least one door per building
  - Place doors on ground floor only
  - Respect corner spacing rules

### Phase 4: Windows [DONE]
- [x] Define window types in `wall.rs`:
  - Small, Tall, Wide, Large variants
- [x] Window placement in `placement.rs`:
  - Place window frames (glass panes/blocks)
  - Handle lintels above windows
- [x] Window generation in `generate.rs`:
  - Distribute windows based on density
  - Consistent heights per floor
  - Respect spacing rules (corners, between openings)

### Phase 5: Generation Logic
- [ ] `generate.rs` with:
  - Random rectangular footprint (within size constraints)
  - Floor count selection
  - Integrate door generation (Phase 3)
  - Integrate window generation (Phase 4)
- [ ] Hook into district/claim system

### Phase 6: Roof Integration [DONE]
#### Implementation Status: ✅ Complete

**Files Created/Modified:**
- ✅ `src/generator/buildings_v2/roof.rs` - New module with full implementation
- ✅ `src/generator/buildings_v2/mod.rs` - Exported roof types and functions
- ✅ `src/generator/buildings_v2/frame.rs` - Added `roof_base_y()` method
- ✅ `src/generator/buildings_v2/test.rs` - Updated tests with roof generation

**Implemented Features:**
- ✅ `RoofType` enum with `Hip` and `Gable` variants
- ✅ `RoofConfig` struct (pitch, overhang, use_stairs, use_slabs)
- ✅ `Roof` struct with type, base_y, and config
- ✅ `place_gable_roof()` - Slopes on two sides, vertical gable walls
- ✅ `place_hip_roof()` - Slopes on all four sides
- ✅ `place_roof()` - Dispatch based on roof type
- ✅ `RoofRules` struct with auto-selection logic
- ✅ `generate_roof()` - Auto-selects roof type based on building aspect ratio
- ✅ Proper stair orientation using Cardinal directions
- ✅ Material integration via Palette and MaterialRole::PrimaryRoof
- ✅ Test demonstration showing gable, hip, and auto-selected roofs

**Test Results:**
```
test generator::buildings_v2::test::tests::place_simple_frame ... ok
test generator::buildings_v2::test::tests::place_two_story_house ... ok
test generator::buildings_v2::test::tests::place_building_row ... ok
```

### Phase 6: Roof Integration
- [ ] Create new roof system for buildings_v2
- [ ] Support hip and gable roof types
- [ ] Roof generation and placement

---

## Roofs

### Roof Types

| Type | Description | Complexity |
|------|-------------|------------|
| Hip | All sides slope toward peak, no gables | Medium |
| Gable | Two sloping sides with vertical gable ends | Simple |

**Hip Roof Characteristics:**
- Slopes on all four sides
- Peak runs along longest axis (for rectangles)
- Corner ridges meet at peak
- More complex geometry, better wind resistance

**Gable Roof Characteristics:**
- Slopes on two opposite sides only
- Vertical triangular gable walls on other two sides
- Simple rectangular footprints work best
- Easier to construct, common in medieval buildings

### Roof Structure

```rust
pub enum RoofType {
    Hip,
    Gable { facing: Cardinal }, // Direction gable faces (N/S or E/W)
}

pub struct RoofConfig {
    pub roof_type: RoofType,
    pub pitch: f32,              // Slope angle (0.5 = gentle, 1.0 = steep, 1.5 = very steep)
    pub overhang: i32,           // Blocks extending beyond walls (0-2)
    pub use_stairs: bool,        // Use stair blocks vs full blocks
    pub use_slabs: bool,         // Use slabs for smoother slopes
}

pub struct Roof {
    pub config: RoofConfig,
    pub footprint: Footprint,    // Base of roof (usually building footprint)
    pub base_y: i32,             // Height where roof starts
}
```

### Roof Generation Algorithm

**Hip Roof:**
1. Calculate roof peak:
   - For rectangular footprints: peak is a line along longest axis
   - Height = (short_side / 2) * pitch
2. For each XZ position in footprint:
   - Calculate distance to nearest edge
   - Calculate height based on pitch: `h = distance * pitch`
   - Place appropriate block (full, stair, or slab) at that height
3. Create ridges along peak line
4. Add overhang by extending footprint outward

**Gable Roof:**
1. Determine gable orientation (e.g., gables on N/S, slopes on E/W)
2. Calculate peak line along gable axis
3. For each XZ position:
   - If on gable side: place vertical wall blocks up to peak
   - If on slope side: calculate height = distance_to_peak * pitch
   - Place stairs/slabs oriented correctly
4. Ridge along peak
5. Add overhang on slope sides

**Common Steps:**
- Use stair blocks for most roof surface (oriented to face outward)
- Use full blocks for peak/ridge
- Use slabs for gentler transitions
- Account for overhang extending beyond footprint

### Roof Placement

```rust
pub async fn place_roof(
    roof: &Roof,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    // Get roof material (use WoodPillar or PrimaryWood for stairs)
    let roof_block = palette.get_block(
        MaterialRole::PrimaryWood,
        &BlockForm::Stairs,
        materials,
        rng
    );
    
    match roof.config.roof_type {
        RoofType::Hip => place_hip_roof(...),
        RoofType::Gable { facing } => place_gable_roof(..., facing),
    }
}
```

### Integration with Frame

```rust
impl Frame {
    /// Get the Y level where the roof should start (top of walls)
    pub fn roof_base_y(&self) -> i32 {
        self.base_y + (self.floors as i32) * self.wall_height
    }
    
    /// Create a default roof for this frame
    pub fn default_roof(&self, roof_type: RoofType) -> Roof {
        Roof {
            config: RoofConfig {
                roof_type,
                pitch: 1.0,
                overhang: 1,
                use_stairs: true,
                use_slabs: false,
            },
            footprint: self.footprint.clone(),
            base_y: self.roof_base_y(),
        }
    }
}
```

### Roof Generation Rules

```rust
pub struct RoofRules {
    pub prefer_type: Option<RoofType>,  // None = auto-select based on dimensions
    pub pitch: f32,                      // Default pitch
    pub overhang: i32,                   // Default overhang
}

impl Default for RoofRules {
    fn default() -> Self {
        Self {
            prefer_type: None,  // Auto-select
            pitch: 1.0,         // 45-degree slope
            overhang: 1,        // 1 block overhang
        }
    }
}

pub fn generate_roof(
    frame: &Frame,
    rules: &RoofRules,
    rng: &mut RNG,
) -> Roof {
    // Auto-select roof type if not specified
    let roof_type = rules.prefer_type.unwrap_or_else(|| {
        let bounds = frame.footprint.bounds();
        let width = bounds.size.x;
        let depth = bounds.size.y;
        
        // Gable works better for elongated buildings
        if (width - depth).abs() > 4 {
            let facing = if width > depth {
                Cardinal::North  // Gables on N/S, slopes on E/W
            } else {
                Cardinal::East   // Gables on E/W, slopes on N/S
            };
            RoofType::Gable { facing }
        } else {
            RoofType::Hip  // Hip for square-ish buildings
        }
    });
    
    frame.default_roof(roof_type)
}
```

### Implementation Plan

**Module Structure:**
```
src/generator/buildings_v2/roof.rs
```

**Key Functions:**
- `place_hip_roof()` - Generate and place hip roof geometry
- `place_gable_roof()` - Generate and place gable roof geometry
- `calculate_roof_height()` - Compute height at any XZ position
- `get_roof_block_orientation()` - Determine stair/block facing
- `place_roof()` - Main entry point

**Stair Orientation Rules:**
- Stairs face down the slope (water flows off)
- For hip roofs: stairs face away from peak line
- For gable roofs: stairs face away from peak, parallel to gable walls
- Use `BlockForm::Stairs` with proper `facing` state

---

## Integration Points

| Existing System | How to Integrate |
|-----------------|------------------|
| `Point2D` / `Point3D` | Use from `crate::geometry` |
| `Editor` | Use `editor.place_block()` |
| `Palette` / `Material` | Use for block selection |
| `BuildClaim` | Claim footprint area |
| `Cardinal` | Wall facing / door direction |

---

## Doors

### Door Types

| Type | Width | Height | Notes |
|------|-------|--------|-------|
| Single | 1 | 2 | Standard door |
| Double | 2 | 2 | Grand entrances |
| Archway | 2-3 | 3 | Open passage, no door block |

**Placement Rules:**
- Doors only on ground floor (floor 0)
- At least one door per building (entrance)
- Minimum 2 blocks from corners
- Must have solid blocks above (lintel)

### Door Generation

```rust
pub struct DoorRules {
    pub min_count: u32,           // At least 1
    pub max_count: u32,           // Per building
    pub prefer_symmetry: bool,    // Center on wall
}
```

**Algorithm:**
1. Select wall(s) for door placement
2. For each door, find valid position (respecting corner spacing)
3. If `prefer_symmetry`, center door on wall

---

## Windows

### Window Types

| Type | Width | Height | Y Offset | Notes |
|------|-------|--------|----------|-------|
| Small | 1 | 1 | 1 | Basic window |
| Tall | 1 | 2 | 1 | Vertical emphasis |
| Wide | 2 | 1 | 1 | Horizontal emphasis |
| Large | 2 | 2 | 1 | Statement window |

**Placement Rules:**
- Windows on any floor
- Y offset from floor level (typically 1 block up)
- Minimum 1 block from corners
- Minimum 1 block between openings
- Consistent pattern per wall (aligned heights)

### Window Generation

```rust
pub struct WindowRules {
    pub density: f32,             // 0.0-1.0, windows per available space
    pub prefer_symmetry: bool,    // Center or mirror placements
    pub consistent_type: bool,    // Same window type per floor
}
```

**Algorithm:**
1. For each floor, for each wall:
   - Calculate available space (excluding corners, doors, existing windows)
   - Place windows based on density
   - If `prefer_symmetry`, center or mirror placements
   - If `consistent_type`, use same variant for entire floor

---

## Coordinate Convention

- `Point2D.x` = Minecraft X (East/West)
- `Point2D.y` = Minecraft Z (North/South)
- `Point3D.y` = Minecraft Y (Up/Down)
- Clockwise winding when viewed from above (+Y looking down)
