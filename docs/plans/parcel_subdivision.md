# Parcel Subdivision + Frontage Houses

Session log: 2026-05-24. Branch: `jd/placement` (with stair-door fix touching `buildings_v2`).

The goal of the day was to make urban super-parcels more amenable to road-frontage house placement: every cell should be within reach of a road, and the road network should look like a medieval town instead of voronoi blobs. Houses were threaded back in to judge whether sub-block sizing was actually buildable.

---

## What got built

### `src/generator/parcels/subdivide.rs` — recursive BSP partitioner

Takes a `HashSet<Point2D>` of cells and returns `(Vec<HashSet<Point2D>>, HashSet<Point2D>)` — sub-blocks + alley cells.

- **Stop condition:** both bounding-box dims ≤ `max_dim` (default 32).
- **Axis choice:** if both dims ≥ `2 * max_dim`, pick at random; otherwise prefer the axis that still needs cutting; fallback to the longer side.
- **Cut location:** uniform in `[axis_min + margin, axis_max - margin - 1]` where `margin = max_dim / 4`. Off-center cuts give widely varied sub-block sizes (avoiding the stacked-grid look).
- **Alley width:** 2 cells (rows `v == cut` and `v == cut + 1`).
- **Connected components:** after cutting, each side is split into connected components and recursed independently — handles concave/voronoi-shaped inputs naturally.

Sibling `voronoi_subdivide_block(cells, rng, sections)` wraps the existing `voronoi_fill_with_recenter` and derives 2-wide alleys by marking any cell whose cardinal neighbour belongs to a different section.

### `src/generator/parcels/test.rs::subdivide_urban_with_houses`

Live-server viz/integration test. Pipeline:

1. `generate_parcels` → filter `ParcelType::Urban` super-parcels.
2. For each urban SD: take a 2-cell perimeter ring as the parcel road (`get_outer_and_inner_points(_, 2)`), subdivide the interior.
3. **Mix strategies:** alternate `subdivide_block` (BSP) and `voronoi_subdivide_block` per super-parcel so adjacent parcels visually compare the two patterns.
4. Claim every road cell (perimeter + alleys) as `BuildClaim::Path(PathType::Pavement)` so the frontage walker treats them as roads.
5. Paint road cells as polished_andesite, **one Y below** ground (sunken path).
6. For each sub-block: run the frontage pass only (interior fill currently commented out — see "Tuning").

### Per-house palette + roof rolling

Inlined the frontage walker so every house gets a fresh roll of:

- **Wood palette:** oak / spruce / dark_oak
- **Stone palette:** stone_bricks / cobblestone / deepslate
- **Roof palette:** acacia / brick / oak / red_wood
- **Roof pitch:** Slab / Stairs / Double

≈108 distinct combos. Helper `roll_palette` stacks `base.merged_with(wood).merged_with(stone).merged_with(roof)` — partial palettes only override their respective roles.

### Stair-blocks-door fix in `buildings_v2/floors/stairs.rs`

Two distinct bugs were covering doors with stair steps:

1. **Wall-direction check was too coarse and too loose in spirals.** The old `door_facings: HashSet<Cardinal>` check rejected stairs by *which wall direction* they touched, not *which cells*. Spiral specifically used `walls.iter().all(...)` (rejected only when *all* adjacent walls had doors), so a spiral on a corner with one door-bearing wall got placed across the doorway.

   **Fix:** new helper `doorway_cells_on_floors(wall_segs, &[floor, floor + 1])` returns `HashSet<Point2D>` of interior cells one step inside each door (`wall_cell + seg.facing`; facing points inward per CW polygon convention). `pick_stair_for_floor` rejects any candidate whose positions overlap that set. Checks both floors because a stair spans both.

2. **`pick_attic_stair` had no door check at all.** This was the actual cause of the screenshot bug. For a 1-story House with `RoofStyle::Gable(Double)` (random ~33% of Houses), `has_attic = true` and the attic stair sits on floor 0 — same y-layer as the ground door. Threaded `wall_segs` and `top_floor` into `pick_attic_stair` and applied the same `doorway_cells_on_floors` check.

`pipeline_invariants_property_test` (240 buildings × 20 seeds) still passes.

---

## Settings / tuning levers

Hardcoded in the test, easy to vary:

| Lever | Current | Notes |
|---|---|---|
| `max_dim` (BSP) | 32 | Lower → denser road grid. |
| `margin` (BSP) | `max_dim / 4` | Lower → more lopsided sub-blocks. |
| Alley width | 2 cells | Hard-coded in `subdivide_block` recurse. |
| Voronoi sections | `inner.len() / 400` | Lower divisor → more, smaller sub-blocks. |
| Perimeter ring depth | 2 | `get_outer_and_inner_points(_, 2)`. |
| Frontage size pool | `[SizeClass::House]` | 1–2 floor bias. Switch to `[Hall]` for 2–3. |
| Interior fill | **disabled** | Commented out — gardens/yards left open. |

---

## Open / next

- **Voronoi alley width.** Voronoi alleys are exactly 2 wide because every cell with a different-section neighbour is marked. BSP alleys are exactly 2 wide because we mark `cut` and `cut + 1`. Both consistent, but voronoi alleys can fork in unusual ways near triple-points — could be visually noisy at high section counts.
- **No min-spacing enforcement between alleys.** The BSP recursion naturally avoids placing close-together cuts because the stop condition kicks in once dims drop below `max_dim`. But voronoi gives no such guarantee.
- **Frontage walker on irregular sub-blocks.** The walker assumes a clear road-adjacent chain. Voronoi sub-blocks have wiggly perimeters — chains break at concave points and we may want a fallback (or use `detect_perimeter_frontages` more aggressively).
- **Re-enable interior fill** with a sensible size class and density once frontage looks right.
- **`max_dim` tuning per super-parcel** — wealthier/denser super-parcels could get smaller `max_dim` for tighter blocks.
- **City-wide consistency** — currently every SD picks wood/stone/roof per *building*. Could anchor a wealthier "stone manor" parcel vs a poorer "wattle cottage" parcel.

---

## Tangential

- Fixed merge conflicts on PR #17 (`jd/houses` → `master`): only `.claude/settings.local.json` (modify-on-master / delete-on-`jd/houses`). Resolved by keeping it deleted — file is gitignored on `jd/houses`.
