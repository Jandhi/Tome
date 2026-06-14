# Tome - Procedural Minecraft Settlement Generator

Tome is a Rust-based procedural generation system for creating detailed medieval-style settlements in Minecraft. It communicates with a Minecraft server via the GDMC HTTP Interface to place blocks, read world data, and build complex structures.

## Features

- **Voronoi-based Parcel Generation** - Divides the build area into urban, rural, and off-limits zones
- **Procedural Building Placement** - Generates and places buildings within parcels
- **Material Palette System** - Data-driven block selection with color variation and weathering effects
- **Terrain Modification** - Clears trees, levels ground, and adapts to existing terrain
- **Path Generation** - A* pathfinding for roads and walkways between structures
- **NBT Structure Support** - Places pre-built Minecraft structures with rotation support
- **AI-Powered Naming** - Generates settlement names and chronicles via OpenAI integration

## Requirements

- Minecraft server with [GDMC HTTP Interface](https://github.com/Niels-NTG/gdmc_http_interface) mod
- Rust toolchain (for building from source)
- OpenAI API key (optional, for AI-powered naming)

## Project Structure

```
src/
├── main.rs              # Entry point and orchestration
├── editor/              # World editing interface
├── generator/           # Generation subsystems
│   ├── parcels/       # Parcel/zone generation
│   ├── buildings/       # Building placement and construction
│   ├── materials/       # Material and palette management
│   ├── terrain/         # Terrain modification
│   ├── paths/           # Pathfinding and routing
│   ├── nbts/            # NBT structure placement
│   └── chronicle/       # Narrative generation
├── minecraft/           # Minecraft data structures
├── geometry/            # Spatial math utilities
├── http_mod/            # GDMC HTTP communication
├── data/                # Data loading system
├── noise/               # Random number generation
└── ai/                  # AI integration
```

## Documentation

- [Architecture Overview](architecture.md) - High-level system design
- [Parcel System](parcels.md) - How parcels are generated and classified
- [Building System](buildings.md) - Building placement and construction
- [Materials & Palettes](materials.md) - Block selection and material system
- [Editor & World](editor.md) - World editing interface
- [HTTP API](http-api.md) - Communication with Minecraft

## Generation Pipeline

1. **Connect** to Minecraft server via GDMC HTTP Interface
2. **Load** world state (chunks, heightmaps, biomes)
3. **Generate Parcels** using Voronoi partitioning
4. **Place Buildings** within urban parcels
5. **Clear Trees** from construction sites
6. **Build Walls** around the settlement perimeter
7. **Generate Paths** connecting structures
8. **Create Chronicle** with settlement narrative

## Key Concepts

### Data-Driven Design
All materials, building templates, and palettes are defined in JSON files in the `data/` directory. This allows easy customization without code changes.

### Spatial Indexing
The world is indexed using 2D maps for parcels, biomes, heights, and build claims. Coordinates use Minecraft's convention: X/Z horizontal, Y vertical.

### Async I/O
All communication with Minecraft is asynchronous using Tokio. Block placements are buffered and flushed in batches for performance.

### Seed-Based RNG
Random number generation is seed-based for reproducible results. Child RNGs can be derived deterministically.

## License

See LICENSE file for details.
