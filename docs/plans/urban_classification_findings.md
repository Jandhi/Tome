# Urban super-district classification — log findings & tuning notes

Analysis of the `superdistrict_classification` visual test across **9 runs** captured in
`output/logs/run_2026-06-10_*.log`. Goal: evaluate the prime-fallback + variable-size
(`URBAN_SIZE_MIN..=URBAN_SIZE_MAX`) urban logic in `src/generator/districts/classification.rs`
and decide what constants/scoring to change.

Note: each run pulled a *different* build area from the live server (the area was moved between
runs), so this is effectively a 9-terrain sweep, not a single-seed repeat. That makes the
consistency of the findings below more meaningful.

## Constants in play (current values)

From `src/generator/districts/constants.rs`:

| Const | Value | Role |
|---|---|---|
| `URBAN_SIZE_MIN` | 3 | city floor |
| `URBAN_SIZE_MAX` | 5 | city cap |
| `URBAN_GROWTH_CUTOFF` | 0.10 | candidate score to grow up to MIN |
| `URBAN_GROWTH_CUTOFF_HIGH` | 0.33 | higher bar to grow MIN→MAX |
| `URBAN_OPTION_SCORE_MAX` | 0.75 | max `superdistrict_score` to be an urban (prime) candidate |
| `RURAL_OPTION_SCORE_MAX` | 1.5 | rural/off-limits split |
| `ADJACENCY_WEIGHT` | 3.0 | adjacency weight in `get_candidate_score` |

## Per-run results

| Run | Urban options | Top prime won? | Final city size |
|---|---|---|---|
| 151411 | 12 | yes, first try (58) | 3 |
| 152246 | 10 | yes, first try (39) | 3 |
| 161454 | 10 | no — 5 primes failed → 169 | 3 |
| 161751 | 2 | no — both failed | **1 (degenerate)** |
| 161941 | 13 | no — 2 failed → 251 | 3 |
| 162027 | 15 | no — 5 failed → 54 | 3 |
| 162121 | 6 | no — 3 failed → 150 | 3 |
| 162216 | 13 | no — 5 failed → 58 | **4** |
| 162340 | 10 | no — 2 failed → 50 | 3 |

**Size distribution: 1×size-1, 7×size-3, 1×size-4, 0×size-5.**

## Findings

### 1. Scores go strongly negative — the root cause
`get_candidate_score` / `district_similarity_score` build from `1 - |diff|` terms. The
`water`/`forest`/`biome` terms use percentages in `[0,1]` (safe), but `gradient` and `roughness`
are **un-normalized** terrain metrics whose differences routinely exceed 1.0, so those terms go
negative and drag the whole score below zero.

Observed: candidate `201` scored **−0.785** and **−0.799** (run 162340); district-level
similarity scores down to **−0.51** (e.g. `District 145 … -0.5062`).

Consequence: the cutoffs (`0.10`, `0.33`) sit on a scale running roughly **−0.8 .. +1.0**, so they
don't mean "fraction similar." **Every other constant is being tuned against a moving target until
these terms are clamped/normalized to `[0,1]`.** This is the #1 fix.

### 2. City size is effectively pinned to the minimum (3)
7 of 9 runs landed exactly on 3; only one reached 4; none reached 5; one collapsed to 1. The
requested 3–5 variability is not happening in practice. The mechanism *can* produce 4 (proven once,
run 162216), but it's rare.

The pin happens for two *different* reasons depending on terrain:
- **Cutoff-bound** (flatter terrain): the would-be 4th member scores below
  `URBAN_GROWTH_CUTOFF_HIGH = 0.33` (e.g. earlier flat run: next candidate −0.166).
- **Starvation-bound** (rougher terrain): growth runs out of eligible neighbours — the dominant
  failure mode in the logs ("No more candidates reachable").

### 3. Growth candidates == the urban-option pool (key structural coupling)
`try_grow_city` only considers neighbours with `district_type == Unknown`. But
`classify_superdistricts` sets every non-option super-district to Rural/OffLimits *before* growth
runs, so the only Unknown super-districts left are the urban **options**.

