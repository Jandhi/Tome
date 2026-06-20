# Placement (`src/generator/placement/placement.rs`)

Picks a spot inside a super-parcel, prepares the ground, places a prebuilt NBT structure, and claims its footprint. Used downstream of the resource-chain solver — once each rural super-parcel has been assigned a raw resource (and a gathering building) and the urban area has a list of processing buildings to host, this module turns those abstract assignments into concrete placements on the map.

The module exposes three public entry points and one set of geometry helpers; everything else is private. This doc explains the moving pieces in the order you'd read the file.

---

## Public API

```rust
pub async fn place_rural_building(
    district: &District,
    structure: &Structure,
    rng: &mut RNG,
    editor: &mut Editor,
    data: &LoadedData,
) -> Result<()>
```

Places one structure inside a single (Rural) super-parcel. Used per-assignment from `SettlementProductionResult::parcel_assignments`.

```rust
pub async fn place_urban_building(
    urban_districts: &[&District],
    structure: &Structure,
    rng: &mut RNG,
    editor: &mut Editor,
    data: &LoadedData,
) -> Result<()>
```

Places one structure anywhere in the urban region — it treats *all* urban super-parcels as one big candidate pool, since processing buildings have no fixed home super-parcel.

```rust
pub async fn place_urban_buildings(
    urban_districts: &[&District],
    building_counts: &HashMap<String, u32>,
    rng: &mut RNG,
    editor: &mut Editor,
    data: &LoadedData,
) -> Result<()>
```

Convenience orchestrator. Flattens `building_counts` (e.g. `{"sawmill": 3, "carpenter": 2}`) into a queue of individual buildings, drains it in random order via `rng.pop`, and calls `place_urban_building` for each. Each placement claims its footprint, so subsequent picks naturally steer around what's already been built.

Both functions return `Ok(())` whether or not a placement actually happened — site rejection is logged as a warning, never propagated as an error. The only `Err` case is when `place_structure` itself fails after the site has been chosen.

---

## High-level flow

Both `place_rural_building` and `place_urban_building` follow the same shape:

1. **Build the candidate pool** — interior cells = `points_2d \ edges`. Rural uses one super-parcel's data; urban unions every urban super-parcel.
2. **Sample 10 random centres** (`NUM_CANDIDATES`) from the interior pool via `RNG::choose_many`. Pair each with all four `Cardinal`s — a 5×3 building facing east covers different cells than the same building facing north.
3. **Score each candidate** — see *Scoring* below. Hard reject any with water inside the footprint, claim overlap, wall proximity, or footprint extending outside the urban/super-parcel `points_2d`.
4. **Pick the best** (lowest score) with `select_best_candidate`. If nothing survives, log a warning and return.
5. **`execute_placement`** — clear vegetation, flatten + blend ground, run `place_structure`, claim the footprint.

---

## Footprint geometry

`place_structure` rotates the NBT by `Rotation::from(direction) - Rotation::from(structure.facing)`. For scoring we need the world-space rect those rotated cells will fill — `footprint_rect` mirrors that math.

Let `(sx, sz) = structure.size_xz` and `(ox, oz) = structure.origin.{x,z}`.

| Rotation         | World footprint dims | Anchor offset `(dx, dz)` from rect min to centre |
| ---------------- | -------------------- | ------------------------------------------------ |
| `None`   (0°)    | `(sx, sz)`           | `(ox, oz)`                                       |
| `Once`   (90°)   | `(sz, sx)`           | `(sz - 1 - oz, ox)`                              |
| `Twice`  (180°)  | `(sx, sz)`           | `(sx - 1 - ox, sz - 1 - oz)`                     |
| `Thrice` (270°)  | `(sz, sx)`           | `(oz, sx - 1 - ox)`                              |

Implemented in two free functions for unit testing:

- `footprint_dims_for_rotation(size, rotation) -> (i32, i32)`
- `anchor_offset_for_rotation(size, origin_xz, rotation) -> (i32, i32)`

`footprint_rect(structure, candidate)` combines them: `Rect2D { origin: centre - (dx, dz), size: (fw, fd) }`.

