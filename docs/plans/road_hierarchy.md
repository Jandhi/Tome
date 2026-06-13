# Road Hierarchy: Feathered Urban Flatten + Tiered A\* Network

Plan drafted 2026-06-06. Branch: `jd/placement`.

## Problem

Roads today have no hierarchy. There are two disconnected road systems:

- **A\*-routed paths** (`paths/routing.rs` + `paths/building.rs`) — terrain-aware, priority→width baked in (`PathPriority::{Low,Medium,High}` → width `{1,2,3}`), but only used for point-to-point routing, not the urban grid.
- **Partition-derived roads** (the settlement grid) — no hierarchy at all. Legacy `place_buildings` gives every Voronoi block the same 3-cell perimeter ring; the new `subdivide` flow hardcodes every alley to 2 wide (`subdivide.rs:81`) regardless of depth. A top-level parcel split and a leaf plot split produce identically-wide roads.

We want **large > medium > small roads with decreasing density**, and roads that **respect terrain height** (the partitioner is pure-2D and terrain-blind; the post-hoc pave pass just drapes a straight alley over whatever cliff it crosses).

## Approach

Invert the model: **roads first, blocks fall out.** Build a tiered road *network* with A\* (which is genuinely height-aware — y clamped ±2/step, burrowing + water penalties), then later let blocks be the connected components of land minus road cells. The BSP recursion depth that we currently throw away becomes the tier signal (shallow = wide/sparse arterial, deep = narrow/dense lane), with density falling out for free.

**This plan covers the first prototype only:** the feathered urban flatten + the tier-1/2 A\* network. Recursive A\* subdivision into local streets and frontage placement are explicitly **out of scope** here — this validates the two new primitives before we build on them.

### Ordering rationale

Flatten **first**, route **second**. `force_height` updates the heightmap, so A\* then plans over already-gentled terrain → arterials come out naturally flat and the corridor terraform shrinks to almost nothing. This is why flatten-before-route is the right order.

## Reuse — what already exists

We are *not* reinventing road realization. The existing code does it well:

- **Coarse-lattice A\* + dense fill** — `get_neighbours_4` hops 4 cells in `ALL_8`; A\* plans on that lattice; `fill_out_path` densifies one tile at a time with `can_update_y` toggling (climbs ≤1 per 2 tiles).
- **Turn penalty** — the `wobble` term in `get_cost` (`routing.rs:122`) penalizes direction change → straight roads. Plus `burrowing_cost` and a water penalty. This is the per-tier tuning surface.
- **Slab-remainder grading** — `build_path` carries height as a *float*, smooths it 3× with a local min/max bump pass, and lays a **slab** when the fractional remainder > 0.3 (`building.rs:90`). This is how you get smooth half-step grades. `build_path` also widens by `width-1`, clears air above, and uses `MaterialPlacer` for variety. **`build_path` *is* the corridor terraform + paint** — Phase 3 mostly collapses into "call `build_path` per `Path`."

Wiring note: `routing` and `building` are **private modules** (`paths/mod.rs` only re-exports `a_star`, `Path`, `PathPriority`, `PathType`). Add `pub use routing::get_path; pub use building::build_path;`.

### Sparse vs per-tile A\* — decision

Sparse (mod-4) is **correct for tiers 1–2** (perf, straightness, allowed earthworks) and **wrong for the future local tier** (terrain-blind between waypoints, can't thread <4-cell gaps, can't dodge exact building footprints, snaps endpoints up to ~4 cells).

Decision for this prototype: keep **sparse (`step=4`) for v1 arterials** — at route time the ground is freshly flattened and no buildings exist yet, so the two things per-tile buys (terrain faithfulness, claim-dodging) don't bite. But **build the A\* step-parameterized now** so "normal A\* in the city / sparse outside" is the same function with a different `step` (+ per-tier cost weights), not two algorithms.

The current A\* node state is the **entire `Vec<Point3D>` path**, and `closed_set` dedupes whole paths, not cells — survivable at mod-4 over short hops, but it would explode at `step=1` over parcel spacing. So the `(cell, incoming_dir)` + `came_from` state refactor is **deferred** to when we add the local/subdivision tier that actually needs `step=1`.

---

## Phases

### Phase 0 — A\* prep (precondition)
- Fix latent bug: `routing.rs:82,94` `if !point.y.abs_diff(neighbour.y) > 4` — `!` is bitwise-NOT on the `u32`, so it's ~always true and the 2-step fallback is dead code. Restore the intended grade check.
- Parameterize the router by **step size** (replace the hardcoded `*4`/`*2`) and **cost weights** (turn penalty, grade clamp, water/burrow) so tiers tune behavior. Default `step=4`.
- Add `pub use routing::get_path; pub use building::build_path;` to `paths/mod.rs`.
- **State refactor (`Vec` → `(cell, dir)` + `came_from`) is NOT in this phase** — deferred to the local tier.

