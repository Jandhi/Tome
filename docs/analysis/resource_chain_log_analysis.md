# Resource Chain — Log Analysis (2026-06-14)

Analysis of the resource-chain behaviour captured in today's run logs under
`output/logs/2026-06-14/`. Twelve runs emitted the `[resource-chain]` settlement
production report (added this session in `resolve_for_parcels`). All generation
runs used `Seed(12345)`; the live server hands out a different build area each
run, so the biome mix — and therefore the resource outcome — varies between runs.

> **Note on the two log "eras".** Runs `110911`–`112613` were produced with the
> *original* (ban-based) cap logic. The `114124` run is the *post-fix* offline
> diversity test (`plains_parcels_stay_diverse`, parcels `0..11`, all plains).
> Together they give a clean before/after.

## Method

Each report logs, per settlement: producing-parcel count, per-parcel resource +
gather building, aggregate raw supply, finished goods, leftover goods, and gather
/ processing building counts. The lines are prefixed `[resource-chain]` for
grepping:

```
grep "resource-chain" output/logs/2026-06-14/*.log
```

## Per-run summary (pre-fix generation runs)

| Run | Producing / total | Dominant resource | Wasted (leftover) raw | Notable |
|-----|------------------|-------------------|-----------------------|---------|
| 110911 | 10 / 11 | wood ×4 | **coal ×6** | tools ×4, furniture ×2; all coal idle |
| 111106 | 11 / 11 | **iron_ore ×6** | **iron_ore ×12, coal ×2** | **0 tools made** — iron chain fully broke |
| 111155 | 11 / 12 | wood ×4 | **coal ×6** | tools ×5; all coal idle |
| 111355 | **6 / 13** | wheat ×2 | none | ~54% of rural parcels idle |
| 111512 | 11 / 11 | wool ×4 | none | well-balanced (cow/wheat/wool/wood) |
| 111704 | 11 / 11 | wood ×5 | none | furniture ×10 (wood-heavy) |
| 111801 | 7 / 9 | wood ×3 | none | balanced; 2 parcels idle |
| 111919 | 9 / 9 | honey ×4 | **honey ×4** | mead ×1 only — honey under-consumed |
| 112015 | 10 / 10 | honey ×6 | **honey ×12** | **all honey idle** — no wheat for mead |
| 112352 | 11 / 11 | honey ×6 | none | candles ×6, mead ×6 — good |
| 112613 | 11 / 11 | **wool ×11** | none | **total collapse to a single resource** |

## Performance / behaviour observations

### 1. Single-resource collapse (now fixed)
`112613` collapsed all 11 parcels to **wool**. Root cause (verified by
instrumenting the cap loop): the cap logic *banned* any over-supplied resource
from **every** parcel, the survivors absorbed the freed parcels and overshot in
turn, and the cascade continued until only the least-efficient chain (wool's
steep 3→2→1) survived. Partial versions of the same effect appear in `111106`
(6× iron_ore) and `112015` (6× honey).

**Status: fixed.** The cap loop now limits the *number of parcels per resource*
(floored at 1) instead of banning. Post-fix `114124` run shows all-plains
settlements resolving to a healthy **wheat ×5 / wool ×4 / cow ×3** mix every
seed. Guarded by the offline test `plains_parcels_stay_diverse`.

### 2. Availability-blind recipe selection broke the iron chain (now fixed)
The `111106` "iron without fuel" case turned out **not** to be co-input
starvation — coal *was* gathered (×2). The chain broke because recipe-variant
selection was availability-blind:

- `resolve()` is optimistic: it marks `tools` producible because the **coal**
  recipes (`iron_ore_coal_to_ingot`, `iron_ingot_to_tools_coal`) have all their
  inputs present. So `tools` entered the plan.
- But chain *construction* (`trace_resource` / `compute_recipe_runs`) and
  `raw_cost` always took `produced_by[id].first()` — the alphabetically-first
  recipe. For `iron_ingot` that is `iron_ore_char­coal_to_ingot` (charcoal sorts
  before coal), which needs **wood**.