### Foundation embedding

`Structure.size_xz` and `Structure.y_offset` are read straight from the JSON sidecar (in `nbts/structure.rs`). `y_offset` is the depth of subgrade — how many blocks the NBT extends below `origin.y`. Zero means no foundation; `1`, `2`, … indicate progressively deeper foundations/cellars. The maintenance test `migrate_resource_building_metadata` (in `nbts/test.rs`) recomputes these from the NBT and patches the JSON; run it whenever an NBT is replaced.

In `execute_placement`:

```rust
let anchor_y = target_y + structure.y_offset;
```

Adding `y_offset` to the target ground height lifts the anchor so the lowest block of the structure embeds at ground level. With `y_offset = 0` the floor sits on the ground; with `y_offset = 1` the floor is one block up and the foundation row fills ground level, and so on.

---

## Candidate scoring

`score_candidate` computes a `CandidateScore` (lower is better):

| Component       | What it measures                                        | How it's computed |
| --------------- | ------------------------------------------------------- | ----------------- |
| `flatness`      | Standard deviation of footprint heights                 | `sqrt(variance)` over `world.get_non_tree_height(p)` for each footprint cell. |
| `water_margin`  | Water cells within `WATER_MARGIN_RADIUS` of the rect    | Counts, doesn't reject (water *inside* the footprint is a hard reject). |
| `edge_penalty`  | Closeness to the world edge                             | `1 / (1 + min_manhattan_distance_to_oob_cell)`, scanned within `BLEND_RADIUS + WATER_MARGIN_RADIUS`. |
| `road_bonus`    | Negative penalty if a `BuildClaim::Path` cell is nearby | `-(ROAD_SEARCH_RADIUS - dist)`, only if `dist ≤ ROAD_SEARCH_RADIUS`. |

Combined:

```text
total = FLATNESS_WEIGHT * flatness
      + WATER_WEIGHT    * water_margin
      + EDGE_WEIGHT     * edge_penalty
      + ROAD_WEIGHT     * road_bonus
```

When no roads have been generated yet, every candidate gets `road_bonus = 0` — the term is a no-op rather than a tiebreaker. Same idea for `edge_penalty` away from the world border.

---

## Hard rejects

Before scoring, `select_best_candidate` short-circuits any candidate that fails:

1. `rect_inside_points` — every footprint cell must be in the bounds passed in (super-parcel `points_2d` for rural, the unioned urban points for urban). Catches buildings that would extend outside the assigned region.
2. `rect_overlaps_claim` — any cell already in the build-claim map. Catches overlap with previously-placed structures, the wall, paths, or buildings_v2.
3. `rect_too_close_to_wall` — any cell within `WALL_BUFFER_RADIUS` of a `BuildClaim::Wall`. Even after claim-overlap rejection, this guarantees a visible gap between the wall and any building.
4. `score_candidate` returning `None` — at least one cell of the footprint is water.

Reject ordering matters: claim-overlap is cheaper than wall-buffer (smaller scan), and water rejection runs as part of scoring so we don't pay the height stat-collection cost on rejected rects.

---

## Site preparation (`execute_placement`)

Once a winner is chosen, ground prep runs in this order:

1. **Clear vegetation** in the footprint plus a `YARD_RADIUS` margin via `log_trees`. The yard is purely visual — it isn't claimed.
2. **Flatten** the footprint to `target_y` = median of `world.get_non_tree_height(p)` over the footprint cells. Median (not mean) because a single cliff cell shouldn't drag the plateau down. Implemented as a `force_height` call with `skip_water = false` so any residual water inside the footprint is overwritten.
3. **Blend ring** outside the footprint, up to `BLEND_RADIUS` cells. For a cell at distance `d` from the rect:
   ```text
   t        = d / BLEND_RADIUS                  // 0 at footprint edge, 1 at outer edge
   natural  = world.get_non_tree_height(cell)
   blended  = round(lerp(target_y, natural, t))
   ```
   Every ring cell is graded toward natural terrain (no early bail on steep deltas), so the pad edge ramps down instead of dropping off as a cliff. Footprint steepness is bounded earlier by the `MAX_PLACEMENT_SLOPE` hard reject (except `allow_steep` structures like mines, which accept the larger earthworks), keeping the ramp reasonable. `force_height` is called with `skip_water = true` so the blend doesn't fill nearby lakes.
