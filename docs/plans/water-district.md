# Water Districts & Scattered Ships — Implementation Plan

## Goal

Use the procedural ship generator (`src/generator/ships/`) in the settlement
pipeline by placing **free-floating ships on large bodies of water**.

Rather than inventing a bespoke water-body detector, **reuse the districting
system**: add a new `ParcelType::Water` so water-heavy districts split off from
the existing classification, then run a ship-placement pass over those districts.

Scope for this first pass:

- **Scatter only** — ships are placed on any sufficiently large water district
  anywhere in the build area. No harbour clustering at the town waterfront yet.
- **Free-floating only** — no piers, gangplanks, or shore connections yet.

Both are deliberate first increments; see **Deferred** below.

---

## Why districts (not a custom flood-fill)

Water-heavy districts already exist — the classifier currently dumps them into
`Rural` via the `water > URBAN_WATER_LIMIT → Rural` branch in
`classify_districts` (`src/generator/districts/classification.rs`). They are
already:

- Voronoi-partitioned and **merged** (`merge_down`) so adjacent water cells
  coalesce into larger regions,
- **analyzed** (`ParcelAnalysis::water_percentage`),
- stored on `world.districts` with their cell sets (`district.data.points_2d`)
  and adjacency.

So body detection, sizing, and grouping all come for free from machinery that's
already trusted and tested. A side benefit: reclassifying lakes/oceans out of
`Rural` stops phantom rural farms from being assigned to water parcels (the
rural economy filters on `== ParcelType::Rural`).

---

## Part 1 — New district type

### `districts/parcel.rs`

Add a variant to the enum:

```rust
pub enum ParcelType {
    Unknown,
    Urban,
    Rural,
    Water,      // new
    OffLimits,
}
```

### `districts/constants.rs`

```rust
/// Minimum merged water fraction for a district to classify as `Water`.
/// Well above `URBAN_WATER_LIMIT` (0.33) so only genuinely watery districts
/// split off — marshy / riverside districts still fall to Rural.
pub const WATER_DISTRICT_LIMIT: f32 = 0.6;
```

### `districts/classification.rs`

- **`classify_districts`** — before the existing
  `water > URBAN_WATER_LIMIT → Rural` branch, add:
  `water > WATER_DISTRICT_LIMIT → ParcelType::Water`.
- **`classify_parcels`** — mirror the same gate at the per-parcel level so
  parcel subtypes feed `district_score` consistently.
- **`district_score`** — add an explicit `ParcelType::Water => count * 2.0`
  arm. (Currently caught by `_ => 2.0`, so behaviour is unchanged; making it
  explicit keeps water from ever pulling a district urban.)

### `districts/footprint.rs`

`reconcile_districts_to_footprint` and the urban-footprint vote must treat
`Water` as **non-urban** (same side as Rural/OffLimits) so a lake is never
absorbed inside the wall. The buildable check at `footprint.rs:36` already
rejects `is_water` cells, so no change is needed there.

### Blast radius

Most usages are `== ParcelType::Urban` / `== ParcelType::Rural` comparisons
that need no change. The exhaustive `match` sites the compiler will flag are:

- `footprint.rs` (the urban/rural count + vote matches) — add a `Water` arm
  (non-urban).
- Test-only color maps: `districts/test.rs`, `placement/test.rs`,
  `resource_chain/tests.rs` — add a `Water` arm.

The rural economy (`resolve_rural_production`, `settlement.rs:43`) filters on
`== ParcelType::Rural`, so water districts are **automatically excluded** from
farm placement once reclassified.

**Optional:** give `Water` a color in the visualizer snapshot for debugging.

---

## Part 2 — Ship placement pass

New module `src/generator/ships/fleet.rs`:

```rust
pub async fn scatter_ships(editor: &mut Editor, data: &LoadedData, seed: Seed) -> usize
```

Returns the number of ships placed. Builds its own `RNG::new(seed).derive()`
chain so it's deterministic and independent of the town RNG stream.

### Step 1 — Bodies = Water districts

```rust
world.districts.values().filter(|d| d.data.parcel_type == ParcelType::Water)
```

