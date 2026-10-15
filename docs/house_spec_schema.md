# House spec reference

How to write a house spec: a YAML file describing one buildings_v2 house. This
page is the field reference, written so a person or an LLM can author a spec
and fix it from the error messages. Design and history are in
[plans/house_spec.md](plans/house_spec.md); the code is
`src/generator/buildings_v2/spec.rs`.

## The idea in four rules

1. **Every field is optional.** A field you write is *pinned* and built as
   written. A field you leave out is *open* and drawn from the house's seed.
2. **The seed fixes everything open.** The same spec with the same `seed`
   builds the same house, block for block. Pinning a field never reshuffles the
   others: each decision draws from its own stream of the seed.
3. **A spec holds decisions, not blocks.** Walls, windows, stairs and furniture
   are always computed from the decisions, so a spec can't break a house.
4. **Typos fail loudly.** An unknown key or word is refused and the error lists
   the valid ones. Contradictions are refused with every problem listed at once.
   Pins that build but are odd for the house print a `note:` and build anyway.

The smallest spec is `{}` (everything drawn). A good starting point is to pin
what you care about and leave the rest open:

```yaml
culture: medieval
size: hall
roof: { gable: double }
colour: purple
cellar: { theme: wine }
```

## Running a spec

```bash
cargo run --profile fast -- --houses --spec my_house.yaml
```

Builds it alone on a flat world into `output/houses/spec-my_house/`: renders,
a view inside every room, and `my_house_00.spec.yaml`, the **resolved spec**
with every field filled in as built. Copy fields from the resolved spec back
into yours to pin what you liked.

| To | Run |
|---|---|
| See the variety a spec allows (only open fields change) | `--houses --spec my_house.yaml --count 8` |
| Use a different base seed for that sweep | add `--seed N` (only when the spec has no `seed`) |
| Build several specs side by side | `--houses --spec a.yaml --spec b.yaml` |
| Put a spec into a generated settlement | `cargo offline fixtures/<snapshot>.nbt.gz [seed] --spec my_house.yaml` (also works on live runs) |
| Get the specs of every house in a settlement | add `--out +specs` to a settlement run: `output/runs/<seed>/houses/house_<n>.spec.yaml` |

With `--houses`, the flags `--culture`, `--size`, `--jetty`, `--knee-wall`,
`--tall-top` and `--rects` fill fields the spec leaves open.

In a settlement, each spec takes the first road slot where it fits, before an
ordinary house gets it, turned so its front faces the road. Open `palette` and
`colour` take the district's, so the house fits its street. A spec too big for
every lot is reported as not built; see `spec_houses` / `specs_unplaced` in
`metrics.json`.

## Fields

| Field | Values | Open: drawn as |
|---|---|---|
| `seed` | any integer | `--houses`: from `--seed` (default 42); settlement: one per road slot |
| `culture` | `medieval`, `desert`, `japanese` | `--culture` (default `medieval`); in a settlement, the settlement's culture |
| `size` | `cottage`, `house`, `hall`, `manor` | any of the four; in a settlement, the road tier's sizes |
| `roof` | `flat`, `irimoya`, `{ gable: slab \| stairs \| double }`, `{ hipped: slab \| stairs }` | from the culture's roofs, weighted by size |
| `palette` | `{ style, base, wood, stone, roof, accent }`, each optional | a style from the culture's catalog, then its material pools |
| `colour` | a dye colour (below) | from the culture's colour pool |
| `footprint` | list of rects `{ x, z, w, h, floors }`, core first | a shape for the size class |
| `rooms` | one list of room types per footprint rect | the size class's room rules |
| `cellar` | `false`, `true`, `{ theme: storage \| wine \| larder \| workshop }` | the size class's chance, then a theme |
| `timber` | `plain`, `{ studded \| braced \| decorated: { spacing: N } }` | by size (medieval); `plain` elsewhere |
| `jetty` | `true`, `false` | medieval: 2 in 3; others: no |
| `engawa` | `true`, `false` (Japanese only) | Japanese manor: always; hall: 1 in 3; else no |
| `attic` | `true`, `false` | follows `roof` (see below) |
| `knee_wall` | `true`, `false` | off |
| `tall_top_floor` | `true`, `false` | off |
| `site` | written by the generator | read back only (see below) |

### culture

