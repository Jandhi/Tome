# Resource Building Placement

## Overview

After `ResourceRegistry::resolve()` assigns each rural district a raw resource and a required building type (Phase 4 of the resource chain plan), we need to physically place that building somewhere inside the district. Rural districts are mostly natural terrain — no city blocks, no pre-cleared plots.

Resource buildings are **prebuilt NBT structures** living in `data/structures/resource_buildings/`. They are loaded via the existing `Structure` `Loadable` and placed via `place_structure()` in `src/generator/nbts/place.rs`

This plan covers picking a good spot inside a district, preparing the ground, and calling `place_structure` with the right offset and facing direction.

---

## Inputs and Outputs

**Input**
- `district: &District` — a rural district with `points_2d` and `edges` populated and classified `DistrictType::Rural`
- `structure: &Structure` — loaded from `data/structures/resource_buildings/<building>.json`; the `id` matches the `building` field of the recipe assigned to this district
- `rng: &mut RNG` — derived from the district ID for determinism
- `editor: &mut Editor`
- `data: &LoadedData` — needed by `place_structure` for palettes

**Output**
- Side effect: site cleared, ground flattened, NBT placed via `place_structure(...)`
- Returns `anyhow::Result<()>` (or `Option<()>` with a logged warning when no viable site exists)

---

## Reading the Footprint

`Structure.meta.path` points at the `.nbt` file. The `NBTStructure` parsed from it carries `size: [i32; 3]` = `[x, y, z]`.

We need only `(size_x, size_z)` for footprint scoring. Parse the NBT once at the start of placement (or cache size on `Structure` post-load if we want to avoid re-parsing — see "Open Questions"). The `Structure.origin: Point3D` tells us where the anchor sits within those bounds, which we use later to compute the placement `offset`.

### Footprint with rotation

`place_structure` computes `rotation = Rotation::from(direction) - Rotation::from(structure.facing)`. We need to mirror that math for footprint scoring so the cells we score are the cells the NBT will actually fill.

Let `(sx, sz) = (structure.size.x, structure.size.z)` and `(ox, oz) = (structure.origin.x, structure.origin.z)`.

| Rotation | World footprint dims | Anchor offset within rect (`(dx, dz)` from rect min to centre point) |
|---|---|---|
| `None`   (0°)   | `(sx, sz)` | `(ox, oz)` |
| `Once`   (90°)  | `(sz, sx)` | `(sz - 1 - oz, ox)` |
| `Twice`  (180°) | `(sx, sz)` | `(sx - 1 - ox, sz - 1 - oz)` |
| `Thrice` (270°) | `(sz, sx)` | `(oz, sx - 1 - ox)` |

Given a candidate `(centre, direction)`:
1. Compute `(fw, fd)` = footprint dims for that rotation.
2. Compute `(dx, dz)` = anchor offset within the rect.
3. Footprint rect min corner = `centre - (dx, dz)`, max corner = `min + (fw - 1, fd - 1)`.
4. Score every cell in that rect.

This is also what we hand to the build-claim step in Step 6.

### Foundation detection

NBT designers may or may not include sub-grade foundation blocks. Convention: `Structure.origin.y` is the floor level. To detect at load time, extend `Structure::post_load` to parse each NBT once and cache:

```rust
pub size_xz: (i32, i32),
pub has_subgrade: bool,   // true iff any block has pos[1] < origin.y
```

Placement formula:
- `offset.y = target_y + 1` (floor sits one above the flattened surface)
- If `!has_subgrade`, decrement `offset.y` by 1 to embed the lowest row in the ground (avoids "floating building" gaps when the NBT has no foundation)

This keeps the convention simple — designers don't need a separate flag, the geometry of the NBT itself encodes intent.

---

## Step 1 — Candidate Sampling

Sample `N = 10` candidate anchor points from the district's interior.

```text
edge_2d           = district.data.edges.iter().map(|p| p.drop_y()).collect()
candidate_pool    = district.points_2d().difference(edge_2d)
interior_candidates = random_sample(candidate_pool, N, rng)
```

Stripping edges keeps buildings off the district boundary.

For each candidate, also pick a **facing direction** (one of the four `Cardinal`s) randomly per candidate — different facings yield different footprint rectangles when the building isn't square, and a candidate that's bad facing north might be great facing east. So a "candidate" is the pair `(centre_point, facing)`.

A candidate is rejected outright if:
- Its rotated footprint rect would extend outside `district.points_2d` (any cell of the building outside the district), **or**
- Any cell in the rotated footprint is already claimed: `editor.world().is_claimed(cell)` returns `true`. This catches overlap with previously-placed resource buildings, walls, paths, or buildings_v2 structures generated earlier in the pipeline.

---

## Step 2 — Scoring

Score each surviving candidate by summing three penalties (lower is better):

### 2a. Flatness penalty

Heights from `world.get_non_tree_height(p)` for every cell of the footprint. Penalty = standard deviation of those heights. High deviation means more terraforming and worse visual result.

### 2b. Water penalty

For every cell in the footprint call `world.is_water(p)`.