- No wood was gathered, so the built chain's budget was
  `min(iron_ore, wood) = min(12, 0) = 0` → **0 tools**, and all 12 iron_ore + 2
  coal were stranded. The 2 coal could never help because no coal recipe was ever
  in the selected chain.

**Status: fixed.** Added `choose_producer(resource, producible)`, which picks the
first recipe whose inputs are all reachable from the current supply (falling back
to the first producer otherwise). `trace_resource`, `compute_recipe_runs`, and a
new availability-aware `chain_raw_cost` all use it, and `execute_chain` now
budgets against the chain's own `raw_cost` rather than the registry's canonical
first-recipe cost. With no wood, smelting now routes through the coal recipes, so
the mined iron/coal are actually consumed. The charcoal path is still preferred
when wood is present (it sorts first), so the common case is unchanged. Guarded by
`iron_chain_routes_through_coal_when_no_wood` and
`iron_chain_prefers_charcoal_when_wood_present`.

### 2b. Genuine co-input starvation — honey without wheat
Distinct from the above and still open: `honey → mead` also needs flour (wheat).

- **`112015` / `111919`: honey without wheat.** With no wheat assigned, mead ≈ 0
  and honey piles up (12 wasted in `112015`); only `beeswax → candles` consumes
  the apiary output.

Here there is genuinely no co-input anywhere in the settlement, so recipe choice
can't fix it. The resolver should bias *assignment* so gathered resources form
*completable* chains — when it picks a resource, value the presence of its chain's
co-inputs (wheat/flour for honey, etc.).

### 3. Coal is near-permanent dead weight (when wood is also present)
Two smelting recipes exist — `iron_ore_charcoal_to_ingot` (wood→charcoal) and
`iron_ore_coal_to_ingot`. When **both** coal and wood are available the planner
prefers the **charcoal** path (it sorts first and both are valid): every such run
(`110911`, `111155`) shows `charcoal_burner ×3–4` **and** `coal ×6` left over
(100% of mined coal idle). This is the *inverse* of finding #2: the availability
fix only changes the choice when the first recipe is *unsatisfiable*; when both
are satisfiable it still defaults to charcoal, so coal mined alongside wood is
still wasted effort and a wasted building slot. Fixing this needs a *preference*
(use already-mined coal) rather than a *feasibility* rule — see recommendation 2.

### 4. Idle rural parcels
`111355` produced from only **6 of 13** rural parcels; `111801` 7 of 9. Rural
districts sitting on resourceless biomes (river / ocean / beach — empty lists in
`biome_resources.yaml`) contribute nothing. Up to ~50% of rural land can be
unproductive, which also starves the chains above of inputs.

### 5. Cap is approximate, not strict
The new quota model approximates the caps rather than enforcing them. Post-fix
`bread` lands at **16** vs `FINISHED_GOOD_CAP = 15`, because the quota is derived
from a *trial* assignment (separate RNG stream) and the final assignment can land
a parcel or two differently. This is an acceptable trade for guaranteed
diversity, but worth noting.

## Recommendations (prioritised)

0. **DONE — availability-aware recipe selection.** Chain construction now picks
   recipe variants whose inputs are reachable from supply (`choose_producer`), so
   the broken iron chain in finding #2 is fixed.