Sets the look and the defaults for everything else: its roofs, palettes,
colours, and whether jetties and engawas happen. Pinning another culture's roof
or palette is allowed (with a note for the roof).

| Culture | Roofs | Colour pool |
|---|---|---|
| `medieval` | `{ gable: slab }`, `{ gable: stairs }`, `{ gable: double }` | red, blue, green, yellow, white, black, brown |
| `japanese` | `{ hipped: slab }`, `{ hipped: stairs }`, `irimoya` | red, blue, white, black, purple, yellow, green |
| `desert` | `flat` (square rects get domes) | white, red, green, yellow, light_blue, black |

Open Japanese roofs lean on size: a cottage is always hipped, a house is
irimoya 1 time in 6, a hall half the time, a manor 3 times in 4.

### size

| Size | Floors (core) | Max wings | Bedrooms | Typical core (front x depth) |
|---|---|---|---|---|
| `cottage` | 1 | 1 | 0-1 | 5-6 x 5-7 |
| `house` | 1-2 | 2 | 1-2 | 6-8 x 7-10 |
| `hall` | 2-3 | 3 | 2-3 | 8-10 x 9-12 |
| `manor` | 2-3 | 4 | 2-4 | 9-12 x 11-15 |

Floors or wings outside these ranges build with a note.

### roof and attic

`roof` is a word or a one-key map:

```yaml
roof: flat
roof: irimoya              # Japanese hip-and-gable
roof: { gable: stairs }    # pitch: slab (shallow), stairs, double (steep)
roof: { hipped: slab }     # pitch: slab or stairs
```

Only `{ gable: double }` is steep enough for an attic, so `attic` always
follows the roof. Leave `attic` out unless you want to say "a roof with an
attic, any of them": `attic: true` with `roof` open draws among the roofs that
have one. `attic: true` with a roof that has none is an error.

### footprint

The house's ground plan: one or more axis-aligned rects, **core first**, in
blocks. `x` grows east, `z` grows south; `w` is the size along x, `h` along z.
Coordinates are relative to the corner of the plot, so start at `x: 0, z: 0`.

```yaml
footprint:
- { x: 0, z: 0, w: 9, h: 7, floors: 2 }   # core
- { x: 4, z: 7, w: 5, h: 5, floors: 1 }   # wing south of the core's east half
```

```
x: 0        8
  +---------+  z: 0
  |  core   |
  |  9 x 7  |
  +---+-----+  z: 6
      |wing |  z: 7..11
      +-----+
    x: 4  8
```

Rules (each breach is an error naming the rect):

- At least one rect, every `w` and `h` at least 1.
- No two rects overlap.
- Every rect shares an edge with the rest (corner-only contact is not enough).
- `floors` is at least 1, and no wing has more floors than the core.
- Leave `floors` out to draw it; to pin `rooms`, pin every rect's `floors`.

Doors avoid the walls where a wing meets the core, and the main door goes on
the side nearest the plot edge. In a settlement that side is turned to face the
road (`site.front_door` says which side it is; south if missing).

### rooms

One list per footprint rect, in footprint order. Each list has one room type
per floor, bottom floor first, plus one more for the attic when the roof has
one. The layout inside each room (walls, doors, furniture) stays computed.

```yaml
rooms:
- [great_room, multi_bedroom]   # core: 2 floors
- [kitchen]                     # wing: 1 floor
```

| Room type | What it is |
|---|---|
| `common` | single-room house: bed, furnace, crafting table, chest |
| `hearth` | main ground-floor room in a multi-room house: living + kitchen |
| `great_room` | large living/dining room; the ground floor of halls |
| `dining` | long table, chairs, candles (ground floor) |
| `kitchen` | furnaces, smoker, cauldron (ground-floor wing) |
| `pantry` | barrels, chests, hay bales (ground-floor wing) |
| `bedroom` | sleeping quarters (upper floors) |
| `multi_bedroom` | upper core split into a hallway and small bedrooms |
| `master_bedroom` | larger private bedroom in a wing |
| `study` | bookshelves, desk, enchanting table |
| `library` | bookshelf-lined, lectern (upper floor, manors) |
| `studio` | loom, glazed terracotta, flower pots, wool (upper floor, manors) |
| `armory` | armour stands, item frames, anvil (upper floor, manors) |
| `storage` | chests and barrels; filler |

