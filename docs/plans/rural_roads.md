# Rural Road Network

Plan drafted 2026-06-17. Branch: `td/rural_roads`. **Status: implemented** (see
"As built" at the end).

## Goal

Connect every **rural resource building** to the urban road network, so the
countryside reads as one settlement instead of buildings scattered in empty
land. Concretely:

- Each rural resource building is joined to a **town gate** by a routed road.
- The road attaches at the building's **door** when it has one, otherwise at the
  nearest footprint-perimeter cell ("just connect anywhere").
- Where a production area will paint a `rural_road` **border ring**, the network
  **routes to / reuses that ring** instead of laying a redundant parallel path.
- Rural and urban meet **at the gates** — the urban collectors already route each
  gate inward, so the gate is the shared node; no change to the urban network.

## Settled decisions

These came out of the design discussion and are not open:

1. **Junction = gates only.** Rural roads terminate at gate cells (stepped to the
   rural/outward side). The existing urban network connects gates → town interior,
   so the two systems join at the threshold with no reorder of the urban build.
2. **Door attach = runtime door scan, perimeter fallback.** Rural buildings are
   NBT structures with no door metadata (`Structure` has no door field). After
   placement, scan the footprint's just-placed blocks for a door block and use
   that cell; fall back to the nearest perimeter cell when none exists. No data to
   maintain, can't drift from the NBT, and doorless buildings (mines, open
   pastures/apiaries) get the fallback for free.
3. **Ordering = three phases via ring prediction** (see below). The production
   border ring is deterministic from district geometry, so we predict it and run
   the road network *before* the painters.
4. **Border use = route to / reuse the predicted ring.** The ring becomes a
   first-class part of the network (a goal/merge set), not decoration.

## Ordering — three phases

The current `generate_town` loop (`settlement.rs:81-107`) interleaves *place rural
building* with *paint production area* per district. We split it:

```
Phase R1  Place ALL rural buildings        (placement only — no painting)
Phase R2  Build the rural road network      (predicting border rings)
Phase R3  Paint ALL rural production areas  (painters; border ring overlays the routed road)
```

The painter (`paint_production_area`) filters border cells by `!is_claimed`, so a
ring cell the road already claimed as `Path` is simply skipped by the painter —
the two passes compose without double-paving. This is why R2-before-R3 is safe.

### Why prediction works