→ `URBAN_OPTION_SCORE_MAX` is secretly the **growth-candidate gate**, not just the prime selector.
Cities can only grow into other option-pool super-districts that happen to be adjacent. This is why
"No more candidates reachable" dominates: the option pool is small and/or not contiguous.

### 4. The best-ranked prime rarely wins (fallback over-fires)
The top prime (by `urban_district_score`) succeeded on the first try in only **2 of 9** runs. In the
other 7, between 2 and 5 primes failed before one stuck. Primes are ranked purely on the terrain
quality of a *single* super-district, but growability depends on having adjacent Unknown neighbours —
the two are nearly uncorrelated, so the ranking wastes its best candidates. The fallback logic itself
works correctly; it's just compensating for a prime metric that ignores adjacency.

### 5. Degenerate size-1 "city" (run 161751)
Only 2 urban options existed and both were isolated, so the final fallback committed a **single**
super-district as the entire city. A 1-block "city" is arguably worse than failing — the
final-fallback path needs a floor or a relaxation step (see recommendations).

### 6. More options ≠ bigger city
Option count ranged 2–15 but did not predict size: run 162027 had **15** options yet still finished
at 3 after 5 failed primes. Adjacency among options — not their count — is the limiter (reinforces #3).

## Recommended changes (priority order)

1. **Normalize/clamp the terrain terms** in both `get_candidate_score` and
   `district_similarity_score` — clamp each `1 - |diff|` to `[0,1]`, or normalize `gradient` by 3 and
   `roughness` by `OFF_LIMITS_ROUGHNESS` (6.0) before the subtraction. Do this first; everything else
   is uninterpretable without it.

2. **Re-tune `URBAN_GROWTH_CUTOFF_HIGH` (0.33 → ~0.20–0.25)** *after* normalization, so size 4–5 is
   reachable when a decent neighbour exists (run 162216 proved 4 is possible). Re-measure the score
   distribution from a normalized run before locking a value.

3. **Loosen `URBAN_OPTION_SCORE_MAX` (0.75 → ~0.9–1.0)** to widen the Unknown-neighbour pool and cut
   starvation — this is the actual size cap on rough terrain (findings #3, #6). Alternatively,
   **decouple growth candidates from the option pool**: let `try_grow_city` consider adjacent
   Rural-ish neighbours (gated by the score cutoff) instead of only Unknown options.

4. **Fix the degenerate-city fallback** (run 161751): if no prime reaches `URBAN_SIZE_MIN`, either
   relax the cutoff/option threshold and retry, or refuse to commit a size-1 city. A 1-super-district
   "city" should not ship.

5. **Make prime ranking adjacency-aware** so the fallback stops over-firing: incorporate a cheap
   "number of adjacent Unknown options" (or summed adjacency) term into `urban_district_score`, so
   ungrowable primes don't sort to the top. (Optional; #1–#3 matter more.)

6. **Revisit `adjacency_score / candidate.size()`** in `get_candidate_score`. Dividing by candidate
   size collapses the adjacency credit of large neighbours to ~0 (e.g. `201` at 7448 cells), so the
   natural large block a city would expand into is structurally disfavoured.

`URBAN_GROWTH_CUTOFF = 0.10` looks acceptable as a floor (it admitted 0.17–0.50 joins, rejected the
−0.7 ones); revisit only after normalization.

## Non-constant issues spotted while reading logs
- **Misleading log string**: the district-level else-branch in `classify_districts` logs
  `"classified as Off-Limits"` while actually assigning `DistrictType::Rural`. Cosmetic, but it
  confuses log analysis.

## How to reproduce
```
cargo test superdistrict_classification -- --nocapture
```
Then read the newest `output/logs/run_*.log`; the decision trail is the
`Tome::generator::districts::classification` lines: `ranked best-first`, per-candidate scores,
`below cutoff`/`No more candidates reachable`, and `grew a city of size N`. Final per-super-district
type/size + sign coordinates are the `Tome::generator::districts::test::tests` lines at the end.
```
