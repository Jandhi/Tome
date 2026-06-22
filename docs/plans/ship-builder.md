# Ship Builder — Initial Implementation Plan

Procedural ship generator for Tome, modeled on the `buildings_v2` pipeline. Goal:
several **size classes** that compose independently chosen **hull shapes** and
**sail/rig plans**, placed into the world through the existing `Editor`.

## Goals & Non-Goals

**Goals**
- Multiple size classes (rowboat → galleon) with sensible dimension envelopes.
- Hull shape as a swappable strategy (rowboat, cog, caravel, longship, …).
- Sail/rig plan as a swappable strategy (none/oars, single-mast, multi-mast).
- Deterministic from a seed (`noise::RNG`), like every other generator.
- Offline/dry-run buildable + an ASCII/SVG diagnostic, mirroring buildings_v2.

**Non-Goals (v1)**
- Below-deck interior rooms/furnishing (designed below, but **not implemented in
  the first pass** — v1 stubs an empty hold).
- Sailing/physics or animated sails.
- Settlement-level placement (docks, harbors) — separate follow-up.

## Architecture

New module tree under `src/generator/ships/` (sibling of `buildings_v2`), declared
in `src/generator/mod.rs`. Reuse, don't fork: `Editor`, `World::synthetic`,
`noise::RNG`, `materials::Palette`, `geometry` (Point2D/3D, Rect2D/3D, Cardinal),
`minecraft::Block`.

```
src/generator/ships/
  mod.rs           # ShipClass, HullShape, RigPlan enums; ShipContext; re-exports
  pipeline.rs      # ShipCtx (editor/data/palette/rng) + build_ship() orchestrator
  dimensions.rs    # ShipClass -> length/beam/depth/freeboard/mast-count envelopes
  hull/
    mod.rs         # HullShape dispatch; HullModel output struct
    rib.rs         # cross-section profiles (rib curves) per shape
    plank.rs       # plank the ribs: keel, hull planking, gunwale, stem/stern
    deck.rs        # deck planking + hatches/openings from HullModel
  rig/
    mod.rs         # RigPlan dispatch
    mast.rs        # mast + crow's nest + boom/yard placement
    sail.rs        # sail surfaces (wool/banner), furled vs full
    rigging.rs     # shrouds/stays via fences/chains/tripwire
  fittings.rs      # rudder, oars, railings, ladders, lanterns, anchor, figurehead
  blueprint.rs     # ShipBlueprint + render_ascii (per-deck) for diagnostics
  test.rs          # offline build_ships_offline + invariant/property tests
```

### Pipeline order (`build_ship`)

Mirrors the buildings_v2 "stage producing a model, later stage consuming it" style.

1. `dimensions::resolve(class, rng)` → `ShipDimensions` (length, beam, depth, mast slots).
2. `hull.build_model(dims, rng)` → `HullModel`: ordered ribs, keel line, deck Y,
   waterline Y, gunwale outline, hatch cells. Pure geometry, no block writes.
3. `hull::plank(ctx, &model)` — keel, hull planking, stem/stern posts, gunwale.
4. `hull::deck(ctx, &model)` — deck planking, rails, hatch openings.
5. `rig.build_plan(&model, dims, rng)` → `RigModel` (mast bases, yard heights, sail rects).
6. `rig::raise(ctx, &rig_model)` — masts, yards, sails, rigging.
7. `fittings::place(ctx, &model, &rig_model, rng)` — rudder, ladders, lanterns, anchor.
8. `check_ship_invariants(&model, &rig_model)` — see below.
9. Return `ShipOutput { dims, hull_model, rig_model, class, hull_shape, rig_plan }`.

`build_ship` is `async` (block writes are async); caller owns the final
`editor.flush_buffer()`, exactly like `build_house`.

### Coordinate convention

Build the ship in a **local frame** (bow toward +X, length along X, beam along Z,
keel at Y=0) and translate/rotate to the world via a `Cardinal` heading + origin.
Keeps hull math symmetric about the Z centerline and rotation a single transform —
do not bake world coords into the hull/rig models.

## Type sketches

