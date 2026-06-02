# Jetty Phase 3 — Multi-Rect Overhangs

Plan, drafted 2026-06-02. Branch: `jd/houses`.

Phase 2 (`apply_jetty`, `src/generator/buildings_v2/frame/mod.rs:263`) jetties **single-rect**
buildings only: it grows every upper floor's extent by 1 block on all four sides, gated by
`plot_bounds.contains_rect`. Multi-rect buildings (Manor, Hall — and any House/Cottage that rolls
a wing) fall through the `if frame.rect_count() != 1 { return frame; }` guard and stay flush.

Phase 3 extends jettying to multi-rect frames. A live spot-check (`build_jetty_manors_halls_live`)
confirmed the status quo: 6 Manors/Halls across distinct seeds, all `rects=3..4`, all `flush`.

---

## Why this is mostly a geometry change

The plumbing already exists. `Frame` stores **per-rect, per-floor extents** in
`rect_extents[rect][floor]` (`frame/mod.rs:31`), and every downstream stage reads geometry through
that, not through the raw footprint:

- `rect_at(i, floor)`, `rect_at_top(i)`, `outline_at_floor(floor)`, `filled_points_at_floor(floor)`
- Walls build per-floor from `outline_at_floor`; floors from `filled_points_at_floor`; roof from
  `rect_at_top`; stairs from `rect_at(i, floor)` (jetty-aware since the `td/resource_chain` merge).
- Wall corner posts already handle **upper-only "jetty overhang" corners** generically by floor
  range (`walls/mod.rs:1197-1208`) — driven by per-floor outline corners, not by `rect_count`.

So if `apply_jetty` produces correct multi-rect extents, the consumers largely honor them already.
The work is the extent computation plus verifying/patching a few geometry-driven edge cases.

---

## The compensation problem

Naively growing every rect by 1 on all sides makes adjacent rects **overlap at their shared seam**
(a wing grown toward the core collides with the core) and breaks `outline_from_rects`.

The rule: **only grow exterior edges; leave shared edges flush.**

```
Single-rect (Phase 2)            Multi-rect (Phase 3)
  ┌─────────┐  grow all            ┌──────────┐
  │ ground  │  4 sides             │  core    │  core: grow N/E/W, NOT south (shared seam)
  └─────────┘                      └────┬─────┘  wing: grow S/E/W, NOT north (shared seam)
                                     ┌──┴───┐
                                     │ wing │
                                     └──────┘
```

Shared edges stay aligned → no overlap, seams tile cleanly, exterior facades bulge out.

---

## Algorithm

1. **Derive shared sides per rect.** `find_boundaries(rects)` (`footprint/mod.rs:168`) already
   returns adjacent pairs and their direction. Map each boundary to the two sides it consumes
   (East boundary ⇒ `rect_a` East + `rect_b` West, etc.) to build `Vec<HashSet<Cardinal>>`.
2. **Grow each rect's extent.** For floors ≥ 1, expand every *non-shared* side outward by 1; leave
   shared sides flush. The result is still a single `Rect2D` per rect per floor, so there are no
   inter-rect overlaps and seams stay aligned.
3. **Per-rect floor counts.** Grow only a rect's own upper floors (`floor >= 1` and
   `floor < floor_counts[i]`); ground floor (`floor == 0`) stays at the footprint extent. A
   1-floor wing never grows. Single-step, not cumulative per floor.
4. **Plot-bounds gate.** Require the *union* of grown extents to fit `plot_bounds` (replaces the
   single-rect `contains_rect`). If it doesn't fit, fall back to the un-jettied frame unchanged.
5. **Unify the paths.** Drop the `rect_count() != 1` early return — single-rect becomes the trivial
   case (no shared sides → grows all four → byte-identical to Phase 2 output).

Build the new extents with `Frame::with_per_floor_extents` (`frame/mod.rs:81`), exactly as Phase 2
does.

---

## Design decisions

- **Adjacency from the ground-floor footprint, applied to all floors.** A core wall standing above
  a *dropped-out* wing (wing has fewer floors) stays flush rather than jutting out over the wing's
  roof. Computed once from `footprint.rects()` rather than per-floor.
- **Partial-edge adjacency → conservative.** If a side abuts another rect *at all* (even partially,
  via the `z_start..z_end` overlap span in `find_boundaries`), don't grow that whole side. Keeps
  each extent a single rectangle. Cost: a long, mostly-exterior wall with one small abutting wing
  won't overhang. Acceptable for v1; a later pass could split such an edge into sub-rects.

---

## Edge cases to verify (expected to be small fixes, not redesigns)

- **Roof bounding box** (`roof/mod.rs:236-241`): grown tops enlarge each rect's roof bbox. The
  gable-overhang suppression (`roof/mod.rs:206`) keys off top-floor adjacency between rects — it
  should still fire because shared sides stay flush, so rects still abut at the seam. Verify on L/T
  shapes; watch for the taller-core overhang clipping a shorter wing's roof.