- Any water inside the footprint → **hard reject** (skip the candidate)
- Water blocks within a configurable margin around the footprint → soft penalty proportional to count

### 2c. Edge proximity penalty

```text
edge_penalty = 1.0 / (1.0 + min_manhattan_distance_to_edge as f32)
```

Pushes placement away from the district perimeter.

### 2d. Road proximity bonus

If roads already exist in the district at placement time (i.e., paths were generated before resource buildings), prefer cells near a road. Scan a search radius `ROAD_SEARCH_RADIUS = 8` around the footprint and find the nearest cell with `BuildClaim::Path(_)`:

```text
nearest_road_dist = min Manhattan distance from footprint to any cell where
                    matches!(world.get_claim(cell), Some(BuildClaim::Path(_)))
road_bonus        = if nearest_road_dist <= ROAD_SEARCH_RADIUS {
                        -(ROAD_SEARCH_RADIUS - nearest_road_dist) as f32
                    } else { 0.0 }
```

Negative penalty (= score reduction). When no roads have been generated yet, every candidate gets `road_bonus = 0`, so this term is a no-op without breaking placement order.

### Combined score

```text
score = FLATNESS_WEIGHT * flatness_stddev
      + WATER_WEIGHT    * water_margin_count
      + EDGE_WEIGHT     * edge_penalty
      + ROAD_WEIGHT     * road_bonus
```

Suggested starting weights: `FLATNESS_WEIGHT = 2.0`, `WATER_WEIGHT = 1.5`, `EDGE_WEIGHT = 1.0`, `ROAD_WEIGHT = 1.0`.

---

## Step 3 — Site Selection

Pick the candidate `(centre, facing)` with the lowest score. If every candidate was hard-rejected, log a warning and return without placing for that district. Don't fall back to a clearly-bad spot — leaving the district unbuilt is preferable to a sawmill in a swamp.

---

## Step 4 — Site Preparation

### 4a. Clear vegetation

Use the existing tree-cutter on the footprint plus a **2-block yard margin** so the building is visibly separated from the surrounding forest:

```rust
log_trees(editor, expanded_footprint_2d).await;   // footprint expanded by YARD_RADIUS in each axis
```

`YARD_RADIUS = 2`. The yard is purely visual — it is *not* claimed in the build-claim map (see Step 6).

### 4b. Flatten terrain (with tapered blend)

A flat plateau cut into a sloped landscape looks awful — the building ends up sitting on a featureless mesa. We flatten the footprint exactly, then taper the terraforming outward so it blends into the surrounding terrain.

1. `target_y` = median of `world.get_non_tree_height(p)` over the footprint cells. Median is more robust than mean against small cliffs.
2. **Inner ring (footprint cells)**: forced flat to `target_y`. Build a `HashSet<Point3D>` with `(x, target_y, z)` for every footprint cell and call `force_height(editor, &points, false).await`.
3. **Blend ring (margin cells, radius `BLEND_RADIUS = 4` around the footprint)**: each cell is terraformed to a height that **interpolates** between `target_y` and the natural ground height. For a cell at distance `d` from the footprint edge (1 ≤ d ≤ BLEND_RADIUS):
   ```text
   t            = d / BLEND_RADIUS                     // 0 at footprint edge, 1 at outer edge
   natural_y    = world.get_non_tree_height(cell)
   blended_y    = round(lerp(target_y, natural_y, t))
   ```
   Build a separate `HashSet<Point3D>` of `(x, blended_y, z)` cells and call `force_height` on it.
4. **Hard cap on terraforming.** If a blend cell's `|natural_y - target_y| > MAX_BLEND_DELTA` (default 4), skip it — beyond a point, taper turns into a giant earthworks scar. The site still works; the surrounding cliff just stays as-is.

`skip_water = false` for the inner footprint so residual water is overwritten. Use `skip_water = true` for the blend ring so we don't accidentally fill a lake at the building's edge.

Net effect: footprint is perfectly flat for the NBT to sit on, and the next 4 blocks ramp smoothly to natural ground instead of cliff-edging.

---

## Step 5 — Place the NBT

The `place_structure` signature:

```rust
place_structure(
    editor,
    placer,           // None — these structures don't use material palette swaps yet
    structure,
    offset,           // Point3D — where to anchor the structure
    direction,        // Cardinal — chosen facing from Step 3
    Some(data),
    None,             // palette
    false, false,     // mirror_x, mirror_z
).await
```

`offset = Point3D::new(centre.x, target_y, centre.z)`. The `Structure.origin` shift is already handled inside `place_structure` (`transform.shift(rotation.apply_to_point(-structure.origin))`), so we pass the world-space anchor and the function takes care of the rest.

