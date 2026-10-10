# House spec

A YAML file holding the high-level decisions of one buildings_v2 house. The
generator writes one for every house it builds; `--spec` builds from one.
Every field is optional: a pinned field is used as written, an open field is
drawn from the spec's `seed`, so the same spec builds the same house every
time, and pinning or editing one field leaves unrelated choices alone.

The spec holds decisions, not blocks. Wall segments, stairs, windows and
furniture stay computed by the pipeline (and seeded), so a spec can't build a
house that breaks the invariants.

## Example

```yaml
seed: 4127730158
culture: medieval
size: manor
palette: { style: whitewash_wattle, base: medieval_spruce, wood: oak, stone: cobblestone, roof: brick_roof }
colour: red
roof: { gable: stairs }        # or flat, irimoya, { hipped: slab }
footprint:                     # plot-relative, core first (what --rects takes)
  - { x: 0, z: 0, w: 12, h: 8, floors: 3 }
  - { x: 12, z: 2, w: 6, h: 5, floors: 2 }
jetty: true
engawa: false
knee_wall: false
tall_top_floor: false
timber: { decorated: { spacing: 3 } }   # or plain
attic: false                   # derived: today exactly { gable: double } has one
cellar: { theme: wine }        # or false; themes storage, wine, larder, workshop
rooms:                         # per rect, per floor, bottom up; then its attic room
  - [common, kitchen, bedroom]
  - [storage, bedroom]
site: { origin: [120, 344], front_door: south, base_y: 71 }   # read back, not pinnable
```

The generator writes these (phase 2) with every field filled; serde_yaml
writes them in block style. `site` records where the house stood (absolute
corner the footprint is relative to, front door side, ground-floor y); a
build from the spec takes all three from its plot and road. `window_fill` is
left out: it is the culture's default, not a per-house decision. The manor
family banner is left out too (its design is block-entity NBT drawn on the
`colour` stream; the blazon goes to the chronicle).

## What we have today (findings)

- Settlement houses all draw from one town-wide RNG stream (`settlement.rs:1335..1532`,
  and the same `rng` goes into `build_house`). Any change in one house's draw
  count reshuffles every house built after it.
- Inside `build_house` most decisions draw from that shared stream in order:
  floors per rect, stairs, ridge axis, chimney, parapet, room budget and types,
  attic types, desert curtains. Timber, cellar, furnish, exterior and ramps
  `derive()` their own (each derive still consumes one parent draw).
- `--rects` already pins the footprint (`showcase.rs:141`, `:365`), but skipping
  the footprint draws moves the engawa roll and everything after it, so a
  pinned footprint doesn't rebuild the same interior.
- `BuildingContext` (`buildings_v2/mod.rs:174`) holds most decisions but has no
  derives; `SizeClass`, `RoofStyle`, `TimberPattern`, `WindowFill`, `Culture`,
  `RoomType` need serde. `Rect2D`, `Cardinal`, `Color`, `Palette` already have it.
- `roll_palette` (`style.rs:60`) returns only the merged palette, not which
  style/wood/stone/roof/accent it picked.
- `houses.json` writes two `"rooms"` keys, so the room count is lost.
- `RNG::from_seed_and_string(seed, name)` exists: keyed streams are cheap.

## Design

**One seed per house, one stream per decision.** Each house gets a `seed`
(the settlement draws exactly one value per house from the town stream, so
houses stop disturbing each other). Every decision draws from
`RNG::from_seed_and_string(seed, "<decision>")`: `size`, `core`, `palette`,
`colour`, `roof`, `footprint`, `floors`, `engawa`, `jetty`, `timber`,
`stairs`, `roof.ridge`, `roof.chimney`, `roof.parapet`, `rooms`, `attic`,
`curtains`, `cellar`, `furnish`, `exterior`, `ramps`. A pinned field skips
its own stream and touches no other.

**Resolve, then build.** `HouseSpec` (all `Option`) resolves against the seed
and the plot into a complete `ResolvedSpec`; `build_house` takes that instead
of re-deciding. The resolved spec is what gets written out, so every emitted
spec is fully pinned and still carries its seed for the computed stages.

