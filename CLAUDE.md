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
- `src/minecraft/` — Block, BlockForm, Chunk, Biome
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
- **Data-driven**: materials, palettes, wall/roof components all defined in JSON. Styles (e.g. `Style::Medieval`) select which sets to use.
- Rust 2021 edition, stable MSVC toolchain, `.env` for API keys (dotenv).
