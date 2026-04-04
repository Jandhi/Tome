# Floors Module

Places floor and ceiling slabs for each story. Handles vertical circulation
with three stair types. Returns a `FloorPlan` consumed by rooms and furnish.

## Input
- `Frame` (footprint polygon + floor count + wall heights + base Y)
- `WallSegments` (needed to avoid placing stairs against door-facing walls)
- `has_attic: bool` (whether to place an attic stair from top floor through ceiling)
- `LoadedData`, `Palette`, `RNG`

## Output
- `FloorPlan` — stairwell positions, consumed by rooms module

```rust
struct FloorPlan {
    stairwells: Vec<Stairwell>,
}

struct Stairwell {
    positions: Vec<Point2D>,  // (x,z) cells occupied
    floor: u32,               // starts on this floor, goes up to floor+1
    direction: Cardinal,      // ascent direction (straight) or initial facing (spiral)
    kind: StairKind,
}

enum StairKind { Straight, Spiral, LShaped }
```

`FloorPlan::stairwells_on_floor(floor)` filters by starting floor.

## Floor Surface Placement

```
  ceiling slab (roof_y - 2) ────→  ════════════   per-rect top
                                    |          |
                                    | floor 1  |   3 blocks of air
                                    |          |
  floor 1 slab (floor_y(1)-1) ──→  ════════════
                                    |          |
                                    | floor 0  |   3 blocks of air
                                    |          |
  floor 0 slab (floor_y(0)-1) ──→  ════════════   (overwrites foundation stone)
  foundation ────────────────────→  ▓▓▓▓▓▓▓▓▓▓▓▓
```

**Floor slabs** are placed at `floor_y(f) - 1` for all interior cells on each floor.
Skips:
- **Perimeter cells** — exterior wall positions (computed from outline + concave corners)
- **Stairwell openings** — cells directly above a stairwell on the floor below

Material: `PrimaryWood` (block form) for all floors including ground.

**Ceiling slabs** are placed at `roof_y(rect) - 2` for each rect. Each rect gets
its own ceiling at its roof height. Skips perimeter cells. Uses a `placed` set to
avoid double-placement where rects overlap.

## Stair Types

Three stair geometries, all fitting into corners of the core rect:

### Straight Stair

```
  Top-down:                Side view (ascending East):

  ┌─wall─────────┐        ceiling ════════════
  │ L  1  2  3  4│            4 ──╱
  │               │          3 ──╱
  │               │        2 ──╱
  └───────────────┘      1 ──╱
                         L ════ floor
  L = landing (no step)
  1-4 = stair steps (run = wall_height + 1)
```

- Position 0 is a corner landing (no stair block placed)
- Positions 1..=run are ascending steps
- Each step: stair block at `base_y + (i-1)` facing the ascent direction
- Underside: upside-down stair facing backward below each step (except first)
- Clears 2 blocks of air above each step for headroom
- Run length = `wall_height + 1` = 4 blocks

### Spiral (U-Stair)

```
  Top-down (2×2, ascending North):

  ┌─wall─────────┐
  │ 0  1         │     Step 0 → Step 1 (toward wall)
  │ 3  2         │     Step 2 → Step 3 (return column)
  │               │
  └───────────────┘

  Side view:
         3
       2
     1
   0
  ════ floor
```

- 4 cells in a 2×2 pattern: steps 0,1 go toward the wall, steps 2,3 come back
- Each step is 1 Y higher than the last
- Forward run (steps 0,1): fill solid wood blocks below the stair
- Return run (steps 2,3): upside-down stair facing forward below
- Step 2 facing is computed from adjacency to step 1 (faces away from the forward run)
- Clears 2 blocks of air above each step

### L-Shaped Stair

```
  Top-down (primary=West, turn=South):

  ┌─wall─────────┐
  │ 1  0         │     Steps 0,1: walk toward corner (West)
  │    2         │     Steps 2,3: turn away (South)
  │    3         │
  └───────────────┘
```

- 4 positions: 2 steps in primary direction, then 2 steps turning 90°
- First run (steps 0,1): fill solid wood below
- Second run (steps 2,3): upside-down stair facing opposite of turn direction
- Clears 2 blocks of air above each step

## Stair Placement Strategy

### Candidate Generation

All three stair types generate candidates at the corners of the core rect only
(wings are too small and stairs there feel architecturally odd).

```
  Core rect corner candidates:

  NW───────────NE        Each corner generates:
  │             │        - 2 straight candidates (2 perpendicular directions)
  │    core     │        - 1 spiral anchor (if rect ≥ 4×4)
  │             │        - 2 L-stair candidates (2 turn directions)
  SW───────────SE
```

**Straight**: `corner_candidates(rect)` — 8 candidates (4 corners × 2 directions).
Start positions are 1 block inset from rect edges. Validated via `stair_fits_in_rect()`.

**Spiral**: `spiral_anchors(rect)` — 4 candidates (4 corners). Requires rect ≥ 4×4.
The anchor is the min corner of the 2×2.

**L-shaped**: `l_stair_candidates(rect)` — 8 candidates (4 corners × 2 turn directions).
Each starts 2 blocks from a corner, walks toward it, then turns away.

### Filtering

Candidates are filtered by:
1. **Occupied positions** — no overlap with previously placed stairwells
2. **Door avoidance** — never place stairs against a wall that has a door
   (checks which facings have doors on this floor)
3. **Interior vs exterior walls** — candidates near wings (interior facings)
   are deprioritized; prefer exterior walls

### Selection

Candidates are split into exterior-wall and interior-wall groups. Prefer exterior.
Within the chosen group, pick randomly via RNG.

Stairs are placed floor-by-floor (0 to max_floors-2), each time updating the
occupied set so the next floor's stairs don't overlap.

### Attic Stairs

When `has_attic` is true and `max_floors >= 1`, an additional stair is placed from
the top regular floor into the attic space under the roof.

Attic stairs are **straight only**, placed at a **gable end** (perpendicular to the
ridge axis). This positions them where the roof is tallest, giving the most headroom.

```
  Ridge runs along Z (longer dimension):

  ┌───────────────┐        Attic stair goes N or S
  │  stair→  N    │        (perpendicular to ridge,
  │               │         starting at gable corner)
  │    ridge      │
  │               │
  └───────────────┘
```

## Post-Roof: clear_attic_stair_headroom()

The roof module places blocks that overwrite the air cleared by `place_floors`.
This async function runs **after** `place_roof` to re-carve headroom:

- For each attic stairwell (floor == top_floor):
  - For straight stairs: also clears the ceiling slab above the first step
    so the player can access the stairs
  - Clears 2 blocks of air above each step position through the roof blocks

## Material

All floor slabs, ceiling slabs, and stair blocks use `PrimaryWood` (block form for
slabs, stairs form for steps). Headroom clearing uses hardcoded `air` blocks.