Each district's `data.points_2d` is the candidate region; its area drives the
fleet count: `n = clamp(area / AREA_PER_SHIP, 1, MAX_SHIPS_PER_BODY)`.

### Step 2 — Shore-distance field

Compute a distance-to-shore transform **once** over the whole build area's
`is_water` cells (multi-source BFS seeded from every land / out-of-bounds
neighbour). Computed globally (not per-district) so a footprint never pokes over
land even where two adjacent water districts meet.

### Step 3 — Fit solver (per ship, rejection-sample up to `ATTEMPTS`)

1. **Center** — pick a water cell in the district with
   `dist_to_shore >= required_half_beam + SHORE_MARGIN`.
2. **Heading** — cardinal only; prefer the axis with the most open-water extent
   at the center.
3. **Length** — the largest tier length (14/20/26/32/38/44) whose full footprint
   rect (`length` along heading × `max_beam = length / beam_ratio` across) is
   **all water** with `depth = surface − seabed >= MIN_DEPTH`, clamped by
   vertical clearance under the build ceiling so masts fit.
4. **Overlap** — reject if the rect (+ margin) hits a pass-global placed-set.
5. **Anchor** — `build_ship`'s anchor is the *stern* keel point and the hull
   extends `+length` along the heading, so
   `anchor = center − heading_unit · (length / 2)`
   (mirrors the `anchor_z = center_z + length/2` trick in `build_ship_live`).

If nothing fits after `ATTEMPTS`, place fewer ships on that body.

### Step 4 — Build + claim

- `ShipSpec::new(heading, length)` with per-ship rolled `hull_shape`,
  `sail_state` (mostly `Full`, occasional `Furled`), and `wind`, then
  `build_ship(&mut ctx, &spec, anchor).await`.
- Claim the footprint cells as a new `BuildClaim::Ship` variant
  (`src/generator/build_claim.rs`) for forward-protection against the future
  harbour / road / bridge passes. (Inter-ship overlap is already handled by the
  pass-global placed-set, so the claim is not load-bearing for correctness.)

### Tuning constants (`ships/tuning.rs`)

`AREA_PER_SHIP`, `MAX_SHIPS_PER_BODY`, `MIN_DEPTH`, `SHORE_MARGIN`,
`FURLED_CHANCE`, `ATTEMPTS` — added to the existing central tuning surface.

---

## Part 3 — Integration

One call at the end of `generate_town` (`src/generator/settlement.rs`), before
the final `editor.flush_buffer().await`:

```rust
let ships = crate::generator::ships::fleet::scatter_ships(editor, &data, seed).await;
println!("Placed {ships} ships across water districts");
```

---

## Testing

- **Offline (classification):** synthetic mixed world (flat land + a water rect
  large enough to form a district). Assert the classifier produces ≥1 `Water`
  district; extend the existing district-classification color-map tests with a
  `Water` arm and assert lakes paint as Water.
- **Offline (placement):** on the same world, assert ships are placed; every
  footprint is fully water-contained and non-overlapping; a tiny pond (no Water
  district) yields no ships.
- **Property sweep:** lake sizes × seeds → invariant that no ship ever extends
  over land or another ship.
- **Live:** `scatter_ships_live` analogous to `build_ship_live` for a
  screenshot pass.

---

## Known gaps / decisions

- **Frozen oceans** surface as ice, so `is_water` is false there → no Water
  district → no ships. Acceptable for v1.
- **Float only** — ships require `MIN_DEPTH`; no beached/grounded ships.
- **Size is water-extent-driven**, not tied to settlement wealth/size.
- `WATER_DISTRICT_LIMIT` starts at `0.6`; tune against a real seed.

---

## Deferred

- Harbour clustering on the town waterfront (the `Water` district adjacent to
  the urban core becomes a harbour).
- Piers / gangplanks / shore connections.
- Culture-tied ship palettes (currently `ship_oak`).
- Small boats on narrow rivers (rejected here by the beam/depth fit).

---

## Build order

1. **Part 1** (district type) — land it first and eyeball the classification on
   a real seed via the visualizer before building on top.
2. **Part 2** (ship pass) + **Part 3** (integration).
3. Tests throughout.
