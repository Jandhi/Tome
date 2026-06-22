# Ship Builder v2 — Interactive Co-Design

## Context

v1 (`src/generator/ships/`, `docs/plans/ship-builder.md`) is a fully-procedural
rib+plank ship generator — all phases marked complete — but the procedural hull
math left known realism gaps (pointy bow/stern instead of a rounded stem +
transom, unverified block facings, boxy sections). v2 takes a different *process*:
**the user supplies each geometry algorithm step by step**, and we verify each one
in-game via screenshots before moving on. v1 stays intact; v2 is a fresh module
built alongside it.

This is a thin scaffold + a tight build→screenshot→correct loop. The actual
hull/deck/rig algorithms get filled in interactively, one at a time, as the user
dictates them.

## Decisions

- **Approach:** interactive co-design — user drives each algorithm; I implement
  and we iterate from screenshots.
- **Ship classes / sizes:** v2 has a **minimum ship size — no rowboat**. Classes
  start larger than v1's smallest. (Keel test lengths span ~14–46.)
- **v1 fate:** keep intact, build v2 alongside.
- **Start point:** step by step, one algorithm at a time (user-led order).
- **Verify loop:** live `build_ship_v2` test the user runs against their Minecraft
  server, then pastes screenshots back. Offline ASCII as a fast pre-check.
- **Geometry:** fresh module, fresh geometry. Reuse only harness/placement/palettes.

## Module layout

New tree `src/generator/ships_v2/`, declared in `src/generator/mod.rs` next to the
existing `pub mod ships;`. Start minimal — grow modules only as the user
introduces the algorithm that needs them.

```
src/generator/ships_v2/
  mod.rs        # re-exports; ShipV2Ctx + build_ship_v2 entry
  blueprint.rs  # render_ascii diagnostic — added when there's geometry to draw
  test.rs       # offline build + live build_ship_v2 (the screenshot loop)
```

We deliberately do **not** pre-create hull/rig/deck submodules. They appear when
the corresponding algorithm does, so the tree always reflects what exists.

## What we reuse (do not re-implement)

Imported, not copied:

- `crate::generator::ships::{Placement, ShipDir}` (`src/generator/ships/mod.rs`)
  — local→world transform and ship-relative facings. Harness, not geometry, so v2
  imports them. (If the v1↔v2 import direction bites later, extract both into a
  shared `ships_common` module — defer until it does.)
- `World::synthetic_water(build_area, floor_y, water_y)` + `World::get_offline_editor`
  (`src/editor/world.rs`) — offline ocean flatworld.
- Live placement pattern (`GDMCHTTPProvider::new` → `World::new` → `Editor::new` →
  build → `editor.flush_buffer().await`), copied from
  `src/generator/ships/test.rs::build_cog`, incl. reading `waterline_y` from
  `get_motion_blocking_height_at(center)`.
- `data/palettes/ships/*.json` via `data.palettes.get(&"ship_oak".into())`.
- `Editor` block writes, `Block`, `BlockForm`, `geometry` (Point2D/3D, Cardinal),
  `noise::RNG` (`RNG::new(i64)`, `rng.derive()`).

## Initial scaffold (only code before algorithm #1)

A live test that compiles, runs end-to-end against the server, and places a
trivial unmistakable marker so we confirm the loop works before any real geometry:

1. `mod.rs`: `ShipV2Ctx<'a>` (mirror `ShipCtx`: `editor`, `data`, `palette`,
   `rng`) and `async fn build_ship_v2(ctx, heading: Cardinal, anchor: Point2D,
   waterline_y)` that places a short keel line of planks along local +x via a
   `Placement` (confirms orientation/anchor/waterline in-game).
2. `test.rs`:
   - `build_ship_v2_offline` — `synthetic_water`, `get_offline_editor`, build,
     assert the marker landed, write `output/ships_v2/ship.txt` when there's ASCII.
   - `build_ship_v2` — live, mirrors `build_cog`: anchors at build-area centre,
     reads waterline from heightmap, builds, flushes. The screenshot command.
