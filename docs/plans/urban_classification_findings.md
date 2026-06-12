# Urban super-parcel classification — log findings & tuning notes

Analysis of the `district_classification` visual test across **9 runs** captured in
`output/logs/run_2026-06-10_*.log`. Goal: evaluate the prime-fallback + variable-size
(`URBAN_SIZE_MIN..=URBAN_SIZE_MAX`) urban logic in `src/generator/parcels/classification.rs`
and decide what constants/scoring to change.

Note: each run pulled a *different* build area from the live server (the area was moved between
runs), so this is effectively a 9-terrain sweep, not a single-seed repeat. That makes the
consistency of the findings below more meaningful.

## Constants in play (current values)

From `src/generator/parcels/constants.rs`:

| Const | Value | Role |
|---|---|---|
| `URBAN_SIZE_MIN` | 3 | city floor |
| `URBAN_SIZE_MAX` | 5 | city cap |
| `URBAN_GROWTH_CUTOFF` | 0.10 | candidate score to grow up to MIN |
| `URBAN_GROWTH_CUTOFF_HIGH` | 0.33 | higher bar to grow MIN→MAX |
| `URBAN_OPTION_SCORE_MAX` | 0.75 | max `district_score` to be an urban (prime) candidate |
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
`get_candidate_score` / `parcel_similarity_score` build from `1 - |diff|` terms. The
`water`/`forest`/`biome` terms use percentages in `[0,1]` (safe), but `gradient` and `roughness`
are **un-normalized** terrain metrics whose differences routinely exceed 1.0, so those terms go
negative and drag the whole score below zero.

Observed: candidate `201` scored **−0.785** and **−0.799** (run 162340); parcel-level
similarity scores down to **−0.51** (e.g. `Parcel 145 … -0.5062`).

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
`try_grow_city` only considers neighbours with `parcel_type == Unknown`. But
`classify_districts` sets every non-option super-parcel to Rural/OffLimits *before* growth
runs, so the only Unknown super-parcels left are the urban **options**.

→ `URBAN_OPTION_SCORE_MAX` is secretly the **growth-candidate gate**, not just the prime selector.
Cities can only grow into other option-pool super-parcels that happen to be adjacent. This is why
"No more candidates reachable" dominates: the option pool is small and/or not contiguous.

### 4. The best-ranked prime rarely wins (fallback over-fires)
The top prime (by `urban_parcel_score`) succeeded on the first try in only **2 of 9** runs. In the
other 7, between 2 and 5 primes failed before one stuck. Primes are ranked purely on the terrain
quality of a *single* super-parcel, but growability depends on having adjacent Unknown neighbours —
the two are nearly uncorrelated, so the ranking wastes its best candidates. The fallback logic itself
works correctly; it's just compensating for a prime metric that ignores adjacency.

### 5. Degenerate size-1 "city" (run 161751)
Only 2 urban options existed and both were isolated, so the final fallback committed a **single**
super-parcel as the entire city. A 1-block "city" is arguably worse than failing — the
final-fallback path needs a floor or a relaxation step (see recommendations).

