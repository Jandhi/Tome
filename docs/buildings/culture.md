# Culture

`Culture` is the cultural style that drives palette selection and per-culture
defaults for roofs, windows, floors, and footprint shape. Defined in
`src/generator/buildings_v2/mod.rs`.

```rust
pub enum Culture {
    Medieval,
    Desert,
    Japanese,
}
```

It is threaded through the pipeline via `BuildingContext { culture, .. }` so
downstream modules can make style decisions without a growing parameter list.

## Current state

What is culturally differentiated today:

| Element | Medieval | Desert | Japanese |
|---|---|---|---|
| **Palette** (`palette_id`) | `medieval_spruce` | `desert_sandstone` | `japanese_dark_blackstone` |
| **Roof styles** (`roof_styles`) | Gable: Slab, Stairs, Double | Flat only | Gable: Stairs, Double |
| **Window fill** (`window_fill`) | Glass | Open (no glass) | Glass |
| **Jetty chance** (`jetty_chance`) | 2/3 | 0 | 0 |
| **Square bias** (`square_bias`) | 0 | 40% (square rects → domed roofs) | 0 |
| **Kitchen floor** (`floors/place.rs`) | Stone bricks | Glazed terracotta, 2×2 rotating | Stone bricks (fallback) |

Backing palette files under `data/palettes/`:

- **medieval/** — `medieval_spruce.json` (+ `roof/` subdir)
- **desert/** — `desert_sandstone.json`, `desert_prismarine.json`
- **japanese/** — `japanese_dark_blackstone.json`, `japanese_light_cherry.json`

Only one palette per culture is wired to `palette_id()`; the alternates
(`desert_prismarine`, `japanese_light_cherry`) exist as data but are not selected
by any culture method yet.

### Gaps

- **Japanese is the thinnest culture** — essentially "Medieval roofs with a dark
  palette." It has no jetty, no square bias, and falls through the `_` arm for
  kitchen floors.
- **Furnishing is not culture-aware at all.** `furnish/` has zero `Culture`
  references; furniture placement is identical across all three cultures.
- **Only `Kitchen` branches on culture** in `floors/place.rs`; all other floor
  types are uniform.

## Feature ideas

### Medieval
- **Timber pattern by social class** — drive `TimberPattern` off `SizeClass`:
  dense close-studding on Manors/Halls, sparser square-panel framing on Cottages.
- **Jettied gable + overhang brackets** — combine the existing jetty (2/3) with
  roof overhang brackets so jettied upper floors get decorative corner posts/braces.
- **Daub infill color** — wattle-and-daub panels in off-white/cream contrasting
  the timber; a second "infill" palette slot.
- **Exterior** — kitchen gardens with crop rows, low fences/hedges, a well or
  hay-bale stack, lanterns flanking the door.
- **Roof dormers** on steeper gable pitches for upper-floor light.

### Desert
- **Roof terraces** — flat roofs are walkable; add parapet walls, roof-access
  stair/ladder, and rooftop furniture (carpets as rugs, plant pots). The flat-roof
  path already gives the surface for free.
- **Courtyard footprints** — a footprint variant: a ring of rooms around an open
  central courtyard (fountain/well, sand + vegetation). Hooks into `square_bias`.
- **Arched openings** — doorways/windows shaped as arches (stairs/walls forming
  the curve) rather than rectangular cuts.
- **Wind-catcher towers** — a tall narrow square turret on one rect, capped open;
  the dome path already keys off square rects.
- **Awnings/sunshades** — colored wool/banner strips over windows.

### Japanese
The thinnest culture today — biggest opportunity to build a real identity.

- **Engawa (veranda)** — a raised wooden walkway/platform wrapping the ground
  floor with a low railing.
- **Deep curved/flared eaves** — exaggerate roof overhang and upturn the eave
  ends with stairs/slabs; pair with exposed rafter brackets.
- **Shoji-style walls** — wall infill as paper-screen grids: white concrete/quartz
  panels framed by dark wood lattice (trapdoors/fences). Makes `window_fill` and
  wall-infill culture-aware.
- **Tatami floors** — mat-grid floor pattern (alternating wool/carpet in a 1×2
  layout). Direct analog to the desert glazed-terracotta kitchen branch.
- **Genkan entry + step-up floor** — sunken entry cell at the door, interior floor
  one block higher.
- **Zen garden exterior** — raked sand (smooth sandstone/concrete), stones, a
  small pond, a stone lantern (toro) by the path.
- **Tokonoma alcove** — item frame + flower pot as a `RoomType` accent; low
  furniture and floor cushions.

## Cross-cutting systems

These have the highest payoff-to-effort — build once, every culture plugs in:

1. **Culture-aware furnishing.** `furnish/` has no `Culture` references today; a
   `culture` arm in furniture selection (bed style, lighting, decorative accents)
   would do more for cultural feel than anything else.
2. **Floor-type-by-culture table.** Generalize the `floors/place.rs` `Kitchen`
   match into a `culture × floor_type → material/pattern` lookup so tatami,
   terracotta, and flagstone all flow through one place.
3. **Exterior module** (pending — see `exterior.md`). Make gardens/fences/lighting/
   paths culture-keyed from day one: kitchen garden vs courtyard vs zen garden.
4. **Wall-infill palette slot.** A distinct infill material/color per culture
   (daub, plaster, shoji panel) — small data change, big visual payoff.
5. **Secondary palette selection.** Let `palette_id()` roll between a culture's
   palette variants (`desert_prismarine`, `japanese_light_cherry`) for street-level
   variety.
