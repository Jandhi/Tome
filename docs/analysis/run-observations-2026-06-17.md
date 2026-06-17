# Run observations — 2026-06-17 (run_20260617_151600)

Analysis of the latest generation run (`output/logs/2026-06-17/run_20260617_151600.log`,
25,120 lines) plus the two in-game screenshots. Settlement sits on a grassland
plateau at ~X1936 Z-2139, right on the western edge of a large Badlands biome,
ocean to the west.

## Headline numbers

- **415 WARN, 12 ERROR.**
- **412 of the 415 warnings are a single repeated message** (`No block found for
  material white_wool with form Stairs`) — see #3. The other 3 warnings are the
  real placement failures.
- **Rural buildings: "Placed 6 of 8".** Two production buildings were
  hard-rejected on terrain: `ranch` in `DistrictID(3)` and `farm` in
  `DistrictID(98)`.
- **Urban: `bakery` found no viable placement** ("No viable urban placement for
  'bakery'").

## 1. Terrain rejections silently break the resource chain  ⭐ (the wheat-farm issue)

The resource-chain planner runs **first** and logs a balanced economy
(`registry`, ~line 4172 onward):

```
settlement production: 8 producing parcels of 13 total
parcel 80 -> wheat [farm]
parcel 98 -> wheat [farm]
parcel  3 -> cow  [ranch]
parcel 105 -> cow [ranch]
...
supply: wheat x4
supply: cow x4
finished_good: bread x6
finished_good: beef x5
```

Then placement runs and **drops 2 of those 8** because the chosen super-parcel
has no footprint flat enough (`MAX_PLACEMENT_SLOPE = 4`):

```
WARN No viable placement for 'ranch' in super-parcel DistrictID(3)
WARN No viable placement for 'farm'  in super-parcel DistrictID(98)
INFO Placed 6 of 8 rural buildings
```

Net effect: **only 1 wheat farm (80) and 1 ranch (105) actually got built**, but
the chronicle/economy still believes there are two of each (`wheat x4`,
`bread x6`, `beef x5`). The plan and the world disagree.

**Why it happens:** the planner picks resources from *ruggedness* (a parcel-wide
roughness/gradient score) but placement rejects on the *footprint's local slope*.
Parcel 98 had ruggedness 0.443 — passable as a parcel — yet no 0..4 height-range
pad existed for the farm footprint. There's no feedback loop between the two
stages.

**Suggested improvements (in rough priority):**
- **Reconcile the plan after placement.** When `place_rural_building` returns
  `Ok(false)`, either (a) retry the same resource on one of the *dropped*
  same-resource candidates (see #2), or (b) decrement the supply/finished-good
  tallies so the chronicle reflects reality.
- **Pre-validate feasibility before locking the plan.** Cheaply test "does any
  footprint-sized window in this district fit under `MAX_PLACEMENT_SLOPE`?" during
  resource assignment, and skip/swap parcels that can't seat their building.
- Both of these keep the economy honest instead of advertising bread the town
  can't make.

## 2. The "2 per resource" competition cap throws away flatter backups

In the same registry block, several parcels are dropped purely by the per-resource
cap:

```
competition cap: dropped DistrictID(129) (resource wheat) — over 2 per resource
competition cap: dropped DistrictID(219) (resource cow)   — over 2 per resource
competition cap: dropped DistrictID(230) (resource wool)  — over 2 per resource
...
```

Note **wheat parcel 129 had ruggedness 0.408 — flatter than the kept parcel 98
(0.443)** that later failed placement. We discarded a flatter wheat site and kept
a rougher one that couldn't be built. Same story is plausible for cow (dropped
219 @ 0.375 was the flattest cow parcel of all, capped out in favor of 3 @ 0.538
which then failed).

**Suggestion:** keep the capped-out parcels as an ordered fallback list per
resource. When a primary pick fails placement, promote the best dropped
same-resource candidate instead of just losing the building. This directly
recovers both rural failures above.

## 3. `white_wool` + `Stairs` — 412 spurious warnings (99% of all warnings)

```
WARN [materials::material] No block found for material white_wool with form Stairs   ×412
```

Wool has no stairs block in Minecraft, so some palette / wall / roof component is
requesting `form = Stairs` against a `white_wool` material and silently getting
nothing back 412 times. This is almost certainly **missing geometry in the
output** (wherever those stairs should be, there's a hole) and it's drowning the
log so real warnings are hard to find.

**Suggestion:** find the component/palette mapping `white_wool` to a `Stairs`
form (likely a roof or trim set used by one of the placed buildings) and either
point it at a material that *has* stairs, or fall back to the wool full block /
slab. Worth fixing first purely for log signal-to-noise.

## 4. 12 `world block is None` placement errors

```
ERROR [editor] Failed to place block minecraft:dirt at Point3D { x:1686, y:63, z:-2097 }, world block is None
... (12×, dirt/stone, y 63–76)
```

These are terraforming/foundation fills landing on columns the `World` has no
cached chunk for — likely foundation/blend cells just outside the loaded build
area or below a parcel edge. Low count, but each is a block that didn't get
placed (small cliffs/holes under building skirts). Worth guarding: skip-with-
debug rather than error, or ensure the blend ring stays inside the loaded region.

## 5. Screenshot observations

- **Stray house outside the north wall** (top-center of the aerial shot) is
  disconnected from the town with no road to it. Looks orphaned. Likely an urban
  building placed in a district lobe that ended up outside the regularized wall,
  or a rural building with no path connection. Worth a "connect or cull isolated
  buildings" pass.
- **Production buildings sit well outside the walls with no connecting paths.**
  The sandstone wheat farm at the bottom-right of both shots, and the cattle out
  east, read as detached from the settlement. Rural buildings could use a short
  spur path back to the nearest gate/road.
- **The curtain wall dominates the silhouette** — it's thick and tall relative to
  the buildings, and its star/lobed shape (following district boundaries) snakes a
  lot. Visually impressive but it's the loudest element. Consider thinner walls or
  smoothing the most concave boundary runs.
- **No biome adaptation at the Badlands border.** The town butts directly against
  Badlands (orange) yet everything is oak/spruce medieval. A material or palette
  nudge toward terracotta/red sandstone on the eastern, badlands-facing parcels
  would tie it to the landscape.
- **Marginal placements.** `shepherds_hut@62` and `farm@80` both placed at
  `score 0.00` — accepted but at the floor of viability, i.e. the only pad that
  fit. Consistent with the terrain being tight for rural footprints here; reinforces
  #1/#2.

## Quick-win ordering

1. Fix the `white_wool`/`Stairs` mapping (#3) — kills 99% of warning noise and a
   real visual hole, low effort.
2. Add same-resource placement fallback from the capped/dropped pool (#2) — likely
   recovers both rural failures, keeps the economy balanced.
3. Reconcile economy tallies (or pre-validate) so the chronicle stops advertising
   buildings that weren't placed (#1).
4. Connect or cull isolated outside-the-wall buildings (#5).