```rust
// mod.rs
pub enum ShipClass { Rowboat, Sloop, Cog, Caravel, Galleon }
pub enum HullShape { RowboatHull, RoundCog, SleekCaravel, Longship }
pub enum RigPlan   { Oars, SingleMast, TwoMast, ThreeMast }

pub struct ShipContext {           // analogous to BuildingContext
    pub class: ShipClass,
    pub hull_shape: HullShape,
    pub rig_plan: RigPlan,
    pub heading: Cardinal,
    pub waterline_y: i32,          // where the hull sits in world Y
}

// pipeline.rs
pub struct ShipCtx<'a> {           // analogous to BuildCtx
    pub editor: &'a mut Editor,
    pub data: &'a LoadedData,
    pub palette: &'a Palette,
    pub rng: &'a mut RNG,
}
```

`HullShape` / `RigPlan` start as enums with a `match` dispatch (matches the
codebase's `RoofStyle`/`TimberPattern` idiom). Promote to traits only if the match
arms get unwieldy.

### Size class envelopes (first-pass numbers, tune later)

| Class    | Length | Beam | Decks | Masts |
|----------|--------|------|-------|-------|
| Rowboat  | 5–7    | 2–3  | 1     | 0 (oars) |
| Sloop    | 9–13   | 3–4  | 1     | 1 |
| Cog      | 14–18  | 5–6  | 1+hold| 1 |
| Caravel  | 18–24  | 6–7  | 2     | 2 |
| Galleon  | 26–36  | 8–11 | 2–3   | 3 |

`dimensions.rs` owns these as a `match` returning ranges, like
`SizeClass::target_area_min`.

### Class → valid combinations

Not every hull/rig pairs with every class. `mod.rs` exposes
`ShipClass::hull_shapes()` and `ShipClass::rig_plans()` returning the allowed
variants (mirrors `Culture::roof_styles()`), so callers/random selection stay valid.

## Hull generation detail

The hard part. Approach: **rib + plank**, not voxel-fill.

1. **Spine**: keel line along X at Y=0; stem (bow) and sternpost rise at the ends.
2. **Ribs**: at each X station, a cross-section curve in the Z–Y plane giving hull
   half-width and depth. Shape strategy controls the curve:
   - `RoundCog`: near-semicircular, full beam amidships, tucked ends.
   - `SleekCaravel`: V-bottom, finer entry, pronounced sheer.
   - `Longship`: shallow, symmetric double-ended, low freeboard.
   - `RowboatHull`: tiny, near-flat bottom.
   Taper beam toward bow/stern with an easing curve over X.
3. **Plank**: connect adjacent ribs with stair/slab/full blocks following the curve.
   **Reuse `roof::blocks` stair-stepping helpers first** (decision #4) before
   writing ship-specific sloped-surface code. Gunwale = top edge ring.
4. **Deck**: fill the deck plane at deck-Y inside the gunwale; cut hatches.

`HullModel` carries everything downstream needs without re-deriving: rib outlines,
deck cells, gunwale ring, mast-base candidate cells, waterline/deck Y, **and the
hold volume** (interior cells below deck) so the future interior system has a clean
input without re-running hull math.

## Resolved decisions

1. **Water vs dry-dock (v1):** ships **float at a fixed waterline on a water
   flatworld**. Offline tests use `World::synthetic` filled with water at a known Y;
   `ShipContext.waterline_y` pins where the hull sits. No terrain/water-finding in
   v1 — that's Phase 4.
2. **Rotation:** **cardinal headings only** to start (`Cardinal`). Hull/rig models
   are built in the local bow=+X frame and transformed by one cardinal rotation +
   origin translation at placement. Diagonal hulls are explicitly out of scope for now.
3. **Below-deck interiors:** **designed now, implemented later** (see next section).
   v1 builds the hold as an empty enclosed volume with a deck hatch + ladder; no
   rooms or furniture.
4. **Sloped-surface helpers:** reuse `roof::blocks` stair-stepping first; only fork
   ship-specific helpers if the roof helpers can't express hull curvature cleanly.

## Below-deck interiors (design only — NOT in first implementation)

Goal: when enabled, partition and furnish the hold using a system parallel to
`buildings_v2/rooms` + `furnish`, so the two share concepts (CellState, room types,
furniture data) without ships depending on building internals.

### Shared input

`HullModel.hold_volume` already exposes the below-deck interior: per-deck-level a
set of walkable cells bounded by the curved hull. This is the ship analogue of a
building `Frame` floor plan — an irregular (non-rectangular) cell region.

### Module sketch (future `src/generator/ships/hold/`)

```
hold/
  mod.rs       # build_hold(ctx, &hull_model, class) entry; HoldPlan output
  partition.rs # split the hold into compartments along bulkheads (X-stations)
  assign.rs    # assign CompartmentType per compartment (fore/aft/by class)
  furnish.rs   # place furniture per CompartmentType
  cells.rs     # CellState grid over the irregular hold footprint
```

### Key design choices

- **CellState reuse:** lift the `Empty / Blocked / BlockedReachable /
  UnblockedReachable` semantics from buildings_v2 (documented in CLAUDE.md). Either
  share the enum via a small common module or mirror it — decide when implementing;
  prefer extracting a shared `cells` type so invariants stay identical.
- **Partitioning:** ships partition along the length (bulkheads at chosen X-stations)
  rather than the rectangle-subdivision used for houses, because the hold is a long
  curved tube. Compartments get progressively smaller toward bow/stern as beam tapers.
- **Compartment types (ship-flavored room types):** `Hold` (cargo: barrels, crates,
  chests), `Quarters` (bunks/beds), `Galley` (furnace, cauldron, smoker),
  `CaptainsCabin` (aft, larger — desk, bookshelf, bed, chest), `Brig` (iron bars),
  `PowderStore`. Allowed set scales with `ShipClass`, mirroring how `RoomType`
  availability scales with `SizeClass`.
- **Furnishing:** reuse the furniture-data approach from `furnish/data.rs` (JSON
  furniture catalogs keyed by compartment type) and the placement/approach-cell
  logic. The `BlockedReachable` approach-cell invariant carries over unchanged.
- **Headroom & access:** decks 2.5–3 blocks of clearance; deck hatch + ladder is the
  vertical link (the same role attic ladders play in buildings). Companionway stairs
  for larger classes.
- **Invariants (when built):** every compartment reachable from the deck hatch;
  every `BlockedReachable` furniture cell has a walkable neighbor; bulkhead doorways
  connect adjacent compartments; no furniture clips the curved hull wall.

### Why deferred

Interiors depend on a *stable* `HullModel.hold_volume` and a working deck/hatch.
Building them against a still-changing hull would churn. So v1 ships the hold as an
empty, enclosed, laddered volume; interiors land once hull geometry is locked.

## Diagnostics & testing (copy the buildings_v2 discipline)

- `render_ascii(&ShipBlueprint)` per deck + a side-profile view; write SVG + `.txt`
  to `output/` from an offline test, exactly like `build_furnished_houses_offline`.
- `build_ships_offline` test: `World::synthetic` (water-filled to `waterline_y`) +
  `get_offline_editor`, build one of each class×shape×rig, no server. Canonical
  local iteration loop.
- `check_ship_invariants`:
  - deck is watertight (no deck cell directly over open water / hull gap),
  - every mast base sits on a deck cell,
  - hull is symmetric about the Z centerline (within stem/stern asymmetry),
  - gunwale forms a closed ring,
  - hold volume is enclosed by hull on all sides (precondition for interiors),
  - bounding box stays within declared dimensions.
- `ship_invariants_property_test`: N classes × M seeds through the offline pipeline
  with invariants asserted, mirroring `pipeline_invariants_property_test`.

## Materials

Reuse `materials::Palette`. Add ship-oriented palettes in `data/palettes/ships/`
(hull planks, deck, trim, sail wool color, accent). Sails: white wool or banners;
later support dyed/heraldic. Follow the JSON `Loadable` pattern — no hardcoded blocks.


## Resources for help

General tips:
The Keel: Place a horizontal line of slabs underwater to serve as the baseline and backbone of the build.
The Hull: Build an elongated teardrop shape around the keel. The widest part of the ship should sit near the middle/bottom, tapering inward as you build up toward the waterline.
Decks & Ribbing: Place internal rib supports every 3 to 5 blocks. Use slabs for the main deck to preserve interior headroom for cabins.
Masts & Sails & Rigging: Erect vertical pillar logs, add horizontal spars, and shape wool or banners for billowing sails. Use chain for rigging.
Details: Add lanterns, fences for rigging, trapdoors for cannon ports, and a rudder at the stern

https://piratemc.com/2020/09/09/minecraft-ship-tutorial-30-gun-frigate/
https://www.instructables.com/How-To-Build-a-Ship/
https://www.planetminecraft.com/blog/shipbuilding-guide-w-tips-17th-18th-century-ships/
https://www.minecraftforum.net/forums/minecraft-java-edition/creative-mode/362537-shipbuilding-tutorial

## Build phases

- **Phase 1 — skeleton — ✅ COMPLETE:** module tree (`src/generator/ships/`),
  enums, `dimensions.rs`, `HullModel` (incl. `hold_volume`), and a single
  `RowboatHull` planked + decked on the water flatworld. Offline test renders
  ASCII (`output/ships/rowboat.txt`); a property test runs all 5 size envelopes ×
  40 seeds through `check_ship_invariants`; a live `build_rowboat_in_minecraft`
  test places it on the running server. No rig, sealed empty hold.
  - Watertightness comes from a solid-volume → boundary-shell assembly, not
    stair-stepping. Verified in-game: it floats. **Observation:** the rowboat
    reads as boxy at 5×3 because its sides are vertical (one width per station)
    and it only tapers at the tips. Phase 2's per-height widths (curved
    cross-sections) fix this, and a gentle bilge is back-applied to the rowboat.
- **Phase 2 — one real ship — ✅ COMPLETE:** `RoundCog` hull + `SingleMast` rig +
  billowing wool sail + rudder/hatch/ladder fittings + accessible empty hold. Ribs
  carry **per-height half-widths** so cross-sections curve (rounded bilge), which
  de-boxes the rowboat too. Hull shell cells choose their block form — **stair
  bevels** round the bilge/flare/sheer (mirroring the `roof::blocks` technique;
  the inboard face stays solid so it's watertight), full blocks on the verticals.
  Sail billows on a sinusoidal pillow hump. Invariants (mast on deck, hatch over
  hold) + property + offline cog test (`output/ships/cog.txt`) + live `build_cog`.
- **Phase 3 — variety — ✅ COMPLETE:** all four hull shapes (`SleekCaravel` =
  asymmetric fine bow + V-bottom + lifting bow; `Longship` = slender symmetric
  dragon-prow), every shape reachable through `ShipClass::hull_shapes()`.
  Multi-mast rigs (`TwoMast`/`ThreeMast`) with staggered fore/main/mizzen heights,
  each carrying a billowing sail and shrouds; masts dodge the hatch column.
  Fittings polish: bowsprit, stern lantern, bow anchor chain. `ShipClass::pick_combo`
  + `SHIP_CLASSES` + `default_ship_palette`; ship palettes in
  `data/palettes/ships/` (`ship_oak`, `ship_spruce`, `ship_dark`). Variety property
  test (every class × hull × rig × seeds) + `build_fleet_offline`
  (`output/ships/fleet_*.txt`).
- **Phase 3.5 — reference pass — ✅ COMPLETE:** drew on the NBT reference ships
  (`data/structures/ship/`, ~28×9×8) and the guides in *Resources for help*.
  - **Bigger classes** across the board (cog ~20–26 long × 7 beam × 5 deep;
    galleon up to 44 × 13 × 8).
  - **Teardrop section with tumblehome:** ribs widen from a narrow keel to the
    **widest beam at the waterline**, then draw back in to a narrower deck
    (`rib::teardrop`).
  - **Slab keel** backbone under the flat of the bottom; **internal rib posts**
    (logs hugging the hull) every 3–5 stations; **two-tone wale** (secondary wood
    at the top strake).
  - **Chain rigging** (was fences); **mast height ~0.6 × length** (was overly
    tall). Stern lantern / bowsprit / anchor retained.
  - Deferred (noted): raised fore/stern castles, and a slab main deck.
- **Phase 3.6 — rig & topsides from the guides — ✅ COMPLETE:** read the linked
  tutorials (*Resources for help*) and applied:
  - **Bulwarks:** the deck is recessed with solid topsides rising 2 (1 for small
    craft) above it and a rail cap, replacing the bare fence gunwale.
  - **Gun ports:** trapdoors punched into the lower bulwark course every ~3
    stations along the sides.
  - **Tall-ship rig:** keel-stepped masts (run down to the hull bottom), **stacked
    sails** (course → topsail → topgallant) that shrink upward on **yards that
    shorten with height**, masts thinning to a **fenced topmast**, and **crow's
    nests**. Multi-mast layouts keep the staggered fore/main/mizzen heights.
  - **Rigging:** iron-bar shrouds/ratlines narrowing from the rail to the nest.
  - **Fittings:** a ship's wheel (stair on a fence post) aft, a longer bowsprit
    (~⅓ length), stern lantern raised above the rail.
  - Still deferred: rounded-bow/transom-stern reshaping, gradient sails, diagonal yards.
- **Phase 3.7 — castle, spanker & fixes — ✅ COMPLETE:**
  - **Raised aft quarterdeck (poop)** for Cog/Caravel/Galleon (`superstructure.rs`):
    a railed solid raised deck over the aft ~¼, with a ladder up its forward face;
    the helm, stern lantern, and flag now sit on it. The hatch moved to the waist
    (amidships) so it stays clear of the castle.
  - **Spanker:** a triangular fore-aft driver sail trailing aft of the aftmost
    mast (the guides' lateen/gaff rear sail).
  - **Stern flag** streaming aft on a pole.
  - **Bug fix:** mast ordering — the foremast now sits toward the **bow**, the
    mizzen toward the **stern** (was reversed).
- **Phase 4 — integration:** placement near water/docks in the settlement pipeline
  (terrain/water queries, claim cells) — separate plan.
- **Phase 5 — interiors:** implement the `hold/` system above once hull geometry is
  stable.

---

## Handoff — state for the next session (as of Phase 3.7)

### Where things live
- `src/generator/ships/`
  - `mod.rs` — `ShipClass`, `HullShape`, `RigPlan`, `ShipContext`, `Placement`
    (local→world transform), `ShipDir` (ship-relative facings → world `Cardinal`
    via `placement.world_cardinal`), `SHIP_CLASSES`, `pick_combo`, `default_ship_palette`.
  - `dimensions.rs` — per-class length/beam/depth/freeboard/mast envelopes + `resolve`.
  - `hull/rib.rs` — per-shape cross-sections; `teardrop()` is the shared section
    builder (keel→waterline widen, then tumblehome to deck).
  - `hull/mod.rs` — `build_model` (solid→boundary-shell, watertight by
    construction), `HullModel` (`hull_cells` as `HullPlank{local,form,cut}`,
    `deck_cells`, `gunwale`, `hold_volume`, `frame_posts`, `keel_slabs`, `hatch`),
    `check_ship_invariants`.
  - `hull/plank.rs` — `plank_hull` (blocks + stair bevels + two-tone wale),
    `place_keel`, `place_frames`.
  - `hull/deck.rs` — deck floor, bulwarks (recessed deck + solid topsides + rail),
    gun-port trapdoors.
  - `rig/` — `mod.rs` (`build_plan`, `Mast{base,foot_y,top_y,nest_y,yards}`,
    `Yard{y,half}`, spanker, `check_rig_invariants`), `mast.rs` (keel-stepped pole,
    fenced topmast, yards, crow's nest), `sail.rs` (white wool), `rigging.rs`
    (iron-bar shrouds).
  - `superstructure.rs` — `maybe_quarterdeck` → `CastleInfo{top_y,front_x}`.
  - `fittings.rs` — rudder, bowsprit, helm (stair on fence), stern lantern, stern
    flag, bow anchor, hatch+ladder; all castle-aware via `CastleInfo`.
  - `pipeline.rs` — `build_ship` orchestration order: dimensions → hull model →
    plank/keel/frames → deck/bulwark → quarterdeck → rig → fittings → invariants.
  - `blueprint.rs` — `render_ascii` (top-down, side profile, stern elevation).
  - `test.rs` — see below.
- `data/palettes/ships/` — `ship_oak` (default), `ship_spruce`, `ship_dark`.
- `data/structures/ship/*.nbt` — hand-built reference ships (NOT placed by code;
  used for visual reference only). Format note: they use Minecraft's `palettes`
  (plural, multi-variant) key, so they will NOT parse via `NBTStructure` (which
  expects singular `palette`). Read them with a raw `fastnbt::Value` + GzDecoder if
  you need to inspect again.
- `World::synthetic_water(build_area, floor_y, water_y)` in `src/editor/world.rs`
  — ocean flatworld for offline tests.

### Running tests
- Offline (no server, fast, the iteration loop):
  `cargo test ships:: -- --skip in_minecraft --skip build_rowboat --skip build_cog --skip build_fleet`
  (the property tests + `*_offline` builds). Writes `output/ships/*.txt`.
- Live (needs a running GDMC server + `/setbuildarea` over water): `build_rowboat`,
  `build_cog`, `build_fleet`. NOTE: these are plain `#[tokio::test]` (NOT
  `#[ignore]`), so a bare `cargo test` WILL try to hit the server. The user runs a
  live server, so they pass for them; in CI/serverless they'd fail — keep using the
  `--skip` filters above when iterating without a server.
- ASCII diagnostics can't show solid/placement detail (bulwarks, frames, keel,
  quarterdeck, wheel, flag, gun ports, ratlines) — only hull section, deck plan,
  masts and sails. Real verification is in-game screenshots.

### Conventions / gotchas
- Local frame: `x` = stern(0)→bow(+x), `z` = beam (centerline 0, ±), `y` = up from
  keel(0). Everything is built local then transformed by `Placement`. Bow points
  along `ShipContext.heading`. Only cardinal headings supported.
- Stair/trapdoor/ladder facings: ALWAYS derive world facing from a `ShipDir` via
  `placement.world_cardinal(dir)` so it rotates with heading. Never hardcode.
- Watertightness invariant depends on the boundary-shell: don't punch holes in the
  hull shell that open into `hold_volume` without keeping a sealed face. Gun ports
  are on the *bulwark* (above deck), not the hull shell, so they're safe.
- Stair-bevel orientation (`plank.rs`) and ladder/helm facings are my best reading
  of MC blockstate semantics and are UNVERIFIED visually. If a screenshot shows
  bevels notched the wrong way or ladders detached, the fix is a one-line flip
  (`out.opposite()` ↔ `out`, or toggle the `top_half` bool / facing).
- RNG: `RNG::new` takes `i64` (not `u64`). Use `rng.derive()` for each placer so
  adding a stage doesn't shift the main stream.

### Reference material (already read — see `## Resources for help`)
The inline "General tips" + 3 reachable guides (PirateMC frigate, MinecraftForum
tutorial, PlanetMinecraft/Lowry69 — the richest). Key techniques STILL not done:
- **Rounded bow + transom stern** (guides build hulls as stacked ellipses, rounded
  bow rather than a sharp point; flat transom at the stern with windows/gallery).
  Current hulls just taper to a point both ends — this is the biggest remaining
  realism gap.
- **Forecastle** (raised bow deck) to balance the quarterdeck.
- **Stern gallery/windows** (trapdoors + fence-gates for gilded detail; glass/stair
  windows on the transom).
- **Gradient sails** (birch→sandstone→bone_block) and **diagonal yards** (rotate
  yards a few blocks for organic, wind-filled look).
- **Cannons** at the gun ports (dispenser + iron block behind), **stored cargo**
  (barrels/chests) and **ship's boats** on deck.
- Hull **colour bands** (dark wale line, accent bulwark colour per "nation").

### Suggested next steps (priority order)
1. **In-game screenshot pass** to verify stair/ladder/helm facings before building
   more on top of them.
2. **Forecastle** (mirror `superstructure.rs` for the bow) — quick win, balances
   the silhouette.
3. **Rounded bow / transom stern** reshaping in `rib.rs` — highest realism payoff,
   most involved (the bow currently comes to a 1-wide point; guides want a blunt
   rounded stem and a flat transom).
4. Cannons in gun ports + cargo in the hold (ties into Phase 5 interiors).

### Open questions for the user
- Should the live ship tests become `#[ignore]` so `cargo test` is clean without a
  server? (Left as-is per their preference — they run a live server.)
- Keep an offline `build_fleet_offline` for serverless coverage of every shape, or
  is the live `build_fleet` enough? (Currently live-only.)