`paint_production_area` (`production_area.rs:53-90`) derives the border ring purely
from `district.data.edges`: cells within `EDGE_BUFFER` (= 3) Chebyshev of any edge
cell, that are interior, unclaimed, and not water. None of that depends on the
painter running. Phase R2 replicates that computation to get the ring cells ahead
of time. Only painters that actually use a border palette get a ring (see "Which
areas have a ring").

## Reuse — what already exists

We are **not** writing a new router or a new road realizer. The rural network is a
re-wiring of the urban primitives over a different node/region set.

| Need | Reuse | Notes |
|---|---|---|
| Terrain-aware A* routing | `routing::get_path_with` + `RouteContext` / `RouteParams` | Already step-, turn-, grade-, water-, and blocked-aware. Region is just a different cell set. |
| Road realization (slab smoothing, widening, melding) | `building::build_paths_merged` (and `build_path`) | Carries height as a float, lays half-step slabs, melds overlapping paths. Handles un-flattened grades. |
| Door → nearest road connector | `connect::connect_doors_to_roads` | A*'s a 1-wide path from a door to the nearest road cell, around `blocked`. Needs its `MAX_CONNECTOR` (40) parameterized for longer rural runs. |
| Path tier / width / material | `path::{Path, PathPriority, PathType}` | Reuse as-is. |
| Network graph (MST + loop shortcuts) | `network::{mst_edges, shortcut_edges, edge_betweenness}` | Currently **private** in `network.rs`. Lift to `pub(crate)` (or a small `paths::graph` submodule) so the rural builder can reuse them. |
| Cost-coupled sequential routing (later edges merge onto earlier ones) | the route loop in `build_road_network` (`network.rs:176-209`) | The pattern — record each routed path into `road_cells`/`road_height`, give later routes an on-road discount + y-snap — is exactly what keeps rural roads from running parallel. Factor the loop body so both networks share it, or mirror it. |

`build_road_network` itself stays **urban-specific** (its nodes are district
centres + gates + industry, its region is the urban set). We add a sibling rather
than overloading it.

## New code

### `paths/rural.rs` — `build_rural_road_network`

```rust
pub async fn build_rural_road_network(
    editor: &Editor,
    assignments: &SettlementProductionResult,   // which district got which building + painter
    data: &LoadedData,
    material: MaterialId,                        // rural road surface material (knob)
    route_step: i32,
) -> Vec<Path>
```

Steps:

1. **Anchors (sources).** For each placed rural structure, find its attach cell:
   scan the footprint cells (from the `BuildClaim::Structure` claim map, as
   `settlement.rs:146-150` already does for industry) for a door block via
   `editor.get_block`; fall back to the nearest perimeter cell. One anchor per
   building.
2. **Gate nodes (destinations).** Each `world.gate_locations` entry, stepped a few
   cells **outward** along its `Cardinal` (the rural side), lifted to surface y via
   `world.add_height`. (Confirm outward sign against `gate.rs` — urban steps inward,
   rural is the opposite side.)
3. **Predicted rings.** For each district whose painter uses a border palette
   ("relevant"), compute the `EDGE_BUFFER` ring cells exactly as
   `paint_production_area` does. Store per-district ring sets; their union is a
   `goal/merge` set the trunk can terminate on.
4. **Graph.** Build an MST (reused `mst_edges`) over `{gates} ∪ {ring anchors / building anchors}`,
   so multiple rural buildings share a spine before reaching a gate rather than each
   shooting an independent path. Optionally add capped `shortcut_edges`.
5. **Route, cost-coupled.** Route each edge with `get_path_with` over the **rural
   region** (see below), recording each routed path into `road_cells`/`road_height`
   so later edges merge onto the spine and snap height — same loop as the urban
   builder. Ring cells and the running network are `goal_cells` so a trunk stops on
   first touch.
6. **Door connectors.** For each building anchor not already on/beside the routed
   network or its predicted ring, run `connect_doors_to_roads` (with a raised
   connector cap) to A* a 1-wide spur to the nearest network/ring cell.
7. Return `Vec<Path>`; the caller realizes them with `build_paths_merged` and
   claims paved cells as `BuildClaim::Path` (so Phase R3's painter skips them).

### Rural region & blocked set

- **Region** (allowed A* cells): the union of all **rural** district `points_2d`
  plus a small approach corridor around each gate, minus water. Confining to rural
  land keeps routes out of the urban interior (which the urban network owns) and
  off the wall except at gates. Alternative knob: `region = None` (unconstrained
  over the build area) with `blocked` doing all the work — simpler, but routes may
  wander into urban land. Default to the rural-union region.
- **Blocked**: building footprints (+ small margin, like `IND_MARGIN`), wall cells,
  and existing claims — same construction as `settlement.rs:151-158`.

### Wiring into `generate_town`

Split the existing rural loop. Today (`settlement.rs:78-107`) it does, per district:
`place_rural_building` → `paint_production_area`. Becomes:

```rust
// R1: place only
for sd_id in &sd_ids { place_rural_building(...).await; record placed + assignment }

// R2: rural roads (predicting rings), then realize + claim
let paths = build_rural_road_network(&*editor, &result, &data, rural_material, 1).await;
let slabs = build_paths_merged(&*editor, &data, &paths, &mut rng).await;
claim every paved cell as BuildClaim::Path(PathType::Road);

// R3: paint production areas (border ring overlays the routed road)
for sd_id in &placed { paint_production_area(...).await; }
```

Keep the `resolve_for_parcels` resource resolution where it is (it already runs
before placement). The per-building "resource for the placed building" lookup
(`settlement.rs:96-101`) moves into R3 with the rest of the painting.

## Knobs

| Lever | Default | Notes |
|---|---|---|
| Rural road `MaterialId` | gravel / dirt-path (TBD) | See open question on matching the `rural_road` paint look. |
| Rural road tier / width | `PathPriority::Low` (w1) or `Medium` (w2) | Narrower than urban arterials; lanes, not avenues. |
| A* `route_step` | 1 | Per-cell over rough terrain; 4 (sparse) if perf bites on long inter-area legs. |
| `EDGE_BUFFER` (ring width) | 3 | Must match `production_area.rs` — read the const, don't re-hardcode. |
| Connector cap | raise from 40 | Rural door→spine spurs are longer than urban ones. |
| Gate outward step | 2–4 cells | Far enough off the wall to start on clear rural ground. |
| Region mode | rural-union | vs unconstrained-`None` (see above). |

## Which areas have a ring

From `production_area.rs`, a border ring exists only for painters that pass a
border palette:

- **Has ring:** `Palettes { border_palette: Some(..) }`; `pasture` and `sugarcane`
  function painters (default `rural_road`).
- **No ring:** `logging`, `bee_area`, `mine` function painters, and
  `Palettes { border_palette: None }`.

Phase R2 resolves each district's assigned painter from the registry and includes
its ring only when the painter is one of the ring-bearing kinds. Doorless / ringless
buildings just get a door connector straight to the gate spine.

## Resolved decisions (post-discussion)

- **Surface material = option (a).** A new `rural_road` material
  (`data/materials/ground/rural_road.json`) — packed mud / coarse dirt / dirt /
  rooted dirt to mirror the `rural_road` paint palette, with `mud_brick` slab +
  stairs for the half-step grade lips `build_path` needs.
- **Corridor flattening = yes.** After routing, the corridor is `force_height`-ed
  to the routed road heights (skipping building/wall cells) before
  `build_paths_merged`, so the road sits on graded ground — mirroring the urban
  realization. (Slab smoothing then handles the residual.)

## Open questions / risks (remaining)

- **Gate-node side detection** uses `is_urban(gate + dir)` to pick the rural side,
  stepping one cell out so the width-2 road's paved band still reaches the gate
  tile. Verify in-world that rural and urban roads actually meet flush at gates.
- **`region: None` perf.** Rural routing is unconstrained (only `blocked` =
  urban ∪ footprints) at `route_step = 1`. Long building→gate legs over open
  countryside could get expensive; bump `route_step` to 4 if it bites.
- **No reachable gate / water crossings.** A building with no routable path logs +
  skips (partial connectivity OK). `force_height(.., skip_water=false)` will fill a
  small stream the road crosses into a dirt causeway — acceptable for v1.

## As built

- `data/materials/ground/rural_road.json` — new road material (option a).
- `ProductionPainter::paints_border()` — which painters lay a `rural_road` ring.
- `resource_chain::paint_production_area_for(.., structure_id, ..)` — explicit-id
  variant so painting can run in a pass separate from placement without
  mis-attributing the area to `structures.last()`. `EDGE_BUFFER` made `pub`.
- `placement::try_place_rural` now returns `Option<PlacedRural>` (places only, no
  inline paint); `PlacedRural` carries district, structure id, painter, resolved
  resource, and the `has_border_ring` flag.
- `paths::rural::build_rural_road_network` + `RuralBuilding` — the network: nearest
  gate per building, door-scan/perimeter anchor, predicted-ring discount field,
  cost-coupled routes (reuses `get_path_with`/`RouteContext`/`RouteParams`).
  `network::mst_edges` was made `pub(crate)` for potential reuse (the v1 network
  uses a nearest-gate star with cost-coupling rather than an MST, so it isn't
  called yet — kept exposed for a future multi-destination rural trunk).
- `settlement::generate_town` — rural flow is now three phases: place all rural
  buildings → `build_rural_road_network` + realize + claim `Path` → paint all
  production areas via `paint_production_area_for`.
- **Door scan / live-server cache fix.** The door scan reads back blocks placed
  this run. `get_block`/`try_get_block` subtract `build_area.origin` (they expect
  absolute coords), but the pipeline uses local coords and the cache is local-keyed,
  so on a live server (nonzero origin) those reads fell through to world terrain and
  the door was never seen → every building used the perimeter fallback (path landed
  at a corner). Added `Editor::get_cached_block(local)` — a cache read by the local
  key, no origin adjustment — and the door scan uses it. (Invisible offline because
  synthetic worlds have origin 0.) The broader `get_block` asymmetry is left alone
  for now; worth fixing centrally as a follow-up.
- **Tests:** no live-server harness added yet (see "Test harness" above). `cargo
  check` and `build_furnished_houses_offline` pass; the live settlement path needs
  an in-world run to validate routing/flattening and the door fix visually.

## Test harness

Mirror the existing live-server settlement test (`placement/test.rs`): world →
parcels → wall + gates → place rural buildings → `build_rural_road_network` →
`build_paths_merged` → paint production areas → `flush_buffer`, then inspect via the
SVG/visualizer snapshot. Assert: every placed rural building has at least one paved
cell within N cells of its anchor, and every routed rural path terminates on a gate
node or the running network. An offline variant can assert anchor/ring computation
(door scan, predicted ring == painter ring) without a server.
