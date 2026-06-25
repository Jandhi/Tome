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
   upside-down + outside bottom). Stern is a **mix**: a small transom (`stern_min`)
   blended into the natural stern taper. Because `stern_min` *falls* toward the bow
   while the hull taper *rises*, their `max` used to carve a V-shaped **pinch** in the
   stern outline (the deck/walls/railing then followed the notch → weird stepped
   terraces); the stern side (`x ≤ peak_x`) is now forced **non-decreasing** via a
   cumulative max, so the transom blends into the hull as a clean flat-ish wall and
   stacked levels align. Gun ports sit 2 above the deck floor. Levels
   **stack** (`num_levels`: Large+ may get 2) with **varied heights** (random 3–5),
   each level's inset top becoming the next level's base — rng-driven variation.

   _Above-water detailing TODO:_ rail/trim, windows, more refined topsides — per the
   "more detail above water" principle below.

   **Main railing — implemented** (`additions/railing.rs`). A short solid **bulwark**
   course (`BULWARK_HEIGHT`, 1 for now) capped with a **fence rail** around the
   **topmost open weather deck**. To sit on whatever the structural additions raised
   (not the raw main deck), the additions pipeline now threads a mutable
   `additions::DeckState { top_outline, top_y, railing }` — initialised to the main
   deck, updated by the additional deck(s) to their inset top outline + floor Y, and
   read by the railing. `ShipV2Output.railing` carries the built `RailingModel`. So
   once size gating is on, a Small ship (no additional deck) gets the rail on its main
   deck; larger ships get it around the raised top deck — no manual coordination.

   _Verify on the live screenshot:_ rail follows the top-deck edge cleanly; bulwark
   height reads right (tune `BULWARK_HEIGHT`); no gaps/double-walls where it meets the
   additional-deck wall top.

   **Bowsprit tips (user):** length ≈ **a little less than half the hull length**
   (hull ~35 → bowsprit ~15). The mast that pokes out the front; sits on the
   bowsprit support; figurehead/decoration later.

   **Bowsprit — Approach B (`additions/bowsprit.rs`).** Reverse-engineered from the
   user's hand-fixed NBTs (`analyze_bowsprit_nbt`, ignored test): the prow is **the hull
   continued forward** — a flared cross-section (rounded bilge, `PROW_FLARE_POW`)
   tapering in plan to a **stem point** and in section to a **keel point**, decked +
   railed on top, **solid for Small ships / a hollow shell** (with a deck-floor slab)
   for larger. It rebuilds the forward `~0.30·length` of the bow (blending from
   `hull_top_half` at `x0`) and runs out `ext` past the bow to the stem, with the keel
   point sweeping up from the keel crest (`keel_top`) to the deck. The spar projects on
   from the stem (`reach_factor`-shortened by rake). Threaded `keel` into `DeckContext`
   for the crest. **Shell smoothing** added: the flaring bilge outer edge and the deck
   rim are beveled with inboard-facing upside-down stairs, and the deck surface is top
   slabs (matching the ship deck) — the `//---//` look from the NBT. Stair facings are
   stored per cell (`stair_at`), decoupled from the spar's flip. Checkpoint commit
   `c8bed1b` precedes this; if the hull seam looks bad, revert and move to Approach A
   (fold into `hull.rs`). Underside steps are smoothed with upside-down stairs (`prow_bevel`); the
   prow-top edges get `rail` fences. The centerline (z=0) spar projects on from the prow
   point (`REACH_FRACTION = 0.4·length`, **shortened by `rake.reach_factor()`** so steep
   rakes don't climb away). Rake is still `BowspritRake::pick` (all four with a deck). **Smoothed:** the spar (z=0) is tracked at **half-block
   resolution** (`ramp`) — per column a slab (flat), a single stair on a half-block step
   (upside-down in the cell's upper half, right-side-up in the lower), or, on a
   **full-block step**, a **double-stair wedge** (`push_step`): an upside-down stair on
   the lower cell + a right-side-up stair on the upper, facings flipped opposite so the
   two bevels meet into one continuous diagonal (stairs "on both sides"). Top/bottom
   slabs across columns give the shallow "two slabs" ramp. The knee is the same 45°
   wedge brace from the bow to the spar underside. `RAKE_STAIR_FACE` is the stair-facing
   flip candidate (upside-down stairs auto-face its opposite). This needs
   stair/slab variants, so the `Spar` palette role is **plank wood** (`PrimaryWood`),
   not a log. **Two roots converge** (per the user): primary at the **hull bow tip /
   stem**; with a raised deck, a secondary at the **deck bow**, the knee rising from
   the bow tip to meet the forward spar.
   **Rake** (`BowspritRake` enum: Straight/Gentle/Medium/Steep) is **chosen by
   `BowspritRake::pick(has_deck, rng)`**: a raised deck gives a high anchor so **all
   four** are allowed (Straight = horizontal at `top_y + DECK_CLEARANCE`, clearing the
   bow rail); without a deck, Straight is excluded and we pick among the **angled**
   rakes (rakes up from the low stem). Randomised for now — _later a higher-level
   system may decide which parts/rake go together per ship_ (so the selection lives in
   one `pick` fn, separate from the pure `build_bowsprit_model`). `BowspritModel`
   (spar + knee + tip) is recorded on `ShipV2Output.bowsprit`. Built for all ships for
   now (Medium+ gating deferred, like the additional deck).

   _Verify on the live screenshot:_ spar clears the bow rail and reads as a beam; knee
   converges cleanly (tune the `k` offset / `DECK_CLEARANCE`); forward reach length
   looks right (tune `REACH_FRACTION`); log **axis** is along the spar (heading-derived
   x/z) not stacked; on a deckless/Small ship the Medium up-rake reads right.

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

   **Square sails (deployed / `SailState::Full`) — implemented** (`additions/masts.rs`).
   A billowing **white-wool** sheet hangs from each main yard: **head** pinned just under
   its own yard ("bottom of the stays"), **foot** just above the next yard down — or, for
   the lowest sail, on `mast.sail_foot_base`: **2–3 blocks of open air above the deck
   *railing*** (`SAIL_FOOT_CLEARANCE`, +1 for a wide course). The clearance is measured above
   the rail, not the bare deck — the rail (`BULWARK_HEIGHT` + fence) is added at build time —
   so the course no longer lands on the bulwark. The sheet **bellies forward** (toward the
   bow, into a following wind): one wool block per `(z, y)` at `x = yard.x + bulge`, from the
   bulge field `billow_field`, which is **relaxed to a 1-block gradient** so neighbours never
   step >1 → a single **hole-free** surface (asserted by `square_sail_surface_has_no_holes`).
   Depth = **wind strength**, configurable via `ShipV2Spec::with_wind` (default `SAIL_WIND`);
   `SAIL_BILLOW_DIR` the direction (flip candidate). `ShipV2Spec::new` defaults to `Full`.

   Three **billow shapes** (`SailBillow`, `ShipV2Spec::with_sail_billow`; the live test
   cycles them so all show in one shot):
   - **`Domed`** (attempt 1) — curve runs **across the width**: a parabola in `z` (deepest at
     centre, pinned at the luff edges) × a `sin` down the height; the vertical **sides stay
     straight** at the yard. A pillow. `SAIL_BELLY_POW` tunes fullness.
   - **`Curtain`** (attempt 2, current default) — each row is **flat** (all blocks at one
     `x`), so the curve runs **down the whole length and the sides curve too**. A 1-D profile
     (`sin^SAIL_CURTAIN_CURVE_POW`, `p<1`) bulges **more drastically at the head/foot and
     flattens through the middle**, broadcast across the width; **larger sails curve deeper**
     (`SAIL_CURTAIN_SIZE_GAIN`). The no-holes relaxation caps the end-sweep at ~45°.
   - **`Combined`** (attempt 3) — a blend: the domed `sin`×parabola belly (centre-weighted),
     but the across-width factor only drops to `SAIL_COMBINED_EDGE` at the luff edges (not 0)
     **and those edges are left free** in the relax (curtain-style), so the **sides billow
     partway** instead of pinning flat. Fuller/rounder than `Domed`, more centre-weighted than
     `Curtain`. `SAIL_COMBINED_EDGE` = 0 → `Domed`, = 1 → `Curtain`.

   (Course-foot draw-in is a later pass.)

   **Spanker (deployed) — implemented** (`additions/masts.rs`, `spanker_billow`). The aft
   fore-and-aft sail (**wool**) is **bent to the gaff (head) and mast (luff)** with the
   **leech** the free aft edge — `build_spanker` records the flat (`z = 0`) `sail`
   quadrilateral. Under `Full` it bellies **sideways to leeward**, the side (`±z`) **rolled
   randomly per ship** for now (a real wind input would decide it later — that's the single
   spot to change): each cell offset to `z = depth · side`. `spanker_billow` pins the **head,
   luff and leech to 0** (so the canvas tapers onto the spars — the row under the gaff lands at
   `z = 0`, a **wool block bent onto the upside-down gaff stair**), but leaves the **foot (boom)
   free except at its two corners** (tack + clew, held by the luff/leech) so the foot **billows
   off the boom** for a less boxy belly. To exaggerate the wind, the **foot also arcs upward**
   in the centre (`SAIL_SPANKER_FOOT_LIFT`, 0 at the corners) — the bottom edge lifts off the
   boom (which stays visible below). The **leech** runs straight from the gaff tip to the boom
   tip, with at least one canvas row kept above the boom along its **full length**
   (`leech_top.max(boom_y+1)`) so the boom's aft end (clew) is **under sail, not a bare spar
   tip**. The interior starts at `dc.wind · SAIL_SPANKER_WIND_FACTOR`
   (spankers get large, so they billow deeper) and is **relaxed hole-free** (1-block gradient,
   foot underside excluded from pinning via the per-column `foot_cells`). Boom/gaff spar cells
   skipped. `Furled` keeps the stowed roll on the boom. Asserted by `spanker_sail_has_no_holes`.
   Rolled per ship by `MAST_SPANKER_CHANCE` (50%).

   **Jib (triangular headsail) — implemented** (`additions/masts.rs`: `triangle_xy`, `line_xy`,
   `jib_billow`). Set between three points: the **bowsprit start (A)**, **bowsprit tip (B)** and
   **foremast top (C)** (foremost mast = max `base_x`; A = the spar's inboard cell; B =
   `bowsprit.tip`). The filled triangle bellies **sideways to leeward** (same `spanker_side` as
   the spanker), relaxed hole-free (asserted by `jib_sail_has_no_holes`). **What's pinned at `z = 0`
   (over the bowsprit centreline) vs. free to billow:**
   - The **luff (B→C)** — the sail's **head/top edge**, which runs along the bowsprit-tip→first-mast
     **forestay**. It's pinned, so the sail's top lands at `z = 0` **on that stay line**, over the
     bowsprit, exactly as it would be tied to the forestay. The canvas (wool) *is* placed along it
     (it's no longer a bare rigging line — the sail covers the stay), so the head reads as sail, not
     a thin chain.
   - The **foot (A→B)** touches the bowsprit at **only `JIB_FOOT_HANGER_COUNT` (2) anchor columns**
     — the two ends (forward tack at the tip, clew inboard). Each anchor drapes a wool corner on the
     centreline and drops a **hanger tie** (chain/fence) to the spar top (`SAIL_JIB_FOOT_RAISE` = 3
     of clearance). The **rest of the foot is free**, so it billows **up off** the bowsprit instead
     of lying along its whole length (that earlier "row of feet" is gone).
   - The **leech (A→C)** and the interior are free → billow out to `z = d·spanker_side`.
   - **Curved outline (not a bare triangle):** the **foot (A→B)** and **leech (A→C)** edges bow
     gently **inward** (a hollow, `JIB_CURVE_FRAC` of edge length, capped `JIB_CURVE_MAX`) via
     `curved_sail_xy`, so the sail reads as cloth. The **luff (B→C)** — sail head → forward bowsprit
     tip, on the forestay — is the **only straight edge**. Built as the straight triangle **minus two
     `edge_bite`s** (each a simple, non-self-intersecting Bézier sliver) — an outline polygon would
     **self-intersect** where the two inward curves meet at the shared corner and even-odd fill would
     drop a whole sail section (the bug that left holes). `jib_sail_has_no_holes` asserts no interior
     holes on the curved outline.
   - **Head rigging bridge:** the sail's head corner stops **`JIB_HEAD_RIGGING` (4) blocks below the
     masthead** (`c_sail`, not the masthead `c_mast`); the gap from there up to the head is **pure
     forestay rigging** (chain/fence, no canvas), so the jib ties to the mast with rigging instead of
     ramming solid sail into the masthead. The bridge **starts one block higher than the masthead**
     (ties into the finial) and runs **two blocks tall per column using the cell *above* each step**
     (`[y, y+1]`). It **skips any cell already holding canvas wool** and explicitly includes `c_sail`,
     so a rigging block always lands **on top** of the sail's head wool block (not beside it / not
     carving it), while consecutive steps connect face-to-face down the diagonal.
   - **No sail-on-sail intersection:** before laying each canvas cell the jib checks the placement
     cache (`get_cached_block`) and **skips any cell already holding a sail** (`*wool`). Since the
     square sails + spanker (and any future sail) are placed earlier in `masts::build`, the jib never
     overlaps them.

   - **Stay always present + sail-state gating:** the **forestay stay is built whenever there's a
     bowsprit + foremast** (standing rigging), independent of the jib roll — so a ship with **no jib
     bent on** still shows its stay. The **canvas** is drawn only when the jib **rolled** (`has_jib`)
     **and** the sails are **set** (`draw_canvas = has_jib && Full`). With no canvas (no jib, or
     `None`/`Furled`) the **whole forestay stay** (bowsprit tip B → masthead, 2-tall, tied down to
     the bowsprit at its forward end) is rigging and nothing else; with canvas, only the head bridge
     is bare rigging and the rest of the stay is the canvas luff. Asserted by
     `jib_furled_is_rigging_only`.

   So the sail's head sits over the bowsprit on the stay, the foot pinches to 2 ties, and the body
   bellies to one side. The foremast pole/yards are skipped so the canvas never carves them.
   **Size-gated, chance rising with size** (`JIB_CHANCE_MEDIUM`/`LARGE`/`HUGE` = 55/80/100; Small
   has no bowsprit → none) and only when a bowsprit exists. Billow depth `dc.wind ·
   SAIL_JIB_WIND_FACTOR`. Placement verified by `jib_places_rigging_and_wool`.

   **Rigging material (chain / fence) — `RiggingMaterial` (`additions.rs`).** All thin rigging
   lines (the jib forestay + foot hangers; later shrouds/stays) are drawn from **one per-ship
   material**: a `minecraft:chain` or a palette **fence** post (railing-wood role). Chosen by
   **chance** (`RiggingMaterial::pick`, `RIGGING_CHAIN_CHANCE`) or **forced as an option**
   (`ShipV2Spec::with_rigging`), resolved once in `build_ship_v2` and carried on `DeckContext`.
   **Chains appear to be silently dropped by the current live server** (offline scans place them
   fine, in-game shows none — the adjacent wool places normally), so `RIGGING_CHAIN_CHANCE`
   defaults to **0 (always fence)** until that's understood; flip it up (or per-ship roll) once
   chains place reliably. `jib_places_rigging_and_wool` asserts both materials are honoured
   (chain → chains present; fence → zero chains, fences present).

   _Verify on the live screenshot:_ forestay reads as a clean diagonal from the bowsprit tip up to
   the foremast head (not occluded by the course); the jib foot floats above the bowsprit with
   short hanger ties (tune `SAIL_JIB_FOOT_RAISE`); fences read as the rigging line; no line carves
   the mast or yards.

   **Yard/sail stacking** is sized so the **lowest sail (course) is the largest** — yards
   are laid out **bottom→top** from the foot base (rail clearance excluded from the budget),
   each sail height weighted heaviest at the bottom and shrinking upward (`MAST_SAIL_GROWTH`);
   bottom yard is also widest. `MAST_YARD_SPAN_PER_SAIL` (now 11) sets how many yards/stays a
   mast gets — larger = fewer stays, taller sails. Asserted by `square_sails_largest_at_bottom`.

   _Verify on the live screenshot:_ curve reads as wind-filled (tune `SAIL_WIND`); billow
   faces forward (flip `SAIL_BILLOW_DIR` if not); sails span cleanly stay-to-stay with the
   lowest ending 2–3 above deck; no clipping into the mast or yards.

   **Masthead flags — implemented** (`additions/masts.rs`, `build_flag`). A small wool
   **pennant** streams **aft** off each mast's fence finial: **4–7 blocks long** (rolled
   per mast), **1 block tall** (`FLAG_HOIST_HEIGHT`; can taper from a taller hoist body).
   To read as cloth flapping (not a rigid flat plane), each column aft is **staggered in
   both `y` and `z`** on two out-of-phase sine ripples whose **amplitude grows toward the
   free fly end** (the hoist is pinned to the staff) — a curved 3-D ribbon, the wind on a
   slight angle. One heraldic **wool colour per ship** (`FLAG_COLORS`, hardcoded like the
   quartz sails until a cloth palette role exists); ripple phase + length seed rolled per
   ship and varied per mast. Flies regardless of `sail_state`. Tunables: `FLAG_*` in
   `tuning.rs`.

   _Verify on the live screenshot:_ flag reads as a flapping pennant (tune `FLAG_WAVE_AMP_Y`
   / `FLAG_WAVE_AMP_Z` / the freqs); streams clear aft of the rigging; length range looks
   right; colour variety across the fleet.

   **Helm (ship's wheel) — implemented** (`additions/helm.rs`, `DeckAddition::HelmCapstan`). A
   three-block fitting on the **quarterdeck** (centreline, **halfway between the aftmost mast and the
   stern**, but never closer than `HELM_STERN_CLEARANCE` (2) clear blocks to the stern railing; masts
   lean forward so a station below the aft mast's `base_x` is clear of the pole, and the wheel cell
   `hx-1` is kept on deck): a **lectern** base, a
   **fence** post, and an **open trapdoor as the wheel** — folded up **vertical on the stern (rear)
   side** of the post. A trapdoor hinges to the block **opposite** its `facing`, so `facing=stern`
   hinges it **onto the fence** (attached, not floating) with the disc standing on the rear as the
   wheel. Lectern/trapdoor are hardcoded oak for now (ship is `ship_oak`), the fence is palette wood.
   Built for all ships (Medium+ gating deferred). Asserted by `helm_places_wheel`.

   **Masts keel-stepped fix:** the mast pole now starts at **`y = 1`** (resting **on** the keel's
   bottom course), not `y = 0` (which poked through the keel underside).

   **Mast-to-mast stays — implemented** (`additions/masts.rs`). On **2+ mast ships**, a
   standing-rigging line (chain/fence per `RiggingMaterial`) connects **each mast to the next** —
   **`MAST_STAY_THICK` (1) block thick** but run as a **4-connected staircase** (`step_line_xy`) so
   the diagonal connects face-to-face with no corner gaps. It attaches to the **top of the mast pole
   (below the fence finial)** and **skips the poles, finials, and flag cells**, so it never carves
   the masts or the masthead pennants. The finial itself is bumped from `MAST_TOP_FENCE` (2) to
   `MAST_TOP_FENCE_MULTI` (3) on multi-mast ships, so the taller top reads better with the stays.
   Asserted by `mast_stays_connect_tops`.

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
