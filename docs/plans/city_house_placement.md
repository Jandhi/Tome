# City House Placement

**Status:** Phase 1 (Skeleton) implemented in `src/generator/city_houses/`. Phases 2–3 not yet started.

Plan for placing procedural buildings_v2 houses inside city blocks. Goal: houses front the street with doors facing the road, with the interior of each block filled by random shapes/sizes. Replaces the current `settlement_with_buildings_v2` flow, which uses `fill_plot_multi` (greedy random fill, no street awareness, doors face arbitrary walls).

This is **separate from** `placement.md`, which covers single-NBT resource building placement.

---

## Approach

Two-pass per city block:

1. **Frontage pass** — for each city block, identify chains of cells adjacent to `BuildClaim::Path`. Walk each chain in strides and place rectangular houses with their short edge flush against the road.
2. **Interior pass** — run the existing `fill_plot_multi` on whatever `plot.usable` cells remain. Random shapes and sizes for the back of the block.

The pass order matters: front-row claims the prime road frontage first; the interior fill respects those claims naturally because they get marked `plot.usable = false`.

---

## Pipeline order

```
districts → walls → paths → resource buildings (placement.rs) → city houses (this plan)
```

`paths` **must** precede city houses so `BuildClaim::Path` cells exist for frontage detection. If we ever want to run houses before paths, the frontage detection has to fall back to "any outer perimeter of the block" (see *Open questions*).

---

## buildings_v2 changes

Small. The pipeline (foundation, frame, walls, floors, roof, rooms, furnish) is already orientation-agnostic — it consumes a `Footprint` + `plot_bounds`. We just need to feed it a `Footprint` we constructed ourselves.

### `Footprint::from_rect(rect: Rect2D) -> Self`

Direct constructor: vertices CW-ordered around the rect, `rects = vec![rect]`. Skips `generate_layouts → select_layout → merge_layout` entirely. The frontage walker uses this for road-facing rectangles.

Optional companion `Footprint::from_rects(rects: Vec<Rect2D>)` to support wings later (phase 2). The existing `merge::merge_layout` already does the multi-rect outline math — we just call it directly.

### `SizeClass::front_width_range`, `SizeClass::depth_range`

For the walker to pick stride sizes and rect dimensions. Suggested defaults (units = cells):

| SizeClass | front_width | depth     | notes                       |
| --------- | ----------- | --------- | --------------------------- |
| Cottage   | 5..=6       | 5..=7     | townhouse-ish               |
| House     | 6..=8       | 7..=10    | depth > width slightly      |
| Hall      | 8..=10      | 9..=12    | rarely on frontage          |
| Manor     | 9..=12      | 11..=15   | almost never on frontage    |

Biasing depth ≥ width gives the medieval townhouse silhouette.

### Door direction — no API change for v1

`place_doors` already scores wall segments by `distance_to_plot_edge(mid, plot_bounds)`. We pass a synthetic `plot_bounds` (a thin strip on the road outside the house). The road-facing wall scores ~0; other walls score ≥ depth. Wins automatically.

The `plot_bounds` hack is opaque, so **phase 2** adds:

```rust
pub fn place_doors(
    wall_segs: &mut WallSegments,
    plot_bounds: &Rect2D,
    footprint_area: i32,
    boundary_cells: &HashSet<Point2D>,
    preferred_facing: Option<Cardinal>,   // new
    rng: &mut RNG,
)
```

When `Some(facing)`, segments matching that cardinal get a large score bonus. Defer to phase 2 — v1 ships with the synthetic-plot-bounds approach.

---

## New module: `generator/city_houses/`

```
src/generator/city_houses/
├── mod.rs        — pub entry: place_city_houses(blocks, ctx, ...)
├── frontage.rs   — frontage detection + chain ordering
├── walk.rs       — walk-and-place along a chain
└── test.rs
```

### Public entry

```rust
pub async fn place_city_houses(
    blocks: &[HashSet<Point2D>],
    editor: &mut Editor,
    data: &LoadedData,
    rng: &mut RNG,
) -> Vec<HouseOutput>
```

