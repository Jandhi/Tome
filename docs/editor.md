# Editor & World Interface

The Editor and World components provide the interface between Tome's generation logic and the Minecraft world. They handle block placement, caching, and world state queries.

## Overview

- **World** - Represents the Minecraft world state (read-only view)
- **Editor** - Provides block modification operations (write interface)

## World (`src/editor/world.rs`)

The World struct holds all data about the current Minecraft world state.

### Data Structures

```rust
pub struct World {
    // Heightmaps
    pub heightmap: Vec<Vec<i32>>,           // Ground surface
    pub ocean_floor: Vec<Vec<i32>>,         // Below water
    pub motion_blocking: Vec<Vec<i32>>,     // Including vegetation

    // Chunk data
    pub chunks: HashMap<(i32, i32), Chunk>,

    // District information
    pub district_map: Vec<Vec<Option<DistrictID>>>,
    pub super_district_map: Vec<Vec<Option<SuperDistrictID>>>,

    // Build tracking
    pub build_claims: Vec<Vec<BuildClaim>>,

    // Bounds
    pub bounds: Rect3D,
    pub build_area: Rect2D,
}
```

### Heightmaps

Three heightmaps track different surface levels:

| Heightmap | Description | Use Case |
|-----------|-------------|----------|
| `heightmap` | Top solid block | Building foundations |
| `ocean_floor` | Below water surface | Underwater builds |
| `motion_blocking` | Includes vegetation | Path routing |

### Querying World State

```rust
// Get ground height at position
let height = world.get_height(x, z);

// Get biome at position
let biome = world.get_biome(x, y, z);

// Get block at position
let block = world.get_block(x, y, z);

// Check if position is in build area
let in_bounds = world.in_build_area(x, z);
```

### Build Claims

Build claims track what has been placed at each location:

```rust
pub enum BuildClaim {
    None,                       // Unclaimed
    Building(BuildingID),       // Part of building
    Path,                       // Part of path
    Wall,                       // Part of wall
    Foundation,                 // Building foundation
    Reserved,                   // Reserved for future use
}
```

**Usage:**
```rust
// Check claim
let claim = world.get_claim(x, z);

// Set claim
world.set_claim(x, z, BuildClaim::Building(building_id));

// Check if area is free
let free = world.is_area_free(rect);
```

### Chunk Access

Chunks store the actual Minecraft block data:

```rust
pub struct Chunk {
    pub sections: Vec<ChunkSection>,
    pub heightmaps: HeightmapData,
    pub biomes: BiomeData,
}

pub struct ChunkSection {
    pub y: i32,
    pub block_states: BlockStates,
}
```

**Coordinate Conversion:**
```rust
// World to chunk coordinates
let chunk_x = x >> 4;  // x / 16
let chunk_z = z >> 4;  // z / 16

// Position within chunk
let local_x = x & 15;  // x % 16
let local_z = z & 15;  // z % 16
```

## Editor (`src/editor/editor.rs`)

The Editor provides a buffered interface for placing blocks.

### Structure

```rust
pub struct Editor<'a> {
    pub provider: &'a GDMCHTTPProvider,
    pub world: &'a mut World,

    // Buffering
    block_buffer: Vec<PositionedBlock>,
    buffer_capacity: usize,  // Default: 32

    // Caching
    block_cache: HashMap<Point3D, Block>,
}
```

### Block Placement

```rust
// Place a single block
editor.place_block(point, block).await;

// Place with density check
editor.place_block_if_less_dense(point, block).await;

// Force flush buffer
editor.flush_buffer().await;
```

### Buffering System

Blocks are buffered to reduce HTTP calls:

1. `place_block()` adds to buffer
2. When buffer reaches capacity (~32 blocks), auto-flush
3. Explicit `flush_buffer()` for immediate write

```rust
async fn place_block(&mut self, point: Point3D, block: Block) {
    self.block_buffer.push(PositionedBlock { point, block });

    if self.block_buffer.len() >= self.buffer_capacity {
        self.flush_buffer().await;
    }
}

async fn flush_buffer(&mut self) {
    if self.block_buffer.is_empty() {
        return;
    }

    self.provider.put_blocks(&self.block_buffer).await;
    self.block_buffer.clear();
}
```

### Block Cache

The cache tracks locally placed blocks:

```rust
// Get block (checks cache first, then world)
let block = editor.get_block(point);

// Check if position was modified
let modified = editor.is_cached(point);
```

### Density System

Blocks have implicit density that prevents overwriting:

```rust
fn get_density(block: &Block) -> i32 {
    match block.id.as_str() {
        "air" => 0,
        "water" => 1,
        "grass" | "flowers" => 2,
        "dirt" | "sand" => 3,
        "wood" | "planks" => 5,
        "stone" | "cobblestone" => 7,
        "obsidian" | "bedrock" => 10,
        _ => 5,
    }
}

// Only places if new block is denser
editor.place_block_if_less_dense(point, block).await;
```

### Batch Operations

For efficiency, multiple blocks can be placed at once:

```rust
// Place multiple blocks
let blocks: Vec<PositionedBlock> = vec![...];
editor.place_blocks(&blocks).await;

// Fill a region
editor.fill_region(rect, block).await;

// Replace blocks in region
editor.replace_in_region(rect, old_block, new_block).await;
```

## Coordinate System

Minecraft uses:
- **X** - East (+) / West (-)
- **Y** - Up (+) / Down (-)
- **Z** - South (+) / North (-)

### Point Types

```rust
pub struct Point2D { pub x: i32, pub z: i32 }
pub struct Point3D { pub x: i32, pub y: i32, pub z: i32 }
```

### Conversion

```rust
// 2D to 3D (at ground level)
let point_3d = world.to_3d(point_2d);

// 3D to 2D (drop Y)
let point_2d = point_3d.to_2d();
```

## Async Operations

All Editor methods that communicate with Minecraft are async:

```rust
pub async fn place_block(&mut self, point: Point3D, block: Block) {
    // ...
}

pub async fn flush_buffer(&mut self) {
    // ...
}
```

**Usage in generators:**
```rust
async fn generate_building(editor: &mut Editor<'_>) {
    for point in building_points {
        editor.place_block(point, wall_block.clone()).await;
    }

    // Ensure all blocks are written
    editor.flush_buffer().await;
}
```

## Best Practices

1. **Always flush at end** - Call `flush_buffer()` when done placing blocks
2. **Use density checks** - Prevents accidental overwrites
3. **Batch when possible** - Reduces HTTP overhead
4. **Check bounds** - Verify positions are in build area
5. **Update claims** - Track what you place in build_claims
6. **Cache reads** - Use cached values when re-reading placed blocks

## Example: Building a Wall

```rust
async fn build_wall(
    editor: &mut Editor<'_>,
    start: Point3D,
    end: Point3D,
    block: Block,
) {
    let mut current = start;

    while current != end {
        // Place column of blocks
        for y in 0..3 {
            let point = current.with_y(current.y + y);
            editor.place_block(point, block.clone()).await;
        }

        // Move to next position
        current = current.towards(end);
    }

    // Ensure all blocks written
    editor.flush_buffer().await;
}
```
