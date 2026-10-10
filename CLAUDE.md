# Tome

Procedural Minecraft settlement generator in Rust. Talks to a Minecraft server via the GDMC HTTP Interface mod.

## Build & Test

```bash
cargo build
cargo check
cargo test                                      # Most tests need a live Minecraft server
cargo test build_furnished_houses_offline       # Dry-run pipeline — no server needed
cargo test pipeline_invariants_property_test    # 240 buildings × invariant checks
cargo run                                       # Live server: generate, then save fixture + scene and render to output/runs/<seed>/
cargo run -- --full                             # Same, plus the AI-written chronicle book (OpenAI call; off by default)
cargo offline <fixture> [seed]                  # No server: town from a saved terrain fixture -> output/runs/offline-<seed>/ (alias for `cargo run --profile fast -- --offline`: release speed, ~5 s incremental rebuilds)
cargo render <scene> [out_dir] [--light <mode>] # Re-render a saved town.scene, no server (alias for `cargo run --profile fast -- --render`); also writes viewer.html; --light day|night|dusk|<0..1> also writes every view lit into <out_dir>/lit-<mode>/ (see Lighting)
cargo view <scene> [out.html] [--light day|night|dusk|<0..1>]   # First-person 3D viewer of a saved scene as one self-contained HTML file (default viewer.html beside the scene), textured from the client jar's block models when one is found (see Textures; flat colours otherwise); open it in Chrome, click to look, WASD/Space/Shift to fly, F walk, P screenshot; HUD shows /tp coords. --light bakes Minecraft-style lighting into the page (sky + lamps, smooth per vertex, see Lighting) and writes viewer-<day|night|dusk|light0.30>.html instead; without it the page is unchanged
cargo run --profile fast -- --houses [--culture medieval|desert|japanese] [--seed N] [--count N] [--size cottage|house|hall|manor|mixed] [--rects "x,z,w,h;..."] [--jetty] [--px N] [--light day|night|dusk|<0..1>]   # No server: houses on a flat world, per-house crops + a perspective inside every room (<house>_rooms.png sheet) + ASCII plans + houses.json -> output/houses/<culture>-<seed>/ ; --rects pins the footprint (plot-relative, core first) so one shape can be checked under every roof style; --light lights the room views (sky + lamps, see Lighting) into output/houses/<culture>-<seed>-<day|night|...>/
cargo run --profile fast -- --furniture <name,...> [out_dir]   # No server: each furniture item alone in a walled corner, two lit 3/4 views -> output/furniture/<name>.png + sheet.png; check new pieces here before they go into rooms
cargo run -- --fixture <path>                   # Capture terrain only from the live server (~2 s)
cargo run -- --compare <a.scene> <b.scene> [out_dir]  # Block diff of two scenes: table + diff.json + diff_top.png (red only-A, green only-B, yellow changed) into out_dir (default: A's dir)
```

### Saved towns and renders

Every run leaves a dossier under `output/runs/<seed>/` (or `offline-<seed>/`): `terrain.fixture.nbt.gz` (chunks + heightmaps, taken before generation), `town.scene` (terrain surface + every placed block, gzip text, palette-interned `id[state]` strings) and PNG renders (`top.png` hill-shaded top-down, `iso_se.png`/`iso_nw.png` isometric, `elev_{south,north,east,west}.png`, described in `index.txt`), plus `metrics.json` (`TownReport` from `src/generator/report.rs`: counts per stage, NPC jobs, stage timings, returned by `generate_town_with`). Read the PNGs before asking for an in-game check. `src/render/` is the renderer (`render_all`, `render_views` with a region crop for one building); `Editor::capture_scene` builds the scene; `src/editor/fixture.rs` is the fixture.

### Textures

