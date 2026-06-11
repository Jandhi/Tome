# Plan — balance final district sizes (urban & rural within ±50% of average)

> **Status: IMPLEMENTED via Approach B** (size-band-driven `merge_down`) on branch `td/improvements`.
> Validated by the Round-3 log sweep — see `docs/plans/urban_classification_findings.md`, findings
> R3.1–R3.5. Interior spread collapsed from **60–200×** to **max/min ≈ 2.0–2.9** (the ±50% band's
> ceiling is 3.0). Residual work is tracked under "Remaining work" below. The other approaches (A, C,
> D, E) are retained as "Alternatives considered" for context and future tuning.

## Goal

Final **urban** and **rural** districts (i.e. interior super-districts after `merge_down`) should each
have a block count within **±50% of the average** interior-district block count:

```
0.5 * avg  ≤  district_blocks  ≤  1.5 * avg        (for every Urban/Rural district)
```

**Off-limits districts are exempt** — tiny or huge off-limits regions are fine; the goal there is only
to *minimise how much of the map is off-limits*, not to size them. "Blocks" = `data.points_2d.len()`
(surface cells), the metric logged by the visual test as `size=N cells`.

Before this change, the Round-2 sweep (findings R2.3) showed interior super-districts spanning
~200 → 43,000 blocks in every run — a 60–200× spread.

## Root cause (what the old merge did)

The size distribution is decided entirely in `merge_down` (`src/generator/districts/merge.rs`), which
*used to be* **count-driven and size-blind**:

1. Start with one super-district per base district (~195–250, uneven Voronoi sizes).
2. `while super_district_count > TARGET_DISTRICT_AMOUNT (16)`: pick the **smallest** super-district as
   `child`, merge it into the **best-scoring border-matching neighbour** (similarity score had to clear
   a hard `0.33` gate), decrement count.
3. Stop at 16 (or strand leftover smalls when no merge cleared the gate).

There was **no upper bound on a parent's size** and **no target size** — only a count. With the
now-effective adjacency term (3× weight) dominating the merge score, a large super-district wraps more
of each small child's border, wins the merge, grows, and wins again (rich-get-richer, finding R2.2). One
or two blobs swelled to tens of thousands of blocks while stranded smalls sat at the ~200 floor.

Two structural facts the fix exploits:
- **`is_border` is known before merging**, and the merge only ever joins border↔border or
  interior↔interior. So the interior pool (→ urban/rural) is identifiable at merge time and balanced
  independently of off-limits.
- A super-district is just a **set of base districts** (`SuperDistrict.districts`) — movable,
  roughly-contiguous mass.

## Definitions

- **Interior pool** — non-border super-districts (the ones that become urban/rural).
- **Target size `S`** — desired average interior-district block count. **Implemented count-driven:**
  `S = interior_blocks / TARGET_DISTRICT_AMOUNT`, computed per run from the interior (non-border) mass
  only. (The size-driven alternative — a fixed `TARGET_DISTRICT_BLOCKS` constant — was rejected because
  live build areas vary in size, so an absolute block target would yield wildly different district
  counts on small vs large areas. Deriving `S` from the actual interior mass keeps the band relative to
  each run and lands the realised average near `S`.)
- **Band** — `L = DISTRICT_SIZE_LOWER_FACTOR * S` (= `0.5*S`), `U = DISTRICT_SIZE_UPPER_FACTOR * S`
  (= `1.5*S`).

---

## What was implemented (Approach B — capacity-constrained merge toward a size band)

`merge_down` was rewritten from count-driven to **size-band-driven**. Pseudocode of the shipped logic:

```
interior_blocks = Σ block_size(sd) for non-border sd        # block_size = points_2d.len()
S = max(1, interior_blocks / TARGET_DISTRICT_AMOUNT)
L, U = 0.5*S, 1.5*S
loop:
    child = smallest non-ignored super-district with block_size < L      # interior OR border
    if none: break                                                       # everything is at/above the floor
    cap = Some(U) if child is interior else None                         # off-limits has no ceiling
    parent = pick_balanced_parent(child, neighbours, cap)                # best in-cap same-type neighbour
    if parent is None:                                                   # starvation
        parent = smallest same-type neighbour (ignore cap)              # merge two smalls / relax U
    if parent is None:                                                   # truly isolated
        remove child if block_size < 10 else ignore it; continue
    merge(child -> parent)
```

Key implementation points (all in `src/generator/districts/`):

- **`merge_down` (merge.rs)** — computes `S`/`L`/`U` up front from the interior pool (logs
  `Merge size band: interior_blocks=… target S=… band [L=…, U=…]`), then loops *while any district is
  below `L`* instead of *while count > 16*. The below-`L` set shrinks monotonically (every merge
  removes a child and only grows the parent), so the loop terminates.
- **Both bounds, by construction** — the ceiling `U` is enforced on every interior merge via the cap;
  the floor `L` is the loop's termination condition, so no interior district is left below `L` unless it
  is genuinely isolated.
- **Border districts are exempt from the ceiling** (`cap = None`) but still coalesced up to `L`, so
  off-limits regions can be any size yet don't fragment into hundreds of slivers.
