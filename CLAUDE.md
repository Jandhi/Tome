# Tome

Procedural Minecraft settlement generator in Rust. Talks to a Minecraft server via the GDMC HTTP Interface mod.

## Build & Test

```bash
cargo build
cargo check
cargo test    # All tests need a live Minecraft server with GDMC mod
cargo run     # Same requirement
```

## Layout

- `src/main.rs` — entry point, declares all top-level modules (no `lib.rs`)
- `src/editor/` — `Editor` (block writes, RefCell interior mutability) and `World` (heightmaps, chunks, districts, build claims)
- `src/generator/` — districts, buildings, buildings_v2, materials, terrain, paths, nbts, chronicle, style
- `src/geometry/` — Point2D, Point3D, Rect2D, Rect3D, Cardinal, Voronoi
- `src/minecraft/` — Block, BlockID, BlockForm, Chunk, Biome
- `src/http_mod/` — async GDMC HTTP client (reqwest + retry)
- `src/noise/` — seed-based deterministic RNG
- `src/data/` — `Loadable` trait: generic JSON loader from `data/` directory tree
- `data/` — JSON files: materials, palettes, building wall/roof components and sets, NBT structures, forests

## Key Patterns

- **Coordinates**: X (east/west), Y (up/down), Z (north/south). 2D maps indexed `map[x][z]`.
- **Editor RefCell**: block_buffer, block_cache use interior mutability. Never hold borrows across `.await`.
- **Loadable trait** (`src/data/loadable.rs`): implement `path()`, `get_key()`, `post_load()` to auto-load JSON from `data/<path>/`.
- **RNG**: `noise::RNG` — seed-based. Use `rng.derive()` for child RNGs.
- **Errors**: `anyhow::Result` throughout.
- **Async**: Tokio runtime. Generation is single-threaded; I/O is async. Block writes batched (~32) then flushed via HTTP.
- **Tests**: `#[cfg(test)] mod tests` in `test.rs` files, using `#[tokio::test]`.
- **Block struct** (`src/minecraft/block.rs`): `Block { id: BlockID, state: Option<HashMap<String,String>>, data: Option<String> }`. Constructed via `Block::new()`, `Block::from_id()`, or `"block_id".into()`. States hold facing, part, type, etc. `string_to_block()` parses `"id[key=val,...]"` format.
- **Data-driven**: materials, palettes, wall/roof components all defined in JSON. Styles (e.g. `Style::Medieval`) select which sets to use.
- Rust 2021 edition, stable MSVC toolchain, `.env` for API keys (dotenv).

## buildings_v2 Pipeline

Modules must run in order: `footprint` → `foundation` → `frame` → `walls` (build_segments, place_doors) → `floors` (place_floors) → `walls` (place_wall_infill, place_frame, place_openings) → `roof` → `rooms` → `furnish`

SizeClass controls shape: Cottage (small, 1 floor), House (1-2 floors), Hall (2-3 floors, complex), Manor (2-3 floors, largest).

## Settlement Pipeline

`generate_districts()` → `get_city_blocks_and_off_limits()` (Voronoi partition of urban area) → per-block building placement. City block inner points become a `Plot` for buildings_v2.

## Palette Merging

Roof material variety: merge a roof-only palette onto the base via `base_palette.clone().merged_with(&roof_palette)`. Roof palettes live in `data/palettes/roofs/` and only define `primary_roof` + `secondary_roof`.
