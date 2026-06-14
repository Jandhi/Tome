# Architecture Overview

This document describes the high-level architecture of Tome and how its components interact.

## System Diagram

```
┌─────────────────────────────────────────────────────────┐
│                      MAIN ENTRY POINT                   │
│                    (main.rs - async loop)               │
└──────────────────┬──────────────────────────────────────┘
                   │
      ┌────────────┴────────────┐
      ▼                         ▼
┌──────────────────┐    ┌──────────────────┐
│   HTTP PROVIDER  │    │    WORLD MODEL   │
│ (GDMC Interface) │    │  (Editor + Chunks)│
└────────┬─────────┘    └────────┬─────────┘
         │                       │
         └───────────┬───────────┘
                     ▼
         ┌──────────────────────────┐
         │  GENERATOR SUBSYSTEMS    │
         ├──────────────────────────┤
         │  • Parcels             │
         │  • Buildings             │
         │  • Materials & Palettes  │
         │  • Terrain               │
         │  • Paths & Routing       │
         │  • NBT Structures        │
         │  • Chronicle (Narrative) │
         │  • AI Integration        │
         └──────────────────────────┘
```

## Core Components

### Entry Point (`src/main.rs`)

The main function orchestrates the entire generation process:

1. Initializes the GDMC HTTP provider to connect to Minecraft server
2. Creates/loads the World state from the server
3. Executes generation pipeline in sequence
4. Flushes all pending block changes

### HTTP Provider (`src/http_mod/`)

Handles all communication with the Minecraft server via the GDMC HTTP Interface.

**Key Features:**
- Async HTTP client using `reqwest`
- Automatic retry with exponential backoff
- Request/response logging
- Batch block operations for performance

**Endpoints Used:**
- `GET /blocks` - Read block data
- `PUT /blocks` - Write blocks (batched)
- `GET /heightmap` - Query terrain height
- `GET /chunks` - Get chunk NBT data
- `POST /command` - Execute Minecraft commands
- `GET /biome` - Query biome data

### World Model (`src/editor/world.rs`)

Represents the Minecraft world state including:

- **Heightmaps** - Ground level, ocean floor, motion-blocking
- **Chunks** - Stored NBT data from Minecraft
- **Parcel Maps** - 2D arrays mapping coordinates to parcels
- **Build Claims** - Tracks ownership of each block location

### Editor (`src/editor/editor.rs`)

The main interface for modifying the world:

- **Block Buffer** - Batches ~32 blocks before flushing
- **Block Cache** - Local tracking of placed blocks
- **Density System** - Won't overwrite denser blocks
- **Async Flushing** - Non-blocking write operations

## Generator Subsystems

### Parcels (`src/generator/parcels/`)

Divides the build area into zones using Voronoi partitioning.

**Types:**
- `Urban` - Where buildings are placed
- `Rural` - Farms, outskirts
- `OffLimits` - Water, steep terrain, protected areas

### Buildings (`src/generator/buildings/`)

Places and constructs buildings within urban parcels.

**Pipeline:**
1. City block creation (Voronoi clustering)
2. Grid-based placement
3. Foundation construction
4. Floor layout
5. Wall building
6. Roof construction
7. Interior stairs

### Materials (`src/generator/materials/`)

Data-driven block selection system.

**Features:**
- JSON-based material definitions
- Palette swapping for visual variety
- Material connections (lighter/darker, worn/pristine)
- Form variants (full blocks, slabs, stairs)

### Terrain (`src/generator/terrain/`)

Modifies existing terrain for construction.

**Operations:**
- Tree removal from build sites
- Ground leveling
- Height smoothing
- Vegetation clearing

### Paths (`src/generator/paths/`)

Generates walkways and roads.

**Algorithms:**
- A* pathfinding considering terrain cost
- Building-to-building routing
- Parcel connection

### NBT Structures (`src/generator/nbts/`)

Places pre-built Minecraft structures.

**Features:**
- NBT file loading
- 4-way rotation support
- Position transformation
- Palette substitution

## Data Flow

```
┌─────────────┐
│ JSON Files  │ (data/)
└──────┬──────┘
       │ Loadable trait
       ▼
┌─────────────┐    ┌─────────────┐
│  Materials  │    │  Palettes   │
└──────┬──────┘    └──────┬──────┘
       │                  │
       └────────┬─────────┘
                ▼
       ┌─────────────────┐
       │   Generator     │
       └────────┬────────┘
                │
                ▼
       ┌─────────────────┐
       │    Editor       │
       └────────┬────────┘
                │ HTTP PUT /blocks
                ▼
       ┌─────────────────┐
       │   Minecraft     │
       └─────────────────┘
```

## Coordinate System

Tome uses Minecraft's coordinate convention:
- **X** - East/West (horizontal)
- **Y** - Up/Down (vertical)
- **Z** - North/South (horizontal)

All 2D maps are indexed as `map[x][z]`.

## Concurrency Model

- **Tokio Runtime** - Async executor for I/O operations
- **Single-threaded generation** - Generation logic runs sequentially
- **Async I/O** - HTTP operations are non-blocking
- **Buffered writes** - Block placements are batched

## Error Handling

- `anyhow::Result` for most fallible operations
- Retry logic for transient HTTP failures
- Graceful degradation (e.g., AI naming falls back to defaults)

## Extension Points

### Adding New Materials
1. Create JSON file in `data/materials/`
2. Define block mappings for each form
3. Add to palette if needed

### Adding New Building Types
1. Define building template in `data/buildings/`
2. Implement generation logic in `src/generator/buildings/`
3. Register in building set

### Adding New Structure NBTs
1. Export structure from Minecraft as NBT
2. Place in appropriate `data/` directory
3. Load via NBT system