Per block:
1. Build `Plot` from block cells (`usable[x][z] = block.contains(point) && claim == BuildClaim::None`).
2. `frontages = detect_frontages(block, editor)`.
3. For each frontage: `walk_and_place(frontage, &mut plot, ...)`.
4. `fill_plot_multi(&mut plot, &[Hall, Manor, House, Cottage], ...)` for the interior.

---

## Algorithm details

### Frontage detection (`frontage.rs`)

```rust
pub struct Frontage {
    pub cells: Vec<Point2D>,    // ordered along the street
    pub outward: Cardinal,      // direction toward the road
}

pub fn detect_frontages(
    block: &HashSet<Point2D>,
    editor: &Editor,
) -> Vec<Frontage>
```

Algorithm:
1. For each `cell ∈ block`, for each `d ∈ CARDINALS_2D`:
   - `neighbor = cell + d`
   - If `neighbor ∉ block` AND `editor.world().claim_at(neighbor) == BuildClaim::Path(_)`: record `(cell, d)`.
2. Group records by `d`.
3. Within each group, sort by the perpendicular axis (along the street).
4. Split each sorted group into contiguous runs — those are the chains.

**Fallback** when a block has no path-adjacent cells (interior block): treat the outer perimeter of `block` itself as frontage. Outward direction = the cardinal whose neighbor is outside `block`.

### Walk-and-place (`walk.rs`)

```rust
pub async fn walk_and_place(
    frontage: &Frontage,
    plot: &mut Plot,
    ctx: &mut BuildCtx<'_>,
    ...
) -> Vec<HouseOutput>
```

State machine along `frontage.cells`:

```
cursor = rng.rand_i32_range(0, stride.start)   // random starting offset
while cursor + min_front_width ≤ chain.len():
    size_class    = roll_size_class(...)
    front_width   = rng.rand in size_class.front_width_range
    depth         = rng.rand in size_class.depth_range

    rect = rect_from_frontage(chain, cursor, front_width, depth, outward)

    if not rect.cells().all(|c| plot.is_usable(c)):
        cursor += 1                            // try the next cell
        continue

    footprint     = Footprint::from_rect(rect)
    plot_bounds   = synthetic_plot_bounds(chain[cursor..cursor + front_width], outward)
    bctx          = BuildingContext::new(culture, size_class, roof_style)
    house         = build_house(ctx, footprint, &bctx, plot_bounds).await?

    mark_used(plot, rect, SIDE_BUFFER_CELLS)
    cursor += front_width + SIDE_BUFFER_CELLS
```

`rect_from_frontage`: anchor a rect of `(front_width × depth)` cells with one short edge flush along `chain[cursor..cursor + front_width]`, extending in `-outward` direction (away from the road, into the block).

`synthetic_plot_bounds`: a 1-cell-thick strip placed *outside* the block, on the road side of the house — i.e. `Rect2D` covering `chain[cursor..cursor + front_width]` offset by `outward * 1`. The road-facing wall segment is closest to this strip; `place_doors` picks it.

### Interior fill

Unchanged. After the frontage pass:

```rust
let interior_class_pool = [SizeClass::Hall, SizeClass::Manor, SizeClass::House, SizeClass::Cottage];
fill_plot_multi(rng, &mut plot, &interior_class_pool, max_interior_buildings);
```

The remaining `plot.usable` cells naturally exclude the front-row claims.

---

## Synthetic plot_bounds — worked example

Block at origin, frontage cells `[(5, 0), (6, 0), (7, 0)]` with `outward = North` (north is `-z`, so `outward = Point2D::new(0, -1)`):

- Front-row house rect: `Rect2D { origin: (5, 0), size: (3, 6) }` — extends south into the block.
- Synthetic plot_bounds: `Rect2D { origin: (5, -1), size: (3, 1) }` — a 3×1 strip directly north of the house.
- The house's four wall segments at floor 0:
  - North wall midpoint ≈ `(6.5, 0.5)`, distance to plot_bounds ≈ 0
  - South wall midpoint ≈ `(6.5, 5.5)`, distance to plot_bounds ≈ 6
  - East/West walls midpoint ≈ `(7.5, 2.5)` / `(4.5, 2.5)`, distance to plot_bounds ≈ 2.5