### Phase 1 — `flatten_urban_area` (new, in `generator/terrain/`)
```rust
pub async fn flatten_urban_area(editor: &mut Editor, urban: &HashSet<Point2D>,
                                feather: i32, smooth_iters: usize, skip_water: bool)
```
1. Natural height field: `urban.iter().map(|p| world.add_non_tree_height(p))` → `HashSet<Point3D>`.
2. **Smoothed target:** `average_to_neighbours_5_away_multi(&field, smooth_iters)` (broad, gently-graded — *not* a flat mesa).
3. **Feather:** multi-source BFS distance transform from the urban boundary (cells with a non-urban cardinal neighbour). `t = clamp(dist / feather, 0, 1)`; `final_y = lerp(natural_y, smoothed_y, t)`. Edge → natural terrain, core → fully smoothed.
4. Apply with `force_height(editor, &final_points, skip_water)` (cut/fill columns + `set_heights`).

### Phase 2 — `build_road_network` (new, `generator/paths/network.rs`)
**Center-finding:**
- **Parcel centers:** per urban super-parcel, centroid of `data.points_2d`, snapped to nearest member cell (concave-safe), then `add_height` (post-flatten y).
- **Town center:** centroid of all urban points, snapped. *(Toggle — include as a backbone node for the radial feel.)*
- **Gate nodes:** each `gate_locations` entry stepped a few cells inward along its `Cardinal`.

**Graph + tiers:**
- **Tier 1 — arterials** (`PathPriority::High`, width 3): MST (straight-line distance) over {town center} ∪ {parcel centers}.
- **Tier 2 — collectors** (`PathPriority::Medium`, width 2): each gate → nearest backbone node.
- Route each edge with `get_path(editor, start, end, priority, material, no-op callback)`; `None` → log + skip (partial connectivity OK for v1). Returns `Vec<Path>`.

### Phase 3 — realize
- `build_path` per routed `Path` (reuses slab-smoothing + widening). Phase 1 handles broad grading; `build_path` handles the road surface. **Gap (later):** `build_path` doesn't blend shoulders — fine on flattened ground.

### Phase 4 — test harness
- New `#[tokio::test]` `hierarchical_roads` modeled on the existing live-server setup (`placement/test.rs` / `parcels/test.rs`): world → parcels → super-parcels → walls + gates (populates `gate_locations`) → `flatten_urban_area` → `build_road_network` → `build_path` per `Path` → `flush_buffer`. Inspect via the visualizer snapshot.

---

## Knobs

| Lever | Default | Notes |
|---|---|---|
| `feather` | 12–20 | Width of the flatten→natural transition band. |
| `smooth_iters` | 8–16 | Higher → flatter city, more earthworks/HTTP volume. |
| `skip_water` | true | v1 doesn't drain lakes. |
| A\* `step` | 4 | Sparse for v1; `1` for the future local tier. |
| Tier 1 width / priority | 3 / High | Arterial backbone. |
| Tier 2 width / priority | 2 / Medium | Gate spurs. |
| Town-center node | toggle | Radial hub vs pure parcel-to-parcel MST. |

---

## Open / next

- **Recursive A\* subdivision** into collectors/lanes (tier 3) — the real density hierarchy. Needs `step=1` + the A\* state refactor + claim-aware costs (dodge buildings, snap endpoints onto existing road cells).
- **Blocks = connected components of land minus road cells** → feed existing `detect_frontages` + frontage walker unchanged.
- **Shoulder blending** — `build_path` doesn't grade terrain beside the road meeting road height; add a `blend_terrain`-style taper when we route on un-flattened ground.
- **Sparse-outside tier** — inter-settlement / approach roads (`step=4`, no claim awareness) once there's anything outside the wall to connect to.
- **Water handling in flatten** — `skip_water=true` leaves lakes; decide whether to bridge/fill small water in the urban core.

---

## Progress — full pipeline slice (2026-06-06)

Everything below is wired into the live-server test `hierarchical_roads` in
`src/generator/parcels/test.rs`
(`cargo test --bin Tome hierarchical_roads -- --nocapture --test-threads=1`).

### Working end-to-end
1. `generate_parcels` → **EVAL AID** (test-only) forces a contiguous ~4-parcel
   urban core, because the live build area varies per run and the classifier
   often collapses to 1 parcel (too degenerate to evaluate).
2. Wall + gates → feathered `flatten_urban_area`.
3. **Tiered A\* roads** (`build_road_network`): arterials (MST over backbone) +
   collectors (gate → nearest network).
4. **`find_blocks`** — flood fill (4-connected) of urban − paved-roads − wall.
5. **`subdivide_block`** (max_dim 24) per block → lots + alley (tier-3) cells.
6. **Build ALL roads together** at the end (mains + a synthesised width-1 alley
   `Path`) via one `build_paths_merged` pass → connect + meld.
7. **Hierarchical placement**: per lot, shared `Plot` walked
   arterial → collector → subdivider; gold gets first claim, later tiers can't
   overlap. Houses on roads, cottages on lanes; per-house palette/roof rolls.

