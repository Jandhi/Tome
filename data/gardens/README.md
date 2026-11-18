# Garden styles

One file per culture (`medieval.yaml`, `desert.yaml`, `japanese.yaml`), read by
`GardenStyle::for_culture` (`src/generator/open_space/garden_style.rs`) and used
by `furnish_garden` for every town house's back garden. A file holds what a
culture's gardens are made of and how often each thing happens; the shapes
(beds, ponds, trees, walls) and where they go stay in code. Unknown keys are
refused, so a typo fails loudly.

Blocks in `fence` and `wall` are either a plain id (`"minecraft:bamboo_fence"`) or a role from the house's palette: `{role: primary_wood, form: fence}`, with an optional `fallback` id for a palette without that block. Roles are the material roles houses use (`primary_stone`, `secondary_stone`, `primary_wall`, `primary_wood`, `wood_pillar`, `primary_roof`, `secondary_roof`, ...); forms are `block`, `slab`, `stairs`, `wall`, `fence`, `fence_gate`, ... So a garden's fence and wall match its own house.

| Key | What it is |
|---|---|
| `layouts` | `[{layout, weight}]`: the layouts gardens are rolled from, in order (one entry: no roll). Layouts: `kitchen`, `flowers`, `orchard`, `work_yard`, `courtyard`, `zen`, `pond`, `moss`. A strip too shallow for beds is always a `work_yard`. |
| `walled` | When a garden is walled and closed (no gate, no garden path) instead of fenced: `{always: true}`, or `{big_house_area, big, other}` = percent chance behind houses whose frontage x depth is at least `big_house_area`, and behind the rest. |
| `pieces` | `{layout: [name, ...]}`: what each layout sets, in order. A name is a feature (below) or a group: `azalea_shrubs` (clipped shrubs against the fence), `palms` (date palms at the back), `pots` (pots about the bed, from `pots`). A layout left out sets its usual pieces: zen and pond `[stone_lantern, azalea_shrubs]`, moss `[bamboo, stone_lantern, azalea_shrubs]`, courtyard `[palms, bench, pots]`, the rest none. |
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

Features (`specials`) are built in code (`src/generator/open_space/garden_features.rs`), each from the house's own palette by material role: stone parts from `primary_stone` / `secondary_stone`, wooden ones from `primary_wood`, a shrine's roof from `primary_roof`. So any culture can list any feature and get it in its own materials: a fountain is sandstone in a desert district and stone brick in a medieval one.

| Name | What it is | Built from |
|---|---|---|
| `well` | Stone ring round a water hole, two posts, a little gable roof | `secondary_stone` (wall, block), `primary_wood` (fence, planks, stairs) |
| `scarecrow` | Scarecrow in a patch of wheat, looking at the house | `primary_wood` (fence) |
| `arbour` | Two-seat bench under a flowering arch, roses behind (needs 5x5) | `primary_wood` (stairs, fence) |
| `fountain` | Sunken basin with a lantern post (needs 5x5) | `primary_stone` (block, chiseled, wall) |
| `pergola` | Four posts under a slatted roof with vines, a pot in the shade | `primary_wood` (fence, slab), `primary_stone` (floor) |
| `tsukubai` | Water basin with a bamboo spout, rocks or ferns either side, on moss | the theme's rocks |
| `shrine` | Small shrine on a plinth facing the house, a candle in front | `primary_stone`, `wood_pillar`, `primary_roof` (stairs) |
| `bamboo` | Three to five bamboo stalks (set against the back fence, in a corner) | — |
| `bench` | Two-seat bench looking at the house (set against the back fence, in the middle) | `primary_stone` (stairs) |
| `stone_lantern` | A lantern on a stone post (the Japanese layouts set one) | `primary_stone` (wall) |

Try every culture's features at once with `cargo run --profile fast -- --gardens --culture all --all-features`, and a culture file of your own with `--gardens --culture <c> --culture-file <path>` (every street uses that file).