- `place_doors` picks the north wall — i.e. the road-facing wall. ✓

---

## Constants

| Constant                | Default | Notes                                                         |
| ----------------------- | ------- | ------------------------------------------------------------- |
| `SIDE_BUFFER_CELLS`     | 1       | Side gap between adjacent front-row houses                    |
| `MIN_FRONTAGE_LENGTH`   | 6       | Skip chains too short to fit one Cottage                      |
| `BACK_BUFFER_CELLS`     | 1       | Gap behind front-row before interior fill can start           |
| `MAX_DEPTH_INTO_BLOCK`  | 15      | Cap on how deep front-row houses extend                       |
| `FRONTAGE_SIZE_POOL`    | `[Cottage, House]` | Size classes eligible for the frontage pass        |
| `INTERIOR_SIZE_POOL`    | `[Hall, Manor, House, Cottage]` | Size classes for the interior pass    |

---

## Open questions

1. **Pipeline order.** Do `paths` run before settlement today, or only in some tests? If only in tests, the production pipeline needs the order locked in. Check `src/main.rs` and the settlement entry point.
2. **No-road blocks.** Interior blocks with no `BuildClaim::Path` adjacency. Plan: fall back to using the block's outer perimeter as frontage. Open: should those houses' doors face outward to nothing, or face inward to a future courtyard?
3. **Corner stations.** When two chains meet at a block corner (e.g. a north-frontage chain ends, an east-frontage chain begins), the corner cell may belong to both. Plan: assign to the longer chain. Revisit if visual artifacts appear.
4. **Resource buildings already placed.** They claim cells via `BuildClaim::Structure` before this runs. Inherits naturally — those cells aren't in `plot.usable`. But: should the walker prefer chains *without* nearby structures, so houses cluster apart from workshops? Not for v1.
5. **Hierarchy.** Defer to phase 2. Bias size by distance-to-block-centroid or super-district wealth.
6. **Roof style per block.** Uniform per block (one `RoofStyle` picked per block) vs per-building. Recommend uniform per block — gives streets a coherent look.

---

## Phases

### Phase 1 — Skeleton (the minimum that places houses fronting roads)
- `Footprint::from_rect`
- `SizeClass::front_width_range`, `SizeClass::depth_range`
- `generator/city_houses/` module with `detect_frontages` + `walk_and_place`
- Synthetic `plot_bounds` for door direction
- Interior fall-through via existing `fill_plot_multi`
- New integration test (or extend `settlement_with_buildings_v2`)
- **Acceptance:** every front-row house has its door on the road-facing wall; no overlaps; no claim conflicts.

### Phase 2 — Polish
- `place_doors` `preferred_facing: Option<Cardinal>` (retire synthetic plot_bounds hack)
- Size-class hierarchy: distance-to-centroid or super-district wealth → larger near center
- Corner handling for chains meeting at block corners
- Optional wings extending into the block (`Footprint::from_rects`)

### Phase 3 — Aesthetics
- Uniform roof style per block (one pick per block)
- Multi-story bias on frontage (taller front row)
- Storefront variants (wider door, sign block) on frontage facing main roads

---

## Tests

Unit (no server):
- `frontage_detection_picks_path_adjacent_cells` — synthetic block + claim map, assert chains and outward directions.
- `frontage_chains_split_at_gaps` — non-contiguous path cells produce multiple chains.
- `walk_places_non_overlapping` — fake `BuildCtx`, assert placed rects don't overlap and respect `SIDE_BUFFER_CELLS`.
- `rect_from_frontage_orientation` — every outward direction gives the right rect anchor.

Integration (requires GDMC server):
- `settlement_with_houses_on_streets` — full pipeline, visualize, assert ≥80% of front-row doors face their road.

Offline / property:
- Extend `pipeline_invariants_property_test` to also exercise the frontage pass against a synthetic block + path claim grid.