If we later want palette-aware placement (so a sawmill blends with the district's biome materials), pass a `Placer` and the relevant palette — the path is already there in `place_nbt`.

---

## Step 6 — Claim the Cells

After the NBT is placed, mark the footprint as claimed in the world's build-claim map so subsequent district placements (or any other generator stage) won't build on top of it.

Extend the `BuildClaim` enum (`src/generator/build_claim.rs`) with a new variant for prebuilt resource structures:

```rust
pub enum BuildClaim {
    Nature,
    Wall,
    Gate,
    Path(PathType),
    Building(BuildingID),
    Structure(StructureId),   // new — resource buildings and any other prebuilt NBTs
    None,
}
```

Then claim every cell in the footprint:

```rust
for cell in footprint_cells_2d {
    editor.world_mut().claim(cell, BuildClaim::Structure(structure.id.clone()));
}
```

**Only the footprint is claimed** — the blend ring is left as natural terrain. Adjacent rural buildings can share blend rings without one blocking the other, and roads/paths generated later are free to cross the tapered terrain near a structure.

The visualizer/snapshot code (`src/visualizer/snapshot.rs`) needs to handle the new variant in any `match BuildClaim` it currently does (a colour or render style for `Structure`).

---

## Integration Point

New module: `src/generator/placement/placement.rs`.

```rust
pub async fn place_resource_building(
    district: &District,
    structure: &Structure,
    rng: &mut RNG,
    editor: &mut Editor,
    data: &LoadedData,
) -> anyhow::Result<()>
```

Called from Phase 4 of the resource chain pipeline: after `registry.resolve()` assigns a recipe (and therefore a `building` id) to each rural district, look up the corresponding `Structure` in `data.structures` by `StructureId(building.clone())` and pass it into `place_resource_building`.

The placement system is **not** responsible for choosing which building goes where — `registry.resolve()` does that based on the district's biome and raw resource. Placement just receives `(district, structure)` pairs and finds a good spot.

Recipe `building` strings (`sawmill`, `carpenter`, …) must match the structure JSON `id` fields. The set in `data/structures/resource_buildings/` already lines up with recipe building names.

### Load-time validation

Add to `ResourceRegistry`'s existing load-time validation pass: every `recipe.building` value must resolve to a loaded `Structure` with matching `StructureId`. Today the alignment is a happenstance of the data on disk; without enforcement, dropping or renaming an NBT silently breaks placement at generation time instead of at startup.

Concretely, when the registry is constructed and the structure registry is also available (both live in `LoadedData`), iterate over all recipes and assert the lookup succeeds. Surface failures as an `anyhow::Error` listing every missing structure id, so the operator sees the full set in one go rather than fixing them one at a time.

Optionally also assert each referenced `Structure` lives under `data/structures/resource_buildings/` — protects against a recipe accidentally pointing at a city building NBT.

---

## Tests

Live `cargo test` requires a Minecraft server (per CLAUDE.md), so split into two layers:

### Pure unit tests (no server)

Cover the deterministic, world-free pieces. Live in `src/generator/placement/test.rs` (or wherever the new module lands).

- **Footprint rotation**: given `size = (5, 1, 3)` and `origin = (1, 0, 2)`, assert the footprint dims and anchor offsets match the table in "Footprint with rotation" for all four `Cardinal`s.
- **Scoring**: build a small synthetic heightmap (e.g., `Vec<Vec<i32>>`), run the flatness, water-margin, and edge-proximity penalties on it, assert the values. Refactor scoring so it accepts a heights+water grid rather than `&Editor`, so tests don't need a `World`.
- **Hard rejects**: footprint with a water cell inside, footprint extending outside `points_2d`, footprint overlapping a claimed cell — each should be rejected.
- **Median target_y**: pass an array with cliff cells, verify median is robust.

### Integration tests (live server)

Live in `src/generator/placement/test.rs` gated on the same `#[tokio::test]` pattern other modules use.

- **Happy path**: synthesize a small flat district, run `place_resource_building` with the `sawmill` structure, assert: footprint cells are flat (all heights equal `target_y`), no trees on the footprint, footprint cells are claimed, and one block on the footprint matches a non-air block from the NBT.
- **Sloped terrain blend**: district with a 6-block ramp through it. Assert the blend ring's heights monotonically transition from `target_y` toward natural ground over `BLEND_RADIUS` cells.
- **Water rejection**: district with a pond covering most of its area. Assert the function returns gracefully (no panic, no placement) and logs the skip.
- **Determinism**: run twice with the same seed and district, assert identical placement (same offset and direction).

### Test fixture suggestion

A helper `make_test_district(width, depth, height_fn, water_fn)` that builds a `District` + minimal `World` from closures — used by both layers. Worth the small upfront cost; placement is exactly the kind of code that grows quietly broken without it.

---

## Constants (to tune)

| Constant | Default |
|---|---|
| `NUM_CANDIDATES` | 10 |
| `WATER_MARGIN_RADIUS` | 4 |
| `BLEND_RADIUS` | 4 |
| `MAX_BLEND_DELTA` | 4 |
| `YARD_RADIUS` | 2 |
| `ROAD_SEARCH_RADIUS` | 8 |
| `FLATNESS_WEIGHT` | 2.0 |
| `WATER_WEIGHT` | 1.5 |
| `EDGE_WEIGHT` | 1.0 |
| `ROAD_WEIGHT` | 1.0 |