- **`pick_balanced_parent` (merge.rs)** replaces the old `get_best_merge_candidate`: it filters to
  same-border-type neighbours, **rejects any neighbour where `parent.size + child.size > cap`** (this is
  what breaks rich-get-richer), and among survivors picks the highest `get_candidate_score`. The old
  hard **`> 0.33` similarity gate was removed** — similarity is now only a *tiebreaker for where to send
  the child*, never a gate that could strand it below the floor. The merge keeps the original
  child-centric adjacency ratio (fraction of the child's perimeter facing the parent).
- **Starvation rule** — if no same-type neighbour fits under `U`, the child merges into its *smallest*
  same-type neighbour anyway (combine two smalls; may land slightly above `U`), preferring a mild
  upper-band overshoot to a stranded below-`L` district. Truly isolated below-`L` pockets are dropped if
  `< 10` cells, else left as-is and ignored.
- **Constants (constants.rs)** — `TARGET_DISTRICT_AMOUNT` is now documented as the *desired interior
  count* that sets `S`; added `DISTRICT_SIZE_LOWER_FACTOR = 0.5` and `DISTRICT_SIZE_UPPER_FACTOR = 1.5`.
- **`get_candidate_score` (merge.rs)** was made to take a caller-supplied `adjacency_ratio: Option<f32>`
  so the merge and city-growth can use *different* adjacency references — resolving R2.2's
  "merge should be scored differently from growth." (Growth's set-adjacency change is a separate fix.)

### Result (Round-3 sweep)

`max/min ≈ 2.0–2.9` in 9/10 runs (one outlier 3.54); ~6% of interior districts out-of-band, all on the
upper tail; **no district below the floor**. As a side effect the city-size pinning (R2.4) was relieved
— evenly-sized interior districts give growth more contiguous candidates, so city size went from
5×size-1,5×size-3 to 6×3,1×4,3×5. Full detail in findings R3.1–R3.5.

---

## Alternatives considered (not implemented)

- **Approach A — hard ceiling only** (skip merges past `U`, keep count loop): ~20 lines, but bounds only
  the upper side and strands smalls below `L`. Approach B is A plus the floor-driven loop + starvation
  rule, so B supersedes it.
- **Approach C — soft size penalty in the merge score** (`balance = 1 - (projected-S)/(U-S)`): minimal
  change, smoothly reduces skew, but **no hard ±50% guarantee**. Still useful as a future refinement to
  bias *which* in-cap parent wins (currently the similarity tiebreaker), if the upper tail needs more
  shaping.
- **Approach D — post-merge split + absorb**: corrective pass that *splits* any `> U` district and
  absorbs any `< L`. Handles distributions merge alone can't (one unavoidable giant), but needs a
  contiguous base-district splitter and incremental adjacency recompute — most new code. Reserved for if
  the residual upper-band overshoots (R3.2/R3.3) prove unfixable by tuning.
- **Approach E — balanced partition upstream** (area-balanced Lloyd / capacity-Voronoi at spawn time):
  most principled, removes the input variance, but the biggest/riskiest change and only approximate.
  Overkill now that the merge-stage fix meets the spec.

## Remaining work (from the Round-3 sweep)

1. **Close the upper-band gap (R3.2).** All band violations are upper-tail and come from the **absolute**
   ceiling `U = 1.5*S` vs the **realised** average: when the final interior count exceeds the target 16
   (runs landed at 14–20), the realised avg drifts below `S`, so a district near `U` reads as ratio
   > 1.5. Options: re-derive `S` from the realised interior count and do one corrective sweep; or tighten
   the ceiling to `U ≈ 1.3*S`; or re-derive the band from the realised avg and rebalance once. Target 0
   out-of-band.
2. **Tighten the starvation fallback (R3.3).** Run 141122 produced a 5729-cell district above the
   absolute `U` — the intended relax-`U` fallback firing. Prefer the *smallest* over-cap parent, or split
   afterward, to remove the overshoot.
3. **Normalize/clamp `gradient` & `roughness`** (round-1 #1, still open) — negatives still occur in the
   merge score (e.g. −0.04), mildly distorting the similarity tiebreaker.

## Verification (as built)

- **Visual-test metric** — after `classify_superdistricts`, the test (`districts/test.rs`) computes the
  interior (Urban∪Rural) average and logs per-district `Band check: … size=… avg=… ratio=… in_band=…`
  plus a `Band summary: … avg=… min=… max=… max/min=… band […] N out of band` line. Off-limits skipped.
- **Sweep** — `cargo test superdistrict_classification -- --nocapture`, then read the newest
  `output/logs/run_*.log`; tabulate the `Merge size band` and `Band summary` lines (same format as the
  R3 table). The 60–200× spread should read ≤3× max/min.
- **Growth guard** — the merge's size penalty must not leak into `try_grow_city`; confirmed by the
  separate `adjacency_ratio` argument on `get_candidate_score` (merge and growth pass different ratios).

## Open questions

- **Relative vs absolute band** — accept the absolute `[0.5S, 1.5S]` band (simple, ~6% upper-tail
  violations) or invest in the realised-average correction (Remaining work #1) for strict ±50%? Leaning
  toward tightening `U` first as the cheapest path.
- **Average over what** — currently a single combined Urban∪Rural average. Hold urban and rural to
  *separate* bands instead? Spec said "those urban and rural," read as one interior average.
- **Stranded tiny pocket** — force-merge (relax `U`) or reclassify off-limits? Implemented as
  drop-if-`<10`-else-leave; revisit if isolated interior pockets show up in practice.
```
