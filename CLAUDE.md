# Tome

Procedural Minecraft settlement generator in Rust. Talks to a Minecraft server via the GDMC HTTP Interface mod.

## Build & Test

```bash
cargo build
cargo check
cargo test                                      # Most tests need a live Minecraft server
cargo test build_furnished_houses_offline       # Dry-run pipeline — no server needed
cargo test pipeline_invariants_property_test    # 240 buildings × invariant checks
cargo run                                       # Needs live server
```

### Offline / dry-run mode

`World::synthetic(build_area, ground_y)` builds a flat world without any HTTP traffic, and `Editor::new_offline` (or `World::get_offline_editor`) produces an editor that short-circuits `flush_buffer` and `place_block_no_update`. Block placements still land in `block_cache` so reads stay consistent, but nothing reaches the server. Use this for iterating on buildings_v2 generator logic and blueprint output locally. `build_furnished_houses_offline` in `src/generator/buildings_v2/rooms/test.rs` is the canonical example.

### Diagnostics

- `render_ascii(&Blueprint)` in `src/generator/buildings_v2/blueprint.rs` produces a terminal-friendly per-floor ASCII dump with BR cells (`*`), stairs (arrows + `/`), doors, windows, and furniture (single-character codes). `build_furnished_houses_offline` writes both SVG and `.txt` ASCII per building under `output/`. Use the ASCII dump when diagnosing cell-level layout issues — it's much faster to read than SVG XML.
- `check_building_invariants(&frame, &room_plan)` in `rooms/mod.rs` asserts that (a) every interior-edge cell has an actual wall block on its outside, and (b) every `BlockedReachable` cell has a walkable neighbor after furnishing. Called by `run_furnished_houses_pipeline` per building.
- `pipeline_invariants_property_test` runs 12 buildings × 20 seeds through the offline pipeline with invariant checks — the canonical regression guard for furnish/rooms/walls changes. Runs in ~7 seconds.

### CellState semantics

- `Empty` — walkable, open for furniture placement.
- `Blocked` — has a block, not walkable, not placeable (stair step blocks, furniture cells, walls).
- `BlockedReachable` — not walkable, not placeable, must have a walkable neighbor (furniture approach cells like chest fronts). Check the invariant tests if you're introducing new BR state.
- `UnblockedReachable` — walkable, not placeable (door entrances, stair landings/approaches, stair tops, attic ladder cells, window ceilings). Use this for cells the player walks *through* or *on*, not cells adjacent to furniture.

## Layout

- `src/main.rs` — entry point, declares all top-level modules (no `lib.rs`)
- `src/editor/` — `Editor` (block writes, RefCell interior mutability) and `World` (heightmaps, chunks, parcels, build claims)
- `src/generator/` — parcels, buildings, buildings_v2, materials, terrain, paths, nbts, chronicle, style
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

`generate_parcels()` → `get_city_blocks_and_off_limits()` (Voronoi partition of urban area) → per-block building placement. City block inner points become a `Plot` for buildings_v2.

## Palette Merging

Roof material variety: merge a roof-only palette onto the base via `base_palette.clone().merged_with(&roof_palette)`. Roof palettes live in `data/palettes/roofs/` and only define `primary_roof` + `secondary_roof`.
