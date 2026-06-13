# Plan — density-targeted, flatness-scored city wall gates

> **Status: IMPLEMENTED** in `src/generator/districts/gate.rs`. Towers are explicitly out of
> scope — `build_wall_towers` stays as-is. `N` rounding uses `ceil` (see Open questions).

## Goal

Replace the greedy first-fit gate placement in `build_wall_gate` (`src/generator/districts/gate.rs`)
with a **select-then-build** scheme:

1. **Target count from wall length** — roughly **1 gate per 1,000 wall points** of perimeter
   (`N = max(1, round(len / GATE_TARGET_SPACING))`).
2. **Minimum spacing well below the average spacing** — gates must be at least
   `MIN_GATE_SPACING` (~150) apart along the wall, but are otherwise free to sit wherever the
   wall is best, rather than being pinned to a fixed cadence.
3. **Prefer "perfect" places** — long, flat, straight wall sections — scored and ranked, while
   keeping today's validity rule as the candidate floor, so a gate can still land anywhere the
   current code could place one when nothing better exists.

## Current behavior (what's wrong)

`build_wall_gate` walks the ordered wall loop with a cooldown counter:

- `gate_possible` starts at **0**, so the **first** index passing `is_gate_possible` gets a gate.
- After each gate, `gate_possible = 60` and decrements per point — so on a long straightish wall
  you get a gate roughly **every 60 points** (a 2,000-point wall → ~30 gates; we want ~2).
- Placement is **first-fit**: the gate lands at the first valid index after cooldown expiry, not
  the best one. A barely-valid spot (1-block height step, minimal straight run) wins over a
  perfectly flat rampart 20 blocks later.

`is_gate_possible(point, wall_list, gate_size, index)` is the validity rule worth keeping:
the `gate_size = 7` run starting at `index` must be axis-aligned straight
(`is_straight_not_diagonal_point2d`) and the wall-top height difference across the run must be
≤ 1. It does **not** loop past the end of the list and does **not** check water.

Call sites (all in `src/generator/districts/wall.rs`) — one per wall variant, each passing its
own ordered loop:

| Caller | Gate structure | Extra constraint |
|---|---|---|
| `build_wall_palisade` (wall.rs:164) | `basic_palisade_gate` | none |
| `build_wall_standard` (wall.rs:242) | `basic_thin_gate` | none |
| `build_wall_standard_with_inner` (wall.rs:383) | `basic_wide_gate` | inner-wall clearance check (gate.rs:123-127) |

`build_wall` calls these **per ordered loop** (`order_wall_points` can return several loops), so
the count target is naturally per-loop.

## Design

Restructure `build_wall_gate` into three phases. The build phase reuses the existing per-type
placement code unchanged (claims, air carving, `place_structure`, `gate_locations` push).

### Phase 1 — enumerate + score candidates

One pass over the ordered loop. Index `i` is a **candidate** iff:

- `is_gate_possible(i)` holds (unchanged — this is the fallback guarantee), and
- for the wide gate only: the inner-wall clearance check currently buried in the build loop
  (gate.rs:123-127) passes. Moving it into candidacy means selection never wastes one of its `N`
  slots on a spot that would silently fail to build, which is a live bug today (`gate_possible`
  is never reset when that check breaks out).
- **hard reject** if any point of the gate run is `WallType::Water`/`WaterWall` or the gate's
  ground cell is water (`editor.world().is_water`). Today nothing stops a gate opening into
  water; with only ~N gates per wall each one must be usable.

Score each candidate (higher = better); weights are constants next to the function:

| Term | Measure | Why |
|---|---|---|
| Wall-top flatness | `-(max_y - min_y)` of `wall_points_with_height[i-PAD ..= i+GATE_SIZE+PAD]` (`PAD ≈ 4`, clamped to the loop) | A gate centered in a flat rampart, not at the edge of a slope. The ≤1 rule only checks the run's two endpoints; this looks wider and penalizes bumpy surroundings. |
| Terrain flatness | `-(max - min)` of `get_height_at` over the same window | Wall top can be smooth (rate-limited by `add_wall_points_height`) while the ground below is a slope; doors should open onto level ground. |
| Straight-run length | How far the axis-aligned straight run extends on both sides of the gate, capped (e.g. at 16/side) | Prefers gates centered in long flat sections — the "perfect places". |

A candidate on a perfectly flat, long section scores 0 + bonus; every defect subtracts. No
threshold excludes a scored candidate — bad spots merely rank last, which is exactly the
"still placeable where they currently are if needed" requirement.

### Phase 2 — select up to N with a spacing floor