4. **Place the NBT** via `nbts::place_structure`. The world-space anchor is `(centre.x, anchor_y, centre.y)`; the structure's own origin shift is handled inside `place_structure`.
5. **Claim** every footprint cell as `BuildClaim::Structure(structure.id.clone())`. The blend ring is intentionally **not** claimed, so adjacent buildings can share blend rings and roads can cross tapered terrain near a structure.

---

## Constants

| Constant              | Default | Used by                                                   |
| --------------------- | ------- | --------------------------------------------------------- |
| `NUM_CANDIDATES`      | 10      | How many random centres to sample.                        |
| `WATER_MARGIN_RADIUS` | 4       | Soft penalty radius around the footprint for water.       |
| `BLEND_RADIUS`        | 6       | Width of the tapered ground-flattening ring.              |
| `MAX_PLACEMENT_SLOPE` | 4       | Hard-reject footprints whose height range exceeds this (unless `allow_steep`). |
| `YARD_RADIUS`         | 2       | How far around the footprint we clear trees.              |
| `ROAD_SEARCH_RADIUS`  | 8       | Distance to scan for roads when computing the bonus.      |
| `WALL_BUFFER_RADIUS`  | 1       | Minimum gap between a building and any `BuildClaim::Wall`.|
| `FLATNESS_WEIGHT`     | 2.0     | Score weighting.                                          |
| `WATER_WEIGHT`        | 1.5     | Score weighting.                                          |
| `EDGE_WEIGHT`         | 1.0     | Score weighting.                                          |
| `ROAD_WEIGHT`         | 1.0     | Score weighting.                                          |

---

## Function map

```text
place_rural_building / place_urban_building
        │
        ├── build interior pool (points_2d \ edges)
        ├── rng.choose_many(interior, NUM_CANDIDATES)
        ├── select_best_candidate
        │       ├── footprint_rect
        │       ├── rect_inside_points
        │       ├── rect_overlaps_claim
        │       ├── rect_too_close_to_wall
        │       └── score_candidate
        │               ├── world.is_water
        │               ├── world.get_non_tree_height
        │               ├── edge_proximity_penalty
        │               └── road_proximity_bonus
        └── execute_placement
                ├── log_trees                     (yard clear)
                ├── force_height (footprint)      (flatten)
                ├── build_blend_ring + force_height (blend)
                ├── place_structure
                └── world.claim                   (BuildClaim::Structure)

place_urban_buildings
        └── flatten + rng.pop loop → place_urban_building per slot
```

---

## Caller contract

- The structure passed in must have a valid `size_xz` declared in its JSON sidecar. JSONs that omit it default to `(0, 0)`; placement detects this and skips with a warning rather than dividing by zero downstream.
- The build-claim map must already contain whatever non-building features should constrain placement. In particular: build the city wall **before** placing urban buildings, so the wall's claims (and the 1-cell buffer) keep buildings off the perimeter.
- The resource registry validates at load time that every recipe's `building` resolves to a `Structure` under `data/structures/resource_buildings/` — see `ResourceRegistry::validate_buildings`. Placement assumes that contract holds; if a structure is missing it logs a warning per affected super-parcel and skips.

---

## Tests (`src/generator/placement/test.rs`)

Pure unit tests (no server):
- `footprint_dims_*` and `anchor_offset_*` — the rotation math table above.

Integration tests (require a live GDMC server):
- `rural_resource_placement_paints_parcels` — runs the full pipeline (parcels → resource chain → rural placements → urban placements → ground painting) and visualises parcel types as wool colours.
- `rural_and_urban_placement_with_city_wall` — same flow plus `build_wall(... StandardWithInner ...)` *before* placement, demonstrating that the wall's claims push buildings off the perimeter and the `WALL_BUFFER_RADIUS` keeps a gap.