3. Register `pub mod ships_v2;` in `src/generator/mod.rs`.

## The iteration loop (working agreement)

Per algorithm the user hands me:

1. User describes the algorithm (e.g. "keel = slab line at y=0 from x=0..L";
   "each station's half-width = f(x)").
2. I implement it in the smallest fitting `ships_v2` module, keeping all math in
   the local frame (`x`=stern→bow, `z`=±beam, `y`=up from keel), transformed by
   `Placement` — never bake world coords or hardcode facings (use `ShipDir` +
   `placement.world_cardinal`).
3. I run the offline test as a compile + sanity pre-check, dumping ASCII when
   useful.
4. User runs the live test and pastes screenshots:
   `cargo test build_ship_v2 -- --nocapture` (build area set over water with
   `/setbuildarea`).
5. We correct from the screenshot; repeat until the stage looks right, then next
   algorithm.

## Conventions to hold (from v1's hard-won notes)

- Local frame only; transform via `Placement`. Bow = local +x.
- Stair/trapdoor/ladder/slab facings: always derive from a `ShipDir` via
  `placement.world_cardinal(dir)`. Never hardcode a `Cardinal`.
- `RNG::new` takes `i64`; use `rng.derive()` per placer.
- Watertightness is a property of the shell algorithm — keep a sealed inboard face.
- NBT reference ships in `data/structures/ship/` are **visual reference only** and
  use the plural `palettes` key (won't parse via `NBTStructure`). Not placed by code.

## Verification

- **Offline (fast, every change):** `cargo test ships_v2:: -- --skip _live`
  runs the property + offline-editor builds and asserts; `cargo check` for
  compile-only. (Offline tests write `output/ships_v2/*.txt`.)
- **Live (screenshot, per stage):** user runs `cargo test build_ship_v2_live --
  --nocapture` against the GDMC server with a water build area.
- v1 untouched — its tests must keep passing as a regression check that the new
  module didn't disturb shared reuse.

## Out of scope for the scaffold

No hull shapes, rig, deck, fittings, interiors, size classes, or invariants yet —
each arrives only with the user's algorithm for it.

---

## Algorithms (user-supplied, step by step)

> Add each algorithm below as we go. I implement them in order, one screenshot
> pass each. Rough notes are fine — sketch the math/intent and I'll turn it into
> `ships_v2` code.

### Stage 2 — Deck & above-water

1. **Initial deck** — **implemented** (`deck.rs`). Covers the hull's open top (the
   hollow at the waterline, `y = depth`) with **top slabs** — the floor that
   further superstructure is built on. Deck cells = the hull's top-layer interior
   cells (`HullModel.interior` filtered to `deck_y`). Own `Deck` palette component
   (defaults to the same wood). Not waterlogged (at/above the surface).