`rooms` needs a pinned footprint with every rect's `floors` pinned; the list
lengths must match the floors (plus the attic).

### palette

A style name and/or the palette files it is made from. Each part is optional;
a pinned material replaces what the style would pick.

```yaml
palette: { style: whitewash_wattle }                  # style only: its pools roll
palette: { style: dark_tudor, roof: brick_roof }      # style, with the roof swapped
palette: { style: default }                           # the culture's plain palette
```

| Key | What it sets | Values (palette ids, `data/palettes/`) |
|---|---|---|
| `style` | everything below, from the style's pools | a style key from the table below, or `default` |
| `base` | walls, floors, flowers | `medieval_spruce`, `japanese_dark_blackstone`, `japanese_light_cherry`, `japanese_red_lacquer`, `desert_sandstone`, `desert_prismarine` |
| `wood` | frame, planks, doors | `oak`, `spruce`, `birch`, `dark_oak`, `acacia`, `jungle`, `cherry` |
| `stone` | foundation, cellar, base course | `cobblestone`, `stone_bricks`, `deepslate`, `blackstone`, `sandstone`, `red_sandstone` |
| `roof` | roof blocks | `medieval_roof`, `oak_wood_roof`, `red_wood_roof`, `brick_roof`, `acacia_wood_roof`, `cherry_wood_roof`, `blue_wood_roof`, `bamboo_roof`, `blackstone_roof`, `prismarine_roof` |
| `accent` | flat-roof parapet band (desert) | `accent_green_terracotta`, `accent_light_blue_terracotta`, `accent_orange_terracotta` |

Styles per culture (rare ones are landmarks a settlement uses sparingly):