```
N = max(1, round(loop_len / GATE_TARGET_SPACING))          # GATE_TARGET_SPACING = 1000
sort candidates by score desc (tie: stable by index)
selected = []
for c in candidates:
    if ring_distance(c, s) >= MIN_GATE_SPACING for all s in selected:
        selected.push(c)
    if selected.len() == N: break
```

- `ring_distance(i, j)` = `min(|i-j|, loop_len - |i-j|)` — index distance along the loop,
  circular, since the wall is a closed ring. (If a loop came out open from the
  `order_wall_points` reversal fallback, fall back to `|i-j|`; the first/last points being
  neighbours is cheap to check.)
- `MIN_GATE_SPACING = 150` — deliberately much smaller than the 1,000 average so two genuinely
  good spots that happen to be 200 apart can both win; it only prevents adjacent/overlapping
  gates. Both constants `pub` in `gate.rs` and documented as tunable.
- Greedy best-first is sufficient here: with `MIN_GATE_SPACING ≪ GATE_TARGET_SPACING` the
  spacing constraint almost never forces a trade-off, so optimal selection (DP over the ring)
  isn't worth the code.
- **Under-supply is accepted**: if fewer than `N` candidates exist (short loop, rough terrain,
  water), place what we have. Loops with zero candidates place zero gates — same as today.

### Phase 3 — build

For each selected index (in loop order), run the existing per-type build code, extracted into
`place_palisade_gate / place_thin_gate / place_wide_gate` helpers (or one helper with a match)
so the selection loop is shared across all three wall types. No cooldown counter remains.
`gate_locations` keeps being pushed for the road network (`docs/plans/road_hierarchy.md` relies
on it for gate-to-road routing).

### Signature change

```rust
pub async fn build_wall_gate(
    wall_points: &Vec<Point3D>,
    ...                       // existing args unchanged
)
```

stays outwardly the same — `_rng` is already unused and remains so (selection is deterministic
given the wall; the wall itself is already seed-driven). Internally split into
`fn gate_candidates(...) -> Vec<GateCandidate>` (pure, unit-testable) and the async build phase.

```rust
struct GateCandidate { index: usize, score: f64 }
```

## Constants

| Constant | Value | Meaning |
|---|---|---|
| `GATE_TARGET_SPACING` | 1000 | Wall points per desired gate; `N = max(1, round(len / this))`. |
| `MIN_GATE_SPACING` | 150 | Floor on ring distance between two selected gates. |
| `GATE_SIZE` | 7 (existing) | Length of the straight run a gate occupies. |
| `FLATNESS_PAD` | 4 | Extra points on each side of the run inspected by the flatness terms. |
| `STRAIGHT_RUN_CAP` | 16 | Per-side cap on the straight-run bonus. |

## Out of scope / unchanged

- **Towers** — `build_wall_towers` keeps its 80-block cadence. (Optional cheap guard, only if it
  falls out naturally: penalize candidates whose window overlaps a tower base claim, since gates
  carve air and could currently blast through a tower. Not a requirement of this plan.)
- Gate structures, claim types, air carving heights, `gate_locations` consumers.
- `is_gate_possible` semantics (reused verbatim as the candidacy floor).

## Testing

- **Unit tests on `gate_candidates` + selection** (no server, no editor for selection; synthetic
  `World` for height-dependent scoring, mirroring the `tops_for` harness in `wall.rs` tests):
  - `N` math: loops of length 300 / 1,000 / 2,500 → 1 / 1 / 2–3 gates (whatever rounding we fix).
  - Flat-beats-bumpy: a loop with one flat straight section and one barely-valid stepped section
    → the flat index is selected first.
  - Spacing floor: two top-scoring candidates 40 apart → only one selected; 200 apart → both.
  - Fallback: a loop where only barely-valid (height-diff 1, short run) candidates exist → they
    are still selected.
  - Ring distance: best candidates near index 0 and index `len-10` are treated as close.
- **Live check**: existing wall tests in `districts/test.rs` against a server; count `Placing
  ... gate` log lines vs wall length in `output/logs/run_*.log` and eyeball gate quality in the
  visualizer.

## Open questions

- **Rounding for `N`**: ~~`round`~~ **resolved → `ceil`** (1 gate per *started* 1,000 points), so a
  1,001-point wall already gets 2 gates and a 2,500-point wall gets 3. `N = max(1, ceil(len / 1000))`.
- **Guaranteed gate per loop?** A loop with zero candidates gets zero gates (status quo). If a
  city must always be enterable, a relaxation pass (height-diff ≤ 2, shorter run) could run when
  the candidate set is empty — deferred until it's observed in practice.
- **Score weights**: start with equal weights on the two flatness terms and a half-weight
  straight-run bonus; tune from live runs.