Last run: 5 parcels, 15 gates, 20 roads, 14 blocks, 169 lots,
**317 buildings**, ~70s.

### Code added/changed
- **Dedicated grid A\*** — `route_path_with` rewritten to `(cell, y, dir)` state +
  `came_from` (was path-as-state → blew up on long flat runs). Generic `a_star`
  untouched.
- `routing.rs`: `RouteParams` knobs (`road_cost`, `diagonal_cost`,
  `wall_clearance`, `wall_weight`, …) + `RouteContext { region, road_cells,
  road_height, goal_cells, wall_dist }`. Cost adds: on-road discount + y-snap
  (merge), diagonal surcharge (straightness), wall-clearance penalty; `is_end`
  multi-goal (collectors stop on first network contact).
- `network.rs`: sequential cost-coupling in `build_road_network` (record each
  path → later routes merge, Phase A); `find_blocks`; `wall_distance` BFS.
- `building.rs`: `build_paths_merged` (many paths → one melded surface; lower
  height + higher-priority material win) + shared `smooth_road_heights` (fixes
  the empty-`mean()`→0.0 collapse that killed width-3 roads).
- `city_houses/frontage.rs`: `frontage_from_roads(block, road)` — tier-aware
  frontage from an explicit road-cell band (width-agnostic).

### RESOLVED (2026-06-07): gold/red frontage barely used
Per-tier diagnostic was: `arterial 760 cells / 8 placed / 0 failed · collector
1735 / 10 / 0 · subdivider 5767 / 299 / 0`. **0 failures ⇒ not being attempted.**
Subdivision fragmented the main-road-facing edge: most lots were interior
(touch only alleys); the few touching an arterial/collector did so along a short
edge, so those frontage chains fell below the min house front-width and were
skipped.

**Fix shipped:** `subdivide::reserve_road_ribbon(block, main_roads, depth)` peels
a frontage ribbon (`RIBBON_DEPTH = 10`, fits the deepest House) off each block
*before* subdividing — multi-source BFS inward from cells fronting a main road,
returning the ribbon's connected components as lots + the leftover interior.
The `hierarchical_roads` test now reserves ribbons, then subdivides only the
interior. Ribbon lots keep the full-length arterial/collector edge intact, so
the densest-tier-first placement loop fills them first. Unit-tested in
`subdivide::tests` (depth band, no-road no-op, L-shaped corner = one component).
**Live re-run pending** to confirm the per-tier placement counts shift onto
arterial/collector.

### Diagonal frontage + verge paving (2026-06-07)
After the ribbon fix, arterial/collector placement was still starved: not by short
chains but by **rect-unfit** skips (arterial diag was `15 short / 312 unfit`). Root
cause: a diagonal (45°) road's frontage staircases, and an axis-aligned house rect
can't tile a staircased band. Fixes shipped:

- **`split_into_chains` → 8-connectivity** (`city_houses/frontage.rs`): a staircased
  road edge is now a single ordered chain, not 1-cell stubs. Killed the short-chain
  skips.
- **`rect_from_frontage` → anchor at the block-interior extreme** (`city_houses/walk.rs`):
  the house front sits on the deepest cell of its slice, so the footprint never
  pokes onto the road. Backward-compatible for straight chains (min == max); all
  existing unit tests pass + new staircase test.
- **Depth-shrink-to-fit + `RIBBON_DEPTH` 10→14** (test): on a steep slice the rect
  overruns the band by the staircase *rise*, so shrink depth (floor 5) until it fits
  and deepen the ribbon to absorb the rise.
- **Straighter mains** (`paths/network.rs`): arterial `turn_weight 3→6, diagonal_cost
  2→5`; collector `1→3, 2→4`. In a grid a 45° line is a many-turn staircase, so both
  penalties favor an axis-aligned L (one corner) → long tidy frontage. Cut arterial
  unfit 207→98, placements 38→51.
- **Verge paving** (test): the unavoidable triangular set-back on a diagonal is paved
  into a forecourt of the road's own material (arterial stone-brick, collector
  cobblestone) — walk from each frontage cell into the block until it meets the house.

Last run: 8 blocks, 85 lots, **246 buildings** (arterial 51 / collector 68 /
subdivider 127), 1006 verge cells paved.

**Residual:** axis-aligned houses on a true 45° street are inherently set back by
~front-width; the verge paving hides it. A no-verge fix needs sheared/stepped
footprints, which the rect-based house generator (frame/walls/roof) can't take yet.

### Other open / cleanup
- Interior fill disabled (frontage-only; lot backs empty).
- Radius-1 robustness (deferred): guarantee junction connectivity + deterministic
  arterial merges (currently mod-4-alignment dependent).
- Strip debug scaffolding before graduating: per-run prints, gold/redstone
  markers, the `EVAL AID` forced-parcels block.
- Graduate the flow out of the test into the real settlement pipeline.
- Knobs: `subdivide_block` max_dim, tier size pools, `SIDE_BUFFER_CELLS`,
  `HEURISTIC_WEIGHT` (×10, still greedy — could lower now the search is bounded).