| Culture | Style key | Base | Wood | Stone | Roof |
|---|---|---|---|---|---|
| medieval | `timber_spruce` | medieval_spruce | spruce | stone_bricks | medieval_roof |
| | `whitewash_wattle` | medieval_spruce | oak, birch, spruce, dark_oak | cobblestone, stone_bricks | oak_wood_roof, red_wood_roof, brick_roof |
| | `fieldstone` | medieval_spruce | oak, dark_oak | cobblestone | brick_roof, red_wood_roof |
| | `dark_tudor` | medieval_spruce | dark_oak | stone_bricks, cobblestone | red_wood_roof |
| | `stone_hall` (rare) | medieval_spruce | dark_oak | deepslate | blackstone_roof |
| | `acacia_trade` (rare) | medieval_spruce | acacia | stone_bricks | acacia_wood_roof |
| japanese | `dark_blackstone` | japanese_dark_blackstone | dark_oak | blackstone | red_wood_roof |
| | `white_slate` | japanese_dark_blackstone | dark_oak | blackstone | blackstone_roof |
| | `bamboo_roof` | japanese_dark_blackstone | dark_oak | blackstone | bamboo_roof |
| | `cherry_roof` | japanese_dark_blackstone | dark_oak | blackstone | cherry_wood_roof |
| | `light_cherry` | japanese_light_cherry | cherry | blackstone | red_wood_roof |
| | `cypress_town` | japanese_dark_blackstone | dark_oak, spruce | blackstone | red_wood_roof, acacia_wood_roof |
| | `cherry_ink` | japanese_light_cherry | cherry, dark_oak | blackstone, deepslate | red_wood_roof, blue_wood_roof |
| | `teal_shrine` (rare) | japanese_dark_blackstone | dark_oak | deepslate | blue_wood_roof |
| | `vermilion_hall` (rare) | japanese_red_lacquer | (base's) | blackstone | blue_wood_roof |
| | `vermilion_house` | japanese_red_lacquer | (base's) | blackstone | blackstone_roof |
| desert | `sandstone` | desert_sandstone | (base's) | sandstone | (flat) |
| | `red_sandstone` | desert_sandstone | (base's) | red_sandstone | (flat) |
| | `mixed_sands` | desert_sandstone | (base's) | sandstone, red_sandstone | (flat) |
| | `prismarine_noble` (rare) | desert_prismarine | (base's) | sandstone | (flat) |
| | `green_trim`, `azure_trim`, `ochre_trim` | desert_sandstone | (base's) | sandstone | (flat), with the matching accent |

### colour

The dye colour of banners, beds, carpets and other dyed accents: `black`,
`gray`, `light_gray`, `white`, `red`, `orange`, `yellow`, `lime`, `green`,
`cyan`, `light_blue`, `blue`, `purple`, `magenta`, `pink`, `brown`. Any colour
can be pinned; open ones come from the culture's pool. In a settlement a manor's
colour is its family colour (its door banners and the family's name).

### cellar

`false` (none), `true` (a cellar, theme drawn) or `{ theme: <name> }` with
`storage`, `wine`, `larder` or `workshop`. The cellar is a request: where the
ground is wet or there is no room for its stair, the house is built without
one (a settlement run prints a note when that happens).

### timber, jetty, engawa and the upper-storey variants

- `timber`: the frame pattern of the walls. `plain`, or a pattern with stud
  spacing: `{ studded: { spacing: 3 } }`, `{ braced: { spacing: 4 } }`,
  `{ decorated: { spacing: 3 } }`. The generator uses spacings 3 and 4; open,
  medieval houses roll by size (bigger houses get busier frames), other
  cultures stay `plain`.
- `jetty: true` asks for the upper floors to overhang the ground floor
  (medieval timber framing). It is a request: a shape, floor count or plot
  that can't take one builds flush, and the resolved spec still says `true`.
- `engawa: true` asks for a raised veranda around a Japanese house (an error on
  other cultures). Also a request: a core too small once inset builds plain.
- `knee_wall`, `tall_top_floor`: alternative upper-storey proportions, off
  unless pinned.

### site

Written by the generator into every resolved spec, ignored when building:

```yaml
site:
  origin: [-9290, -3562]   # world x, z of the footprint's corner
  front_door: north        # the side the main door opens out of
  base_y: 89               # ground-floor height
  plot: [24, 20]           # --houses only: the plot size
```

Two parts are read back: `front_door` tells a settlement which side to turn
toward the road, and `plot` (in `--houses`) sizes the plot so the door lands
on the same side again. A hand-written spec can leave `site` out.

## When it says no

Errors stop the build and list every problem, each naming its field and
saying how to fix it. Fix them all and run again. Real examples:

```
unknown field `floors`, expected one of `seed`, `culture`, `size`, `palette`, `colour`, `roof`, `footprint`, ...
colour: unknown variant `teal`, expected one of `black`, `gray`, `light_gray`, `blue`, `cyan`, ...

7 problems:
  - engawa: true, but the engawa is Japanese only (culture: medieval); drop it or set culture: japanese
  - footprint[1]: does not touch the rest of the footprint; every rect shares an edge with another (no gaps, no corner-only contact)
  - footprint[1].floors: 3 is more than the core's 2; the core (footprint[0]) has the most floors
  - rooms[0]: 1 entries but rect 0 has 2 floors (2 entries, bottom floor first)
  - rooms[1]: 1 entries but rect 1 has 3 floors (3 entries, bottom floor first)
  - cellar.theme: unknown theme "brewery"; one of storage, wine, larder, workshop
  - palette.style: unknown medieval style "tudor"; one of default, timber_spruce, whitewash_wattle, ...
```

Notes do not stop the build; they flag pins the generator itself would never
make, in case they were a mistake:

```
note: roof: irimoya is not a medieval roof ({ gable: slab }, { gable: stairs }, { gable: double }) (built as pinned)
note: footprint[0].floors: 3, but a cottage has 1 (built as pinned)
```

## Examples

More in [examples/house_specs/](examples/house_specs/).

A cottage, everything else drawn:

```yaml
culture: medieval
size: cottage
```

A medieval L-shaped house with its rooms pinned:

```yaml
seed: 7
culture: medieval
size: house
palette: { style: dark_tudor }
colour: red
roof: { gable: double }        # steep: has an attic, so each rooms list gets one more entry
footprint:
- { x: 0, z: 0, w: 9, h: 7, floors: 2 }
- { x: 4, z: 7, w: 5, h: 5, floors: 1 }
rooms:
- [hearth, bedroom, storage]   # core: ground, upper, attic
- [kitchen, storage]           # wing: ground, attic
jetty: true
cellar: { theme: larder }
```

A Japanese manor with a veranda, shape left to the seed:

```yaml
culture: japanese
size: manor
roof: irimoya
palette: { style: white_slate }
colour: black
engawa: true
```

A desert house with a painted parapet:

```yaml
culture: desert
size: house
palette: { style: azure_trim }
colour: light_blue
cellar: false
```