**Dependent choices may change, independent ones may not.** Editing the
footprint can change the rooms (they depend on it); it must not change the
palette, timber or roof.

## Status

- **Phase 1 done** (288e10a, e626814): `HouseSeed`, one named stream per decision (`buildings_v2/seed.rs`).
- **Phase 2 done** (6a9679e): `HouseSpec` written for every `--houses` and town house.
- **Phase 3 done**: `resolve` + `ResolvedHouse::build` (`buildings_v2/spec.rs`, whose module
  doc tables what each open field draws); `--houses --spec <file>...` and sweeps; basic
  field-named errors (unknown names, footprint geometry, rooms shape, attic vs roof).
  - No stream was split. Pins that live inside a multi-draw stream still make every draw
    and override the result (frame floors, the cellar roll, the palette style pick), and
    the pinned values that skip a draw (room types, cellar theme) sit after the stage's
    `derive()`s, so nothing else moves. Plain `--houses` output and the caldera town are
    block-identical to before.
  - Sweep seeds: house k of N from a spec is `RNG::from_seed_and_string(base, "sweep<k>")
    .next_i64()` (`showcase::spec_house_seed`), `base` = the spec's `seed`, else the
    `--seed` stream's draw for that spec's position (what plain house j would get). A
    single house (`--count 1`, the default with `--spec`) uses `base` as is.
  - Open `size`/`roof`/`palette`/`colour`/`jetty` draw on their streams as a town would but
    without a town: uniform size, the culture's roof bag, a uniform catalog style (no local
    wood bias), the culture's colour pool. `palette.style: default` is plain `--houses`'
    culture palette; plain `--houses` is itself a spec pinning culture, size, roof, that
    palette and colour, jetty and the variant.
  - `site` is ignored on input except the new `site.plot` (plot size, written by `--houses`):
    the front door goes on the side nearest the plot edge, so the same plot puts it back.
  - Examples: `docs/examples/house_specs/fixed_shape.yaml`, `fixed_style.yaml`.

## Phases

1. **Keyed streams.** Per-house seed and per-decision streams, no spec yet.
   Every seed builds a different town afterwards (as with the squirrel3 fix);
   `offline_town_is_deterministic` and the invariant property test must hold.
2. **Spec type + emit.** `HouseSpec` with serde (snake_case names), derives on
   the decision enums, `roll_palette` reporting its picks. `--houses` writes
   `<house>.spec.yaml`; town runs write `output/runs/<seed>/houses/*.spec.yaml` (opt-in since `--out`: `--out +specs`).
   Fix the duplicate `rooms` key in houses.json.
3. **Build from spec.** `--houses --spec <file>...` builds one house per spec;
   `--rects` becomes shorthand for pinning `footprint`. Tests: an emitted spec
   rebuilds a byte-identical `town.scene`; pinning a field to its drawn value
   changes nothing; changing `timber` changes only timber blocks.
   **Sweeps:** `--houses --spec <file> --count N` builds the spec N times with
   house seeds derived from the spec's seed and the index, so only the open
   fields vary, into one contact sheet plus each house's resolved spec. Fixed
   shape + open style shows off the generator; fixed style + open shape
   iterates on a style across many shapes (sweep before/after a change and
   `--compare` the two `town.scene`s). Same spec + count = same sweep.
4. **Validation.** Errors that name the field: rects overlapping, disconnected
   or outside the plot, room lists that don't match rects/floors, unknown
   names (listing the valid ones). Aimed at an LLM fixing its own spec.
5. **Later.** Settlement lots taking a spec (hand-placed landmark houses), a
   schema reference doc for LLM authoring.

## Decisions (2026-10-10)

- Phase 1 may change every existing seed's output.
- Rooms: v1 pins room types per rect and floor; the layout stays computed.
- Palette: pinnable by style name and/or by individual materials, each optional.
- A spec ranges from just a `seed` to a fully pinned shape, rooms and style;
  the front door and base height always come from the plot.