1. **Make assignment co-input aware** *(highest impact, still open)*. Bias
   `assign_parcel_resources` so a resource scores higher when the settlement also
   has (or can have) the co-inputs its chain needs: wheat/flour for `honey→mead`,
   leather/paper for `book`, etc. This removes the remaining honey-without-wheat
   waste (#2b), the biggest source of dead buildings now that #2 is fixed.

2. **De-duplicate / prefer mined fuel (#3).** When coal has already been mined,
   prefer the coal smelting recipe so it isn't stranded — a *preference* layered
   on top of the feasibility rule from recommendation 0. Alternatively lower
   coal's gather value when wood is plentiful so the resolver doesn't gather both.

3. **Scale caps with settlement size.** `RAW_SURPLUS_CAP`, `INTERMEDIATE_CAP`,
   `FINISHED_GOOD_CAP` are absolute constants. For larger settlements they bind
   too early; for small ones they barely matter. Consider scaling by
   producing-parcel count.

4. **Surface waste as warnings.** Leftover raw goods (coal/iron/honey here)
   represent built gathering buildings producing nothing. Promote them from an
   `info` "leftover" line to a `warn` so they stand out across runs.

5. **Handle resourceless rural parcels.** Give water/beach biomes a fallback
   resource (e.g. fish) or fold resourceless rural cells into adjacent
   productive districts, to cut the ~30–50% idle rate.

6. **Optional: strict cap pass.** If exact cap adherence matters, add a final
   post-assignment trim that drops the marginal parcel when a good exceeds its
   cap (e.g. bread 16 → 14).

## Test observations

- **New guards added** (`src/generator/resource_chain/tests.rs`, all offline):
  - `plains_parcels_stay_diverse` — 5 seeds × 12 all-plains parcels, asserts
    wheat/wool/cow each get ≥1 parcel. Encodes the anti-collapse requirement (#1).
  - `iron_chain_routes_through_coal_when_no_wood` — asserts the iron→tools chain
    uses the coal smelting recipes (and a coal-based `raw_cost`) when no wood is
    available. Guards the finding-#2 fix.
  - `iron_chain_prefers_charcoal_when_wood_present` — confirms the common case
    (wood present) still routes through charcoal, i.e. the fix is non-disruptive.
- **Pre-existing failures unrelated to this work:** six `resource_chain` tests
  (`forest_has_near_miss_for_arrows`, `raw_cost_arrows`, `raw_cost_tools`,
  `forest_plus_mountains_unlocks_tools_chain`,
  `biome_forest_provides_wood_and_feathers`,
  `select_production_ranks_deeper_chains_first`) reference resources that no
  longer exist in the data (`feathers`, `flint`, `arrows`). They fail identically
  on a clean tree. They should be updated or removed — while broken they mask
  real regressions in the resolver.
- The offline pipeline guards (`build_furnished_houses_offline`,
  `pipeline_invariants_property_test`) are unaffected.

---

# Addendum — terrain steering & diversity (evening runs, 2026-06-14)

A second batch of ~13 runs (`184631`–`191410`) was captured after the
`flat_terrain` / `parcel_ruggedness` work landed. These add two new
`[resource-chain]` log lines per parcel: a `ruggedness parcel N -> r (roughness
.. / 4.0, gradient .. / 0.6)` line and a `terrain pick parcel … flat-terrain
penalties: …` line. **All logged runs in this batch use the *soft-penalty*
formulation** (a score penalty scaled by `ruggedness × flat_terrain_weight`); the
working tree has since switched to a *hard gate* (`terrain_allows`), so these logs
predate the current code — see the caveat at the end.

`flat_terrain` weights (from `resources.yaml`): **wheat = 1.0**, **sugar_cane =
1.0**, **wool = 0.7**, **cow = 0.7**; wood / iron_ore / coal / honey / beeswax =
0.0 (terrain-agnostic).

## Resource diversity (this batch)

Per-settlement distinct-resource counts (excluding the all-plains test run):

| Run | Parcels | Distinct | Dominant | Character |
|-----|---------|----------|----------|-----------|
| 185722 | 11 | 4 | honey | mixed |
| 185820 | 15 | 4 | wool ×7 | pasture-heavy |
| 190044 | 11 | 3 | wool ×7 | rough plains |
| 190345 | 10 | 3 | wool ×6 | rough plains |
| 190737 | 11 | **5** | wood ×4 | forest — most diverse |
| 190829 | 9 | **5** | wood ×3 | forest |
| 190933 | 7 | **5** | wood ×2 | forest |
| 191019 | 11 | 3 | **coal ×7** | mineral — least diverse |
| 191121 | 14 | 3 | **coal ×8** | mineral |
| 191410 | 10 | 3 | **coal ×7** | mineral |

- **No single-resource collapse anywhere** — the quota + diversity logic from the
  main report is holding; the worst dominant share is ~60–70% (coal on
  mineral-heavy maps), never 100%.
- Diversity tracks **biome variety**, not the resolver: forest-mix build areas hit
  5 distinct resources; mineral/mountain areas bottom out at 3 and lean heavily on
  coal — which dovetails with the "coal is dead weight" finding (#3): coal is both
  over-gathered *and* rarely consumed.

## How roughness / gradient affected resolving

`parcel_ruggedness = max(roughness / 4.0, gradient / 0.6)`, clamped to `[0,1]`.

1. **Roughness is the binding term, gradient is nearly inert.** Across 105 logged
   parcels, `roughness/4.0 > gradient/0.6` in **103 (98%)**; gradient drove
   ruggedness only **2%** of the time. The `/0.6` gradient normalisation almost
   never exceeds `roughness/4.0`, so in practice this is a *roughness* gate with
   gradient a rounding detail. If gradient is meant to matter, its divisor needs
   lowering.

2. **The penalty does steer crops off rough ground.** Bucketing 180 parcel picks
   by ruggedness:

   | Resource (weight) | low `<0.5` | mid `0.5–0.71` | high `>0.71` |
   |-------------------|-----------|----------------|--------------|
   | wheat (1.0)       | 17% | 21% | **8%** |
   | sugar_cane (1.0)  | 3%  | 0%  | **0%** |
   | wool (0.7)        | 28% | 32% | **34%** |
   | honey (0.0)       | 5%  | 11% | **29%** |

   As terrain roughens, the flat crops (wheat, sugar_cane) shrink and the
   pasture (wool) and terrain-agnostic forest resource (honey) grow — the intended
   behaviour. On an all-flat-terrain biome with no agnostic alternative (e.g. rough
   plains in `190044`: ruggedness-1.0 parcels 30/100/119 all chose wool, never
   wheat) the penalty cleanly shifts crop→pasture.

   *Confound:* high-ruggedness parcels also tend to be mountain/forest biomes that
   don't even offer wheat, so part of this shift is biome availability, not the
   penalty alone. The pure-penalty effect is only unambiguous on rough plains.

3. **The soft penalty is leaky — novelty overrides it.** The `+1000` novelty bonus
   for a not-yet-assigned resource outweighs the penalty, so crops still land on
   rough ground when they're the settlement's first of their kind:
   - `DistrictID(42)` at ruggedness **0.878** → wheat (score −297, penalty −1317)
   - `DistrictID(110)` at ruggedness **0.541** → wheat (score +201, penalty −812)

   So under the soft model a field can still be sited on terrain it was penalised
   for, purely for diversity.

## Caveat: logs predate the current hard gate

The working tree now uses `terrain_allows` — `ruggedness × weight ≤
FLAT_TERRAIN_RUGGEDNESS_LIMIT (0.5)` — which **removes** a resource from a
parcel's candidates rather than penalising it. Under that rule the novelty
overrides above would not happen (wheat/sugar_cane are excluded above ruggedness
0.5, wool/cow above ~0.71), and a parcel whose every candidate is terrain-excluded
is **skipped and left unproduced**. Two things to verify on the next batch:

- the crop→pasture shift should be *sharper* (no leaky novelty placements); and
- watch for an **increase in idle rural parcels** (finding #4) — rough plains-only
  districts can now have wheat/wool/cow all excluded and produce nothing.

## Quick reference — commands used

```bash
# all resource-chain reports
grep "resource-chain" output/logs/2026-06-14/*.log

# leftover (wasted) raw per run
grep "resource-chain" output/logs/2026-06-14/*.log | grep "leftover:" | grep -v "(none)"

# run the resolver regression guards (offline, no server)
cargo test --bin Tome plains_parcels_stay_diverse
cargo test --bin Tome iron_chain

# terrain steering: per-parcel ruggedness and the resulting pick
grep "resource-chain" output/logs/2026-06-14/*.log | grep -E "ruggedness parcel|terrain pick"

# is roughness or gradient the binding term? (expects ~98% roughness)
grep -h "ruggedness parcel" output/logs/2026-06-14/run_*_19*.log \
  | sed -E 's/^.*resource-chain\] *//' \
  | awk '{match($0,/roughness ([0-9.]+) \/ ([0-9.]+), gradient ([0-9.]+) \/ ([0-9.]+)/,m);
          if(m[1]==""){next} tot++; if(m[1]/m[2]>m[3]/m[4]) r++}
         END{printf "roughness binds %d/%d\n", r, tot}'
```