The renderers read Minecraft's own block models and textures from a client jar (`src/render/assets.rs`): `TOME_MC_JAR=<path>` names it, else the newest numbered release under `%USERPROFILE%\curseforge\minecraft\Installersions\` or `.minecraftersions\` is used (`TOME_MC_VERSION=1.21.11` pins one; `TOME_NO_TEXTURES=1` turns textures off). Loading takes ~0.5 s once per process. A block string resolves through `blockstates/<id>.json` (missing properties get defaults, inferred stair shapes and fence links are synthesised first) to model elements with per-face textures; the perspective view samples texels per ray, and the flat views (iso, top-down, elevations) box-filter them per pixel through `src/render/texel.rs` (cut-out texels show what is behind; models with freely rotated elements such as crops keep the flat colour). Chests and decorated pots, which the game draws from entity sheets, get models synthesised from `textures/entity/` (`add_entity_models`). Blocks still without an element model (beds, signs, banners, fluids) and blocks the jar lacks keep the hand-written shape and colour from `colors.rs` / `shape.rs`, which also still decide translucency. Nothing from the jar is stored in the repo.

### Lighting

Off by default; renders are byte-identical unless a renderer is handed a `Lighting { daylight, smooth }` (`src/render/light.rs`). `LightMap::new(&VoxelGrid)` computes Minecraft's sky light (15 under open sky, straight down undiminished, -1 per block into shade) and block light (emitter table in `light.rs`, -1 per block) for every cell in ~1 s for a 160M-cell town; opaque full cubes stop light, partial opaque blocks only through the faces their shape covers (so slab and stair roofs darken attics), water and leaves (opacity 1) dim even straight-down sky light. `brightness(x, y, z, daylight)` maps the brighter channel through the game's ramp over an ambient floor (`AMBIENT`, level 0 stays visible); `face_light_at` is the smooth-lit (corner-averaged, AO-darkened) variant, and the module doc says how a face should be lit; `LightSampler` adds the per-channel colour (warm block light, white sky by day, moonlight blue at night). The perspective views take it through `PerspectiveOptions`; the flat views (iso, top-down, elevations) through `RenderOptions.lighting` / the `render_*_with(.., Option<&Lit>)` variants, with `src/render/lit.rs` lighting each face from the cell on its outside (smooth corners once per face, a bilinear blend and multiply per pixel; outside cells beyond the grid count as open sky) and samples behind the first (water, through glass) by their own cell; the sky background dims too. `--houses --light <mode>` lights the room views and the exterior crops (output in `output/houses/<culture>-<seed>-<mode>/`), `--render <scene> --light <mode>` reports the town's light-map timing and levels and writes the lit views beside the unlit ones into `<out_dir>/lit-<mode>/`. Lighting the 800x800 savanna town costs ~1 s for the map, then ~1.7x on the iso (1.4 s vs 0.85 s), ~1.45x on the top view and ~1.1x on the elevations. `brightness(x, y, z, daylight)` maps the brighter channel through the game's ramp over an ambient floor (`AMBIENT`, level 0 stays visible); `face_light_at` is the smooth-lit (corner-averaged, AO-darkened) variant, and the module doc says how a face should be lit so the flat views can follow. Today the perspective views use it (`PerspectiveOptions`, `--houses --light`) and so does the first-person viewer (`cargo view <scene> --light <mode>`: `src/render/viewer.rs` samples `LightSampler::face_corners_rgb` at every face's corners — the outside neighbour for boundary faces, the block's own cell for inner faces of a partial shape — and embeds 4 x rgb light factors per face that the page multiplies into its vertex colours, so the GPU interpolates the smooth lighting; full-cube faces still greedy-merge, but only across cells whose quantised corner light is identical, so a lamp pool or a shadow stays per cell while a sunlit meadow stays a few quads; the sky/fog takes `sky_rgb` and the HUD names the mode; a town's light map takes ~70 ms and the lit page is about twice the unlit one).

### Determinism

Generation is deterministic in the seed only because generator code imports `crate::util::{HashMap, HashSet}` (fixed hasher, `src/util/stable_hash.rs`). Never use `std::collections::{HashMap, HashSet}` in generator/editor/geometry code (`Block.state` is the exception and stays std). Construct with `default()` / `from_iter` / `with_capacity_and_hasher`. Regression check: `TOME_FIXTURE=<fixture> cargo test --release offline_town_is_deterministic -- --ignored`.

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