### 6. More options ≠ bigger city
Option count ranged 2–15 but did not predict size: run 162027 had **15** options yet still finished
at 3 after 5 failed primes. Adjacency among options — not their count — is the limiter (reinforces #3).

## Recommended changes (priority order)

1. **Normalize/clamp the terrain terms** in both `get_candidate_score` and
   `parcel_similarity_score` — clamp each `1 - |diff|` to `[0,1]`, or normalize `gradient` by 3 and
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
   relax the cutoff/option threshold and retry, or refuse to commit a size-1 city. A 1-super-parcel
   "city" should not ship.

5. **Make prime ranking adjacency-aware** so the fallback stops over-firing: incorporate a cheap
   "number of adjacent Unknown options" (or summed adjacency) term into `urban_parcel_score`, so
   ungrowable primes don't sort to the top. (Optional; #1–#3 matter more.)

6. **Revisit `adjacency_score / candidate.size()`** in `get_candidate_score`. Dividing by candidate
   size collapses the adjacency credit of large neighbours to ~0 (e.g. `201` at 7448 cells), so the
   natural large block a city would expand into is structurally disfavoured.

`URBAN_GROWTH_CUTOFF = 0.10` looks acceptable as a floor (it admitted 0.17–0.50 joins, rejected the
−0.7 ones); revisit only after normalization.

---

# Round 2 — after the adjacency-size fix (10 runs)

Captured in `output/logs/run_20260611_13*.log` (10 runs, again a fresh live build area per run, so a
10-terrain sweep). The only code change since round 1 is recommendation #6: in `get_candidate_score`,
`adjacency_score = 1000.0 * adjacency_ratio / candidate.size()` was replaced with
`adjacency_score = adjacency_ratio` (the ratio is already a normalized `[0,1]` perimeter fraction).
Constants are unchanged from the table above. Findings #1–#5 from round 1 were **not** addressed.

## Per-run results (city = count of urban super-parcels)

| Run | #super-parcels | min SD size | max SD size | urban SDs | city size | largest urban SD |
|---|---|---|---|---|---|---|
| 131607 | 35 | 218 | 35,034 | 103, 79, 13 | 3 | 35,034 |
| 131916 | 16 | 218 | 27,922 | 5 | 1 | 27,922 |
| 132006 | 28 | 206 | 13,668 | 33, 87, 18 | 3 | 12,448 |
| 132157 | 18 | 268 | 27,560 | 96 | 1 | 880 |
| 132312 | 41 | 229 | 20,968 | 24 | 1 | 309 |
| 132404 | 22 | 246 | 27,157 | 35, 32, 197 | 3 | 2,969 |
| 132522 | 18 | 203 | 24,058 | 136 | 1 | 448 |
| 132634 | 38 | 208 | 18,430 | 23 | 1 | 208 |
| 132726 | 32 | 228 | 13,484 | 56, 180, 62 | 3 | 5,714 |
| 132826 | 19 | 212 | 42,928 | 59 | 1 | 42,928 |

**Size distribution: 5×size-1, 5×size-3. No size-4 or size-5** (round 1 was 1×1, 7×3, 1×4).

## Findings

### R2.1 The adjacency fix works as designed
Merge-phase candidate scores now track `adjacency_ratio` sanely: ratio `0.10 → ~0.50`, `0.39 → 0.74`,
`0.50 → 0.60`. The term moves the score instead of contributing ~`0.00004`. Growth can now select a
large neighbour — run 131607 absorbed a **35,034-cell** super-parcel that the old
`/candidate.size()` formula would have scored near zero. The structural bias against big blocks
(round-1 finding #6) is gone.

### R2.2 The fix also reshaped the *merge* phase (shared function)
`get_candidate_score(..., use_adjacency=true)` is called in **two** places — `get_best_merge_candidate`
(parcel→super-parcel, `merge.rs`) *and* `try_grow_city` (`classification.rs`). The fix changed
both. With adjacency now effective at 3× weight, merging is adjacency-dominated and runs
rich-get-richer: a large super-parcel has more border, so it attracts more merges.

### R2.3 Parcel size is now wildly skewed — the headline
Every run produces super-parcels spanning **~200 to 13,000–43,000 cells — a 60–200× range** (see
table). Consequence: **"city size = number of urban super-parcels" is a meaningless metric.** A
size-1 city is **42,928** cells in run 132826 but **208** cells in run 132634 — the same reported size
with a **200× area difference.** Two of the five size-1 "cities" (27,922 and 42,928 cells) are *larger
by area* than every size-3 city in this round. `URBAN_SIZE_MIN..=URBAN_SIZE_MAX = 3..=5` is counting
units that differ by ~100×, so the size target cannot produce consistent cities.

### R2.4 Degenerate-city mechanism unchanged (round-1 finding #5)
The genuinely tiny size-1 cities persist (208 / 309 / 448 / 880 cells). The starvation cause (round-1
finding #3 — growth candidates are only the Unknown option pool) is untouched; the fix changed *which*
neighbour is chosen, not *whether* eligible neighbours exist. The 5/10 size-1 rate is partly the broken
metric (two are huge) and partly real starvation (the four sub-900-cell ones).

### R2.5 Terrain terms still un-normalized (round-1 finding #1 still open)
Observed Candidate 127 scoring **−0.040** (run 131607). Rare and mild now, but the negative path still
exists — `gradient`/`roughness` are still raw `1 - |diff|`.

## Updated recommendations (priority order)

1. **Re-express city size in cells/area, or bound super-parcel size in the merge.** This now outranks
   everything else. The 3–5 count target cannot yield consistent cities while a single super-parcel
   ranges 200–43,000 cells. Either measure the city by total cell count, or cap super-parcel size
   during merge so the count is meaningful.
2. **A/B the merge impact of the adjacency fix.** The size skew (R2.3) is plausibly amplified by
   adjacency now driving the merge (rich-get-richer). Compare super-parcel sizes pre/post-fix, and
   consider giving the merge a *different*, size-aware scoring than growth, since they currently share
   `get_candidate_score`.
3. **Normalize/clamp `gradient` & `roughness`** (round-1 #1) — negatives still occur.
4. Round-1 recommendations #2–#5 remain valid but are blocked on a meaningful size metric (#1 above).

---

# Round 3 — after the size-band merge (Approach B), 10 runs

Captured in `output/logs/run_20260611_14*.log`. Code state: **Approach B** is implemented
(`merge_down` is now size-band driven — see `docs/plans/parcel_size_balancing.md`), so the merge
targets `S = interior_blocks / TARGET_PARCEL_AMOUNT` and holds interior parcels to
`[0.5S, 1.5S]`. The city-growth adjacency fix (measure adjacency against the whole urban set, not just
the prime) is **not** in these runs — growth still scores against the prime here. Round-1 #1
(un-normalized terrain terms) still open.

## Per-run results

| Run | S | band [L,U] | #interior | avg | min | max | max/min | out-of-band | city size |
|---|---|---|---|---|---|---|---|---|---|
| 140122 | 3154 | 1577–4731 | 17 | 2611 | 1603 | 4430 | 2.76 | 1 | 3 |
| 140221 | 3141 | 1570–4711 | 15 | 3134 | 1642 | 4410 | 2.69 | 0 | 5 |
| 140309 | 3141 | 1570–4711 | 17 | 2957 | 1696 | 4677 | 2.76 | 2 | 5 |
| 140536 | 3137 | 1568–4705 | 14 | 3586 | 2295 | 5996 | 2.61 | 1 | 4 |
| 140626 | 3159 | 1579–4738 | 16 | 3159 | 1837 | 4569 | 2.49 | 0 | 3 |
| 140720 | 3150 | 1575–4725 | 18 | 2800 | 1589 | 4366 | 2.75 | 2 | 3 |
| 140824 | 3120 | 1560–4680 | 18 | 2774 | 1619 | 3971 | 2.45 | 0 | 3 |
| 140943 | 3153 | 1576–4729 | 20 | 2432 | 1768 | 3509 | 1.98 | 0 | 3 |
| 141122 | 3128 | 1564–4692 | 18 | 2781 | 1617 | 5729 | 3.54 | 3 | 3 |
| 141340 | 3156 | 1578–4734 | 17 | 2782 | 1714 | 4910 | 2.86 | 1 | 5 |

(Interior super-parcel count entering the merge was ~193–197 in every run; `S` is stable at ~3120–3160
because the build areas were similar-sized.) City-size distribution: **6×size-3, 1×size-4, 3×size-5.**

## Findings

### R3.1 The size band works — the headline
Interior super-parcel spread collapsed from Round 2's **60–200×** to **max/min ≈ 2.0–2.9** in 9 of 10
runs (one outlier at 3.54). The ±50% band permits a max/min of 3.0, so the realised distribution is
essentially inside the target. Out-of-band parcels total **11 across ~170** (~6%), most runs 0–2.
**Approach B resolves R2.3.**

### R3.2 Every band violation is the upper tail, and it's the circularity caveat
All 11 out-of-band parcels are **above** the band (ratios 1.53–2.06); **none fall below the floor** —
the merge's "keep merging anything below L" loop reliably eliminates the lower tail. The upper
violations come from the gap between the **absolute** ceiling `U = 1.5*S` used by the merge and the
**realised** average used by the band check: when the final interior count exceeds the target 16
(runs landed at 14–20), the realised avg drifts *below* `S` (e.g. 140943: 20 parcels → avg 2432 vs
S 3153), so a parcel sitting near the absolute `U` reads as ratio > 1.5 against the lower realised avg.
This is exactly the circularity the plan flagged. **Fix options:** derive `S` from the *realised*
interior count (one corrective pass after the first merge), or shrink the ceiling toward
`U ≈ 1.3*S`, or re-derive the band from the realised avg and do a single rebalancing sweep.

### R3.3 One parcel above the absolute ceiling — the starvation relaxation, as designed
Run 141122's 5729-cell parcel exceeds even the absolute `U = 4692`. That's the intended
"merge-two-smalls / relax `U`" fallback firing when a below-L parcel had no in-cap parent — accepted
on purpose over stranding a parcel below the floor. Rare (1/10 runs); acceptable, but tightening the
fallback (prefer the *smallest* over-cap parent, or split afterward) would remove it.

### R3.4 Secondary win — city size un-pinned (no growth change needed)
City size went from Round 2's **5×size-1, 5×size-3** (half degenerate) to **6×3, 1×4, 3×5** — no
size-1 cities, and 4–5 now reached in 4/10 runs. Balancing the merge fixed the city-size pinning as a
side effect: evenly-sized interior super-parcels mean more, contiguous Unknown growth candidates, so
the starvation that capped cities at the minimum (R2.4 / round-1 #3) is largely relieved. This happened
**before** the growth adjacency fix.

### R3.5 Shape still unaddressed in these runs
Growth here still scores adjacency against the prime only, so the whispy/long city shape is expected to
persist (not visible in logs — only sizes are). The set-adjacency growth fix lands in the next round;
re-evaluate compactness then.

## Updated recommendations

1. **Close the upper-band gap (R3.2).** Make the band relative to the realised average: either re-derive
   `S` from the realised interior count and do one corrective sweep, or set `U ≈ 1.3*S`. Target 0
   out-of-band.
2. **Tighten the starvation fallback (R3.3)** so it can't leave a parcel well above `U`.
3. **Re-run the sweep after the set-adjacency growth fix** to confirm compactness improves (R3.5) and
   city sizes stay healthy (R3.4).
4. **Normalize/clamp `gradient` & `roughness`** (round-1 #1) — still open.

## Non-constant issues spotted while reading logs
- **Misleading log string**: the parcel-level else-branch in `classify_parcels` logs
  `"classified as Off-Limits"` while actually assigning `ParcelType::Rural`. Cosmetic, but it
  confuses log analysis.

## How to reproduce
```
cargo test district_classification -- --nocapture
```
Then read the newest `output/logs/run_*.log`; the decision trail is the
`Tome::generator::parcels::classification` lines: `ranked best-first`, per-candidate scores,
`below cutoff`/`No more candidates reachable`, and `grew a city of size N`. Final per-super-parcel
type/size + sign coordinates are the `Tome::generator::parcels::test::tests` lines at the end.
```