- **Re-entrant corners** (L/T/U exteriors): the overhang corner posts and
  `check_building_invariants` (`rooms/mod.rs`) must still hold — every interior-edge cell needs a
  wall outside it, every `BlockedReachable` cell a walkable neighbour.
- **Jetty underside / support**: geometry-driven corner posts (`walls/mod.rs:1197`) should already
  cover exterior overhang corners; confirm visually that nothing drops logs into the air below.

---

## Implementation steps

1. `shared_sides(rects: &[Rect2D]) -> Vec<HashSet<Cardinal>>` helper (in `footprint` next to
   `find_boundaries`, or private in `frame`).
2. Rewrite `apply_jetty` per the algorithm above; remove the `rect_count() != 1` guard.
3. Replace the plot gate with a union-fits-`plot_bounds` check.
4. **Unit tests** (`frame/test.rs`, alongside the existing `apply_jetty_*`):
   - L-shape (core + 1 wing): assert each rect's seam side stays flush and its exterior sides grow
     by 1 at floor 1; ground floor unchanged.
   - U/T shape (core + 2 wings): seam handling on both wings.
   - 1-floor wing stays flush on every floor while a 2-floor core grows.
   - Plot-overflow → no-op (union exceeds bounds).
   - Single-rect still grows all four sides (regression: Phase 2 parity).
5. Extend `pipeline_invariants_property_test_jetty` (`rooms/test.rs:1139`) to include
   `Manor`/`Hall` size classes with the invariant checker (currently Cottage/House/Hall).
6. Offline blueprint dump of a multi-rect jetty (`build_furnished_jetty_houses_offline` variant or
   reuse) to eyeball per-floor overhang in the ASCII/SVG output under `output/`.
7. Live: re-run `build_jetty_manors_halls_live` (`rooms/test.rs`) — signs should flip
   `flush` → `JETTY`. Iterate on any roof/wall artifacts found in-world.

---

## Done when

- Multi-rect frames grow exterior edges on upper floors with seams flush and no overlaps.
- All `apply_jetty_*` unit tests + the extended jetty property/invariant test pass.
- `build_jetty_manors_halls_live` reports `JETTY` for multi-rect buildings and they render cleanly
  in-world (walls, roof, overhang underside).
- Single-rect output is unchanged from Phase 2.

---

## Implementation outcome (2026-06-02)

Done and verified. The deltas from the plan above:

- **`growable_sides` returns a `GrowMask` struct, not `Vec<HashSet<Cardinal>>`.** Four bools per rect
  (west/east/north/south) is simpler than hashing `Cardinal` and avoids relying on `Cardinal`'s
  commented-out `Into<Point2D>`. Lives private in `frame/mod.rs`.
- **An overlap guard was needed in addition to the plot gate.** The conservative per-side rule stops
  a rect growing *into* an existing neighbour, but two rects with a 1-cell gap on the same side can
  still grow *toward each other* and overlap. `apply_jetty` now bails the whole building to flush if
  any two upper-floor extents would share a cell (adjacent rects only touch, so they pass).
- **The real work was a consistency fix in `rooms/mod.rs`, not `frame`.** The first test run failed
  exactly as the "re-entrant corners" edge case warned: `compute_room_interior` already used grown
  per-floor extents, but the *interior* partition + phantom walls were both placed (`build_rooms`)
  and checked (`wall_cells_on_floor`) from the **ground** rects — so on a jettied floor the walls
  landed one cell off from where the grown rooms expected them, tripping invariant (a). Fix: derive
  `find_boundaries` / `phantom_wall_cells` from per-floor extents in both spots. This is a no-op when
  jetty is off (`rect_at(i, floor)` equals the ground rect then), so the flush path is unchanged.
- **Exterior walls, roof, and overhang corner posts needed no changes** — they were already
  per-floor/outline-driven, as the plan predicted.

Test coverage landed: `apply_jetty_multi_rect_grows_only_open_sides`,
`apply_jetty_u_shape_grows_three_open_sides_each`, `apply_jetty_one_floor_wing_stays_flush`,
`apply_jetty_noop_when_multi_rect_grown_exceeds_plot` (`frame/test.rs`), and
`pipeline_invariants_property_test_jetty_multirect` (`rooms/test.rs`, asserts jetty actually fired so
it can't pass vacuously). Live: 5/6 Manors/Halls jetty in-world; the 6th gracefully falls back.

Possible follow-ups (not done): partial-edge sub-rect splitting (the conservative whole-side block
under-jetties long mostly-exterior walls), and per-rect graceful fallback instead of whole-building
bail on plot/overlap.
