# Garden styles

One file per culture (`medieval.yaml`, `desert.yaml`, `japanese.yaml`), read by
`GardenStyle::for_culture` (`src/generator/open_space/garden_style.rs`) and used
by `furnish_garden` for every town house's back garden. A file holds what a
culture's gardens are made of and how often each thing happens; the shapes
(beds, ponds, trees, walls) and where they go stay in code. Unknown keys are
refused, so a typo fails loudly.

| Key | What it is |
|---|---|
| `layouts` | `[{layout, weight}]`: the layouts gardens are rolled from, in order (one entry: no roll). Layouts: `kitchen`, `flowers`, `orchard`, `work_yard`, `courtyard`, `zen`, `pond`, `moss`. A strip too shallow for beds is always a `work_yard`. |
| `walled` | When a garden is walled and closed (no gate, no garden path) instead of fenced: `{always: true}`, or `{big_house_area, big, other}` = percent chance behind houses whose frontage x depth is at least `big_house_area`, and behind the rest. |
| `fence` | `{post, gate, corner_lantern_chance}`: an open garden's fence; the gate gets its `facing`; leave the lantern chance out for none. |
| `wall` | `{base, body, coping, pier, pier_cap, creeper}`: a walled garden's wall. `base`/`body` are `[block, weathered block]` (weathered in patches); piers stand a block taller at corners, ends and every fourth cell; `creeper` is two leaf blocks mixed into climbing patches with vines down the inside (leave out for a bare wall). |
| `path` | The path from the back door to the gate: one block, or two laid alternately (stepping stones). |
| `lawn` | Noise patches over the ground, tried in order; the theme's ground where none matches. Each: `{noise: blotch\|hash, scale, salt, above, blocks, mix_salt, mix_above}`: `blocks[0]` where the noise is above `above`; with two blocks, `blocks[1]` where a second hash (`mix_salt`) is at or under `mix_above`. |
| `yard_wear` | Patches for a work yard's ground near the house (the theme's ground where none matches); leave out to use `lawn`. |
| `wall_props` | Props set against the house wall (a work yard's first is a woodpile). |
| `yard_extras` | A work yard's scattered extras. |
| `tufts` | `[{below, block}]`: on leftover lawn, the first entry whose `below` a per-cell hash is under; `$flower` picks from `flowers`. |
| `crops`, `flowers`, `pots` | Kitchen crops, flower bed and orchard flowers, a courtyard's pots. |
| `trees` | `{shade_trees, wood, soil}`: whether layouts other than courtyard (and zen, half the time) plant a shade tree; its wood (`oak`, `spruce`, `birch`, `dark_oak`, `cherry`); a block under its trunk. |
| `specials`, `special_chance` | Rare features (names of pieces in `features/`), picked evenly, and the percent of gardens that get one. |

Check a change with `cargo run --profile fast -- --gardens --culture <c>` before a town run.

## Features

`features/*.yaml` hold garden features: set pieces a garden places whole, like furniture (a well, a scarecrow, a shrine, the Japanese stone lantern). Files are merged; each key is a feature's name. Every block is checked when the pieces load, so a bad id fails at once.

A feature is written facing north, toward its house: `x` runs east, `z` south, and heights count up from the ground (`-1` is the ground's top block, `0` the first block above it). When built, it is turned so its north faces the house, and sided block states (`facing`, `axis`, a vine's `north=true`) turn with it.

| Key | What it is |
|---|---|
| `radius` | The clear square of bed it needs round its centre: `1` is 3x3, `2` is 5x5 (default 1). |
| `parts` | Its blocks, placed in order. Each part names cells (`at: [x, z]` or `cells: [[x, z], ...]`) and a `stack` of height -> block. A block is an id, or `{block: id, force: true}` to replace what is there (ground and blocks already placed are kept otherwise). |
| `chance` | On a part: percent each of its cells is built. |
| `one_of` | On a part, instead of `stack`: `[{weight, stack}, ...]`, one picked per cell. |
| `builtin` | A shape built in code: `bamboo_clump` (three to five bamboo stalks four to seven high). |

Ids may use the town's theme: `$wood` (wood family, as in `$wood_fence` or `$wood_stairs[facing=south]`), `$stone`, `$wall`, `$slab` (the theme's own blocks) and `$rock` (one of its rocks, picked per block).

Try every culture's features at once with `cargo run --profile fast -- --gardens --culture all --all-features`.