2. **Deck additions (size-gated)** — framework in `additions.rs` (`SizeTier`,
   `DeckAddition`, gating). Each addition is **its own submodule** under
   `additions/` (e.g. `additions/gallery.rs`) — they're complex. `ShipV2Output.tier`
   carries the derived tier.

   **Size tiers** (from length, tunable): Small ≤20 · Medium 21–30 · Large 31–40 ·
   Huge 41+. `mast_count`: 1 / 2 / 3 / 3. `extra_decks`: 0 / 1 / 1 / 2.

   **Catalog + gating** (`SizeTier::has`):
   | Addition | Gate |
   |---|---|
   | MainRailing (required) | all |
   | Masts + sails (required) | all (count scales) |
   | Bowsprit (required) | Medium+ |
   | AdditionalDeck (gun deck, windows/gun ports) | Medium+ |
   | CargoHatch (+ stairs below) | Medium+ |
   | HelmCapstan | Medium+ |
   | Forecastle (bow box structure) | Large+ |
   | Gallery (stern box structure) | Large+ |
   | Cabin / deckhouse | Large+ |

   **Gallery tips (user):** a "box" of **upside-down stairs**, **5 long**, width =
   **hull beam − 2** (e.g. beam 11 → 9 wide). Window holes in a **secondary colour**;
   optional back wall + door to close it. Stairs/slabs decoration on top; windows,
   torches, final detail. (No single correct design — varies.)

   **Additional deck — implemented** (`additions/additional_deck.rs`). Walls rise
   from the main deck following the hull's waterline outline (`HullModel.top_half`),
   curving **inward (tumblehome)** as they climb (gentle, scales with height); **gun
   ports** every ~2 blocks on the sides, each **randomly a trapdoor lid or an open
   hole**; a new deck floor (top slabs) caps the level. Level **height is a fn input**
   (`level_height(tier)`, const 4 for now, later size-scaled). Bowsprit support
   deferred to the bowsprit step. Built for all ships for now (size gating deferred);
   stacking multiple levels (Huge) is a follow-up.

   Tumblehome curves **inward at the very top** (cubic), stays **near-vertical at
   the stern** (aft ramp), and the step is **stair-bevelled on both faces** (inside
   upside-down + outside bottom). Stern is a **mix**: a small transom (`stern_min`
   ramps 2→0 over the aft ~10%) blended into the natural stern taper, so the back is
   clean and stacked levels align. Gun ports sit 2 above the deck floor. Levels
   **stack** (`num_levels`: Large+ may get 2) with **varied heights** (random 3–5),
   each level's inset top becoming the next level's base — rng-driven variation.

   _Above-water detailing TODO:_ rail/trim, windows, more refined topsides — per the
   "more detail above water" principle below.

   **Bowsprit tips (user):** length ≈ **a little less than half the hull length**
   (hull ~35 → bowsprit ~15). The mast that pokes out the front; sits on the
   bowsprit support; figurehead/decoration later.

   **Mast tips (user):** masts run down to the **very bottom of the ship**
   (keel-stepped) and stand **as tall as the hull length** (hull ~35 → main mast
   ~35 from the bottom). Secondary mast(s) a few blocks shorter. All masts **lean
   forward slightly** (helps sails/nests/rigging).

   **Spar tips (user):** add **spars** (the cross-pieces holding the sails),
   **angled slightly** for style. Add a spar at the **back** for the
   ~triangular rear (spanker) sail. Add the small **platforms** (crow's nests) on
   the masts.

   **Sail tips (user):** treat the wind as coming from a **slight angle** (not
   axis-aligned) for an organic look — rotate the **yards** to a consistent
   direction (max ~**45°** for square rig; keep a fleet's sails parallel). Sail
   shape depends on size / wind / yard angle; build each sail **separately** (not to
   one fixed plan) and tweak. **Courses** (bottom sails) **draw in at the bottom**
   (bottom corners secured to the hull); **topsails/topgallants/royals** are
   slightly **wider at the bottom** than the top (secured to the yard below). (The
   copy-paste-with-water building trick is workflow-only — N/A for generation.)

   **More detail above water (user principle):** above-water features need finer
   detail than the mostly-blocks/large-shapes underwater work — use stairs/slabs,
   trim, secondary colours, windows. Example idea: **side ladders as climbing ropes**
   for boarding.

   **Modularity:** each addition is a submodule `additions/<name>.rs` exposing
   `build(ctx, dc: &DeckContext)`; `additions::BUILD_ORDER` + `build_addition`
   dispatch run them. Adding one = new file + `pub mod` + match arm.

### Stage 1 — Underwater portion

The first stage builds the underwater portion of the boat, largely consisting of
the **keel**, **hull** (below-waterline shell), and **rudder**.

**Stage 1 rule:** any **stairs or slabs** placed as part of the keel, rudder, or
underwater hull must be **waterlogged** (`waterlogged=true` blockstate) **when built
on water** so they sit flush instead of trapping air pockets. **On land, nothing is
waterlogged** (threaded via `on_water` into `place_keel`/`cell_state`).

**Stage 1 design principle — palette-driven blocks:** as much as possible, the
block each step places should be looked up from a **palette role** (a named
ship-part → block mapping) rather than hardcoded, so the materials can be reassigned
/ edited later without touching the shape code. For now we wire it to `ship_oak`;
**the priority right now is getting the shape of each step correct**, with palette
roles as the seam that keeps materials swappable later.

**Stage 1 rule — land vs water footing:** the ship's vertical anchoring adapts to
the terrain at the anchor:
- **On water:** build the below-water portion below the surface as applicable — the
  keel's flat bottom sits `depth` below the water surface, structure rises to the
  waterline and above (current behaviour).
- **On land:** build **everything above the ground** — the keel's flat bottom rests
  on the ground surface, nothing buried.

**Resolved & implemented** (`mod.rs::build_ship_v2`): the generator **auto-detects**
from the world at the anchor — `world.is_water(anchor)`. Water footing = keel
bottom at `motion_blocking_height - depth`, clamped to `ocean_floor_height` (rests
on the seabed in shallow water = "as applicable"). Land footing = keel bottom at
`get_height_at` (ground surface). `ShipV2Output.on_water` reports which was used.

Sub-algorithms (to be detailed):

1. **Keel** — the backbone spine, and the parameter that **sets the ship's
   length**. (Reference: `1.png`.)

   Shape, from the reference image:
   - A **1-wide thin spine** along the centerline (local `z = 0`), running
     bow↔stern along x. _(Assumed thin spine per the image — confirm on first
     screenshot, not a deep solid beam.)_
   - **Longitudinal profile** (total length is **tip-to-tip**, stern post → bow tip):
     - **Stern (back, local origin, x=0):** a near-vertical **post**, rising from
       the flat bottom up to ~the waterline. The **rudder attaches here**.
     - **Flat bottom run:** the **majority** of the length, a single bottom course
       sitting at the lowest point (`depth` below water).
     - **Bow (front, +x):** the front **~15–20% of the length** is the **bow
       rake** — an upward **curve** from the flat bottom to ~the waterline at the
       bow tip, shaped with **full blocks + stairs + slabs**.
   - Block forms: the **flat run** is the single bottom course (top slabs, see
     below); the **bow rake** is a curve of blocks/stairs/slabs; the **stern post**
     is full blocks.

   Depth & materials:
   - **Keel depth (underwater height) is proportional to length.** A ~30-long keel
     sits ~5–6 blocks underwater; the smallest size only 1–2 blocks. (Roughly
     `depth ≈ length / ~5.5`, clamped to a 1–2 minimum.)
   - The keel is **mostly fully submerged**: the flat bottom sits `depth` below the
     waterline, and the bow/stern ends rise up to **about the waterline** — poking
     up to ~1 block **above** water only on the larger ships.
   - The **bottommost course of the keel is always a top slab** (slab in the top
     half of its cell), running the length — gives the keel a recessed, tapered
     underside.
   - **Largely full blocks**, with **stairs and slabs used for smoothing**
     transitions (the rakes and any curvature).

   Input & derivation:
   - **`length` is a passed-in parameter** (chosen per ship class upstream). The
     keel routine consumes it and derives a good keel of that length — depth, bow
     rake (~15–20% of length), stern post height (≈ depth), and the flat run
     (the remaining majority) are all scaled proportionally from `length`. I'll
     pick first-pass proportions and we tune them from the screenshot.

   Blocks: **`ship_oak` palette** (the default ship palette). Logs/planks for the
   flat run / post, matching stairs + slabs (waterlogged) for the bow rake curve
   and the bottom course.

   _All parameters resolved — ready to implement._

   **Status: implemented** (`src/generator/ships_v2/keel.rs`, palette seam in
   `palette.rs`, build entry `mod.rs::build_ship_v2`). Offline + property tests
   green. Final shape (per the user's sketch):
   - **Stern = straight vertical post + a couple of small base steps** (size-scaled,
     `stern_steps = (depth/2).clamp(1,3)`). Not a rake.
   - **Bow = a real parabolic stem curve** `y = depth · t²` (`BOW_CURVE_POW`),
     sampled per block-column and approximated: top slab (gentle) → upside-down
     stair (slope ≈ 1) → full block (steep, near the stem). Small classes degrade
     to a plain stair staircase (acceptable approximation).
   - Bottom edge is a continuous **top-slab** line (incl. the stern tip).
   - All step/curve stairs are **upside-down (top-half)** → solid keel top, curve on
     the underside. Waterlogged on water only; never on land.

   **Verify on the live screenshot:**
   - bow **stair facing/half** (the known MC flip point — `BOW_RAKE_STAIR_FACE` /
     `top_half` in `keel.rs`);
   - bow reads as an **actual curve** (tune `BOW_CURVE_POW` / the `0.18` length
     fraction);
   - stern reads **straight** with a clean couple-of-steps base;
   - **proportional depth** across sizes; **waterlogging** correct on water, absent
     on land.
2. **Hull (shell)** — built **upon the keel**, layer by layer. Each layer (a Y
   level) is a **stretched-teardrop-shaped outline** in the X–Z plane — only the
   **perimeter** is placed (interior left as air), so the hull is a hollow shell.
   The teardrop changes as layers rise (flare). Reference technique: the piratemc
   30-gun frigate tutorial (screenshots).

   **Blocks only for now** — slab/stair smoothing of the shell comes in a later
   pass. Palette: one `Hull` component role (per the per-component palette rule).

   From the piratemc tutorial:
   - Stretched teardrop, built in horizontal layers **from the keel up to the
     waterline**, **perimeter only** (interior air; deck/fill later).
   - Layer beam **expands** going up to a **max beam near the waterline**, then
     tumblehome above (above-water = a later stage).
   - Ref dims: length ~30, **max beam ~11** (≈ length/2.7); **stern rounded**, bow
     has tumblehome.
   - Tutorial says "pointy end at the back" — conflicts with our keel (fine bow,
     blunt stern); orientation resolved by the user below.

   **Implemented** (`hull.rs`):
   - Two plan shapes via `HullShape` (`ShipV2Context::with_hull_shape`):
     **Teardrop** (asymmetric — fuller round bow, blunter/wider stern, widest ~⅓
     fwd; taper via `STERN_TAPER`/`BOW_TAPER`) and **Oval** (symmetric ellipse, both
     ends rounded, widest amidships).
   - Max beam ≈ `length / beam_ratio` — `beam_ratio` is an input param
     (`with_beam_ratio`, default `DEFAULT_BEAM_RATIO = 2.7`).
   - Vertical flare: beam 0 at the keel → max at the waterline via a rounded-bilge
     power curve (`HULL_BILGE_POW`).
   - Shell = **boundary of the 3D hull volume** (sides + underside exposed → placed;
     **top left open** = hollow). This seals the flare-ledge undersides (the earlier
     holes). Full blocks only; slab/stair smoothing later.
   - **Interior cleared to air on water:** the hollow interior cells (`HullModel.
     interior`) are set to air so the hull stays dry (skipped on land — already air).
     (Full dryness up to the rim awaits the above-water hull/bulwarks stage.)
   - **Respects the keel:** the hull volume stays strictly above the keel's **crest**
     (`KeelModel::top_profile()` — highest keel cell per station — fed to
     `build_hull_model`), so the keel protrudes below and its underside touches
     water. The hull floor conforms to the **bow rocker** *and* the solid **stern
     step-up** (sits on top of the steps rather than letting them poke through).
     Enforced by an offline invariant.
   - Live test alternates Teardrop/Oval across the fleet; offline writes both
     `hull.txt` and `hull_oval.txt` plan diagnostics.
3. **Rudder** — **implemented** (`rudder.rs`). A **solid raked fin** (filled X–Y,
   1 thick): vertical leading edge just aft of the post, **raked trailing edge**
   (bottom further aft, `RUDDER_RAKE`) smoothed with **stairs**, from the keel
   bottom up to the waterline. A **vertical line of fences** (1-block gap) connects
   the whole sternpost to the fin. Underwater cells waterlogged on water; the top
   not. Own `Rudder` palette component. Stair facing/half (`RUDDER_STAIR_FACE`/
   `RUDDER_STAIR_TOP`) are screenshot-flip candidates.
