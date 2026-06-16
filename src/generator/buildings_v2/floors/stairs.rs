//! Stair geometry, selection, and block rendering.
//!
//! Three stair kinds — Straight, Spiral (U-shaped, 2x2), LShaped — each with
//! their own position generators, fit checks, and block-placement patterns.
//! `select_stairwells` picks positions for inter-floor transitions + optional
//! attic entry; `place_stair_blocks` renders the chosen stairwells.

use std::collections::{HashMap, HashSet};

use crate::editor::Editor;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Placer};
use crate::geometry::{Cardinal, Point2D, Point3D, Rect2D};
use crate::minecraft::{Block, BlockForm};
use crate::noise::RNG;

use super::super::frame::Frame;
use super::super::pipeline::BuildCtx;
use super::super::walls::{segment_cells, OpeningKind, WallSegments};
use super::{StairKind, Stairwell};

// ---------------------------------------------------------------------------
// Geometry: position generators + fit checks per stair kind
// ---------------------------------------------------------------------------

/// Straight stair: position 0 is the corner landing, 1..=run are steps.
fn stair_positions(start: Point2D, direction: Cardinal, run: i32) -> Vec<Point2D> {
    let sv: Point2D = direction.into();
    (0..=run).map(|i| start + sv * i).collect()
}

/// Corner candidates for a straight stair: 8 (corner, direction) pairs,
/// 1 block inset from the rect edges.
fn corner_candidates(rect: &Rect2D) -> Vec<(Point2D, Cardinal)> {
    let min = rect.min();
    let max = rect.max();
    vec![
        (Point2D::new(min.x + 1, max.y - 1), Cardinal::East),
        (Point2D::new(min.x + 1, max.y - 1), Cardinal::North),
        (Point2D::new(min.x + 1, min.y + 1), Cardinal::East),
        (Point2D::new(min.x + 1, min.y + 1), Cardinal::South),
        (Point2D::new(max.x - 1, min.y + 1), Cardinal::West),
        (Point2D::new(max.x - 1, min.y + 1), Cardinal::South),
        (Point2D::new(max.x - 1, max.y - 1), Cardinal::West),
        (Point2D::new(max.x - 1, max.y - 1), Cardinal::North),
    ]
}

fn stair_fits_in_rect(start: Point2D, direction: Cardinal, run: i32, rect: &Rect2D) -> bool {
    let sv: Point2D = direction.into();
    let min = rect.min();
    let max = rect.max();
    for i in 0..=run {
        let p = start + sv * i;
        if !rect.contains(p) {
            return false;
        }
        if p.x <= min.x || p.x >= max.x || p.y <= min.y || p.y >= max.y {
            return false;
        }
    }
    true
}

/// U-stair positions: 2 steps toward the wall in `dir`, then 2 steps back
/// on the adjacent column. Anchor is the min corner of the 2x2.
fn spiral_positions(anchor: Point2D, dir: Cardinal) -> Vec<Point2D> {
    let (ax, az) = (anchor.x, anchor.y);
    match dir {
        Cardinal::North => vec![
            Point2D::new(ax, az + 1),
            Point2D::new(ax, az),
            Point2D::new(ax + 1, az),
            Point2D::new(ax + 1, az + 1),
        ],
        Cardinal::South => vec![
            Point2D::new(ax, az),
            Point2D::new(ax, az + 1),
            Point2D::new(ax + 1, az + 1),
            Point2D::new(ax + 1, az),
        ],
        Cardinal::East => vec![
            Point2D::new(ax, az),
            Point2D::new(ax + 1, az),
            Point2D::new(ax + 1, az + 1),
            Point2D::new(ax, az + 1),
        ],
        Cardinal::West => vec![
            Point2D::new(ax + 1, az),
            Point2D::new(ax, az),
            Point2D::new(ax, az + 1),
            Point2D::new(ax + 1, az + 1),
        ],
    }
}

/// 2x2 U-stair candidates at each corner of a rect. Requires at least 4x4.
fn spiral_anchors(rect: &Rect2D) -> Vec<(Point2D, Cardinal)> {
    let min = rect.min();
    let max = rect.max();
    if max.x - min.x < 4 || max.y - min.y < 4 {
        return vec![];
    }
    vec![
        (Point2D::new(min.x + 1, min.y + 1), Cardinal::North),
        (Point2D::new(max.x - 2, min.y + 1), Cardinal::North),
        (Point2D::new(min.x + 1, max.y - 2), Cardinal::South),
        (Point2D::new(max.x - 2, max.y - 2), Cardinal::South),
    ]
}

/// Which two walls a spiral/L-stair anchor is nearest to in its rect.
fn spiral_adjacent_walls(anchor: Point2D, rect: &Rect2D) -> [Cardinal; 2] {
    let min = rect.min();
    let max = rect.max();
    let x_wall = if anchor.x - min.x <= max.x - 1 - anchor.x {
        Cardinal::West
    } else {
        Cardinal::East
    };
    let z_wall = if anchor.y - min.y <= max.y - 1 - anchor.y {
        Cardinal::North
    } else {
        Cardinal::South
    };
    [x_wall, z_wall]
}

/// L-stair: 2 steps in primary direction, then 2 steps turning 90°.
fn l_stair_positions(start: Point2D, primary: Cardinal, turn: Cardinal) -> Vec<Point2D> {
    let pd: Point2D = primary.into();
    let td: Point2D = turn.into();
    let corner = start + pd;
    vec![
        start,
        corner,
        corner + td,
        corner + td * 2,
    ]
}

fn l_stair_candidates(rect: &Rect2D) -> Vec<(Point2D, Cardinal, Cardinal)> {
    let min = rect.min();
    let max = rect.max();
    vec![
        // SW corner: walk West into corner, turn South away
        (Point2D::new(min.x + 2, min.y + 1), Cardinal::West, Cardinal::South),
        // SW corner: walk North into corner, turn East away
        (Point2D::new(min.x + 1, min.y + 2), Cardinal::North, Cardinal::East),
        // SE corner: walk East into corner, turn South away
        (Point2D::new(max.x - 2, min.y + 1), Cardinal::East, Cardinal::South),
        // SE corner: walk North into corner, turn West away
        (Point2D::new(max.x - 1, min.y + 2), Cardinal::North, Cardinal::West),
        // NW corner: walk West into corner, turn North away
        (Point2D::new(min.x + 2, max.y - 1), Cardinal::West, Cardinal::North),
        // NW corner: walk South into corner, turn East away
        (Point2D::new(min.x + 1, max.y - 2), Cardinal::South, Cardinal::East),
        // NE corner: walk East into corner, turn North away
        (Point2D::new(max.x - 2, max.y - 1), Cardinal::East, Cardinal::North),
        // NE corner: walk South into corner, turn West away
        (Point2D::new(max.x - 1, max.y - 2), Cardinal::South, Cardinal::West),
    ]
}

fn positions_fit_in_rect(positions: &[Point2D], rect: &Rect2D) -> bool {
    let min = rect.min();
    let max = rect.max();
    positions.iter().all(|p| p.x > min.x && p.x < max.x && p.y > min.y && p.y < max.y)
}

// ---------------------------------------------------------------------------
// Selection: pick stair positions for each floor transition + attic
// ---------------------------------------------------------------------------

/// Select stairwells for all floor transitions plus (optionally) an attic stair.
/// Returns stairwells in ascending floor order, with no position overlaps.
pub(super) fn select_stairwells(
    frame: &Frame,
    wall_segs: &WallSegments,
    has_attic: bool,
    rng: &mut RNG,
) -> Vec<Stairwell> {
    let mut stairwells: Vec<Stairwell> = Vec::new();
    let mut occupied: HashSet<(i32, i32)> = HashSet::new();

    for floor in 0..frame.max_floors().saturating_sub(1) {
        // A flight on `floor` has its steps here and emerges onto `floor + 1`,
        // so it must keep clear of doorways on both.
        let mut door_cells = door_cells_on_floor(wall_segs, floor);
        door_cells.extend(door_cells_on_floor(wall_segs, floor + 1));

        // Prefer stacking straight onto the flight directly below so multi-floor
        // cores read as one continuous tower; fall back to a fresh footprint when
        // the stack doesn't fit or would block a door.
        // The flight that rises onto `floor` (placed last) — its footprint is the
        // hole in this floor's slab and its top step is where the player emerges.
        let below = stairwells.last().filter(|prev| prev.floor + 1 == floor);

        // A stacked continuation keeps the "one continuous tower" look, but only
        // accept it if it doesn't bury an interior-door approach lane — otherwise
        // fall through to a fresh pick (which may choose a ladder).
        let stacked = below
            .and_then(|prev| try_stack_on_previous(prev, frame, floor, &door_cells))
            .filter(|(_, positions, _)| {
                let lane = boundary_approach_cells(frame, floor);
                positions.iter().all(|p| !lane.contains(&(p.x, p.y)))
            });

        let chosen = stacked.or_else(|| {
            pick_stair_for_floor(frame, floor, wall_segs, &occupied, &door_cells, below, rng)
        });

        if let Some((kind, positions, direction)) = chosen {
            for pos in &positions {
                occupied.insert((pos.x, pos.y));
            }
            stairwells.push(Stairwell { positions, floor, direction, kind });
        }
    }

    // Attic stairwell: from top regular floor up through the ceiling.
    if has_attic && frame.max_floors() >= 1 {
        let top_floor = frame.max_floors() - 1;
        // The flight that rises onto the top floor — its top step is where the
        // player emerges, so an attic ladder must be reachable from it.
        let below = stairwells.last().filter(|prev| prev.floor + 1 == top_floor);
        if let Some((kind, positions, direction)) = pick_attic_stair(frame, wall_segs, &occupied, below, rng) {
            for pos in &positions {
                occupied.insert((pos.x, pos.y));
            }
            stairwells.push(Stairwell { positions, floor: top_floor, direction, kind });
        }
    }

    stairwells
}

/// Interior cells one step inward from every doorway on `floor` — the cells a
/// player stands on to use the door. A stair landing or step here walls the
/// door off, so these are excluded from every candidate's footprint.
fn door_cells_on_floor(wall_segs: &WallSegments, floor: u32) -> HashSet<(i32, i32)> {
    let mut cells = HashSet::new();
    for seg in wall_segs.segments_on_floor(floor) {
        // `seg.facing` is the wall's INWARD normal (points toward the interior),
        // so the cell a player stands on to use the door is `door_cell + facing`.
        let inward: Point2D = seg.facing.into();
        let seg_cells = segment_cells(seg);
        for o in &seg.openings {
            if !matches!(o.kind, OpeningKind::Door(_)) {
                continue;
            }
            for w in 0..o.width as usize {
                if let Some(&dc) = seg_cells.get(o.offset as usize + w) {
                    let c = dc + inward;
                    cells.insert((c.x, c.y));
                }
            }
        }
    }
    cells
}

/// The cells immediately on either side of every interior boundary wall active
/// on `floor` — the lane a player walks through to use the archway that will be
/// carved there later (in `build_rooms`). A stair that sits flush along a whole
/// boundary covers all of these, leaving the archway no clear slot and burying
/// the door in the stair. Stair selection *prefers* candidates that keep this
/// lane clear (see `pick_stair_for_floor`), so the archway always has somewhere
/// open to land.
fn boundary_approach_cells(frame: &Frame, floor: u32) -> HashSet<(i32, i32)> {
    use super::super::footprint::find_boundaries;
    let mut cells = HashSet::new();
    // A flight on `floor` emerges onto `floor + 1`; both floors' boundary lanes
    // matter, just like doorways.
    let ground = frame.footprint().rects();
    for f in [floor, floor + 1] {
        let active = frame.active_rects(f);
        // Keep the vec length == rect_count so `find_boundaries` indices line up
        // with `active_rects`; inactive rects get a ground-rect placeholder and
        // are filtered out by the active check below.
        let floor_rects: Vec<Rect2D> = (0..frame.rect_count())
            .map(|i| frame.rect_at(i, f).unwrap_or(ground[i]))
            .collect();
        for b in find_boundaries(&floor_rects) {
            if !active.contains(&b.rect_a) || !active.contains(&b.rect_b) {
                continue;
            }
            // The wall is colinear: constant x means it runs along z (approaches
            // are east/west), otherwise it runs along x (approaches north/south).
            let perp_x = b.wall_cells.first().map_or(true, |c0| {
                b.wall_cells.iter().all(|c| c.x == c0.x)
            });
            for c in &b.wall_cells {
                if perp_x {
                    cells.insert((c.x + 1, c.y));
                    cells.insert((c.x - 1, c.y));
                } else {
                    cells.insert((c.x, c.y + 1));
                    cells.insert((c.x, c.y - 1));
                }
            }
        }
    }
    cells
}

/// Try to continue the previous floor's flight as one straight run: same kind
/// and direction, with the new flight's landing sitting on the floor directly
/// above the previous flight's *top step* and its steps carrying on in the same
/// direction. This is the only stagger that keeps a full floor of clearance
/// between the two flights — a smaller offset (`d` cells) leaves only `run - d`
/// blocks of vertical gap at the cells they share in plan, so the upper flight's
/// underside drops into the lower flight's headroom (the steps visibly collide).
/// Continuing end-to-end needs a long core; when it doesn't fit, or a step would
/// block a doorway, we return None and a fresh-footprint pick is used instead.
///
/// (The cellar's `stacked_under_main_stair` can use a one-cell stagger because
/// its flight descends *below* the floor while the main flight rises above it —
/// they never share vertical space. Two ascending flights one story apart do.)
fn try_stack_on_previous(
    prev: &Stairwell,
    frame: &Frame,
    floor: u32,
    door_cells: &HashSet<(i32, i32)>,
) -> Option<(StairKind, Vec<Point2D>, Cardinal)> {
    if prev.kind != StairKind::Straight {
        return None;
    }
    if !(frame.active_rects(floor).contains(&0) && frame.active_rects(floor + 1).contains(&0)) {
        return None;
    }
    // The stair must fit in both floors it spans. Jetty only grows upward, so
    // the lower floor (`floor`) is the binding extent — use its rect.
    let core = frame.rect_at(0, floor)?;
    let run = (frame.wall_height() + 1) as i32;
    let dir = prev.direction;
    let sv: Point2D = dir.into();
    // Start the new flight at the previous flight's top step: its landing lands
    // one block above that step, and the steps continue past it. The two runs
    // then overlap only at that single handoff cell (a landing, which places no
    // block), so they never intrude on each other's headroom.
    let start = *prev.positions.last()?;
    if !stair_fits_in_rect(start, dir, run, &core) {
        return None;
    }
    let positions = stair_positions(start, dir, run);
    if positions.iter().any(|p| door_cells.contains(&(p.x, p.y))) {
        return None;
    }
    // Safety net: only the handoff landing (positions[0]) may coincide with the
    // previous flight; no actual step may re-enter its footprint.
    let prev_cells: HashSet<(i32, i32)> =
        prev.positions.iter().map(|p| (p.x, p.y)).collect();
    if positions[1..].iter().any(|p| prev_cells.contains(&(p.x, p.y))) {
        return None;
    }
    Some((StairKind::Straight, positions, dir))
}

/// The cell a player stands on to board a stairwell (its base), and the cells
/// its step blocks occupy (impassable on that floor). Straight stairs board from
/// the flat landing at `positions[0]`; spiral / L-shaped stairs have a step block
/// there, so the boarding cell is one back from `positions[0]` (opposite ascent).
fn stair_base_and_blocked(
    kind: StairKind,
    positions: &[Point2D],
    dir: Cardinal,
) -> (Point2D, HashSet<(i32, i32)>) {
    match kind {
        StairKind::Straight => {
            let blocked = positions[1..].iter().map(|p| (p.x, p.y)).collect();
            (positions[0], blocked)
        }
        _ => {
            let back: Point2D = (-dir).into();
            let blocked = positions.iter().map(|p| (p.x, p.y)).collect();
            (positions[0] + back, blocked)
        }
    }
}

/// Whether a stair's base on `floor` can be walked to from where the flight
/// below emerges — i.e. it isn't stranded by the hole that the flight below
/// punches in this floor's slab. `below` is the flight rising onto `floor`;
/// its footprint marks both the slab hole and (at its top step) the cell where
/// the player arrives. We flood-fill from that emergence cell across the core's
/// interior, treating the hole and this stair's own steps as impassable, and
/// require the base to be reached. Returns true when there is no flight below
/// (no hole) or the geometry can't be evaluated.
fn base_reachable_from_below(
    frame: &Frame,
    floor: u32,
    base: Point2D,
    blocked: &HashSet<(i32, i32)>,
    below: &Stairwell,
) -> bool {
    let Some(core) = frame.rect_at(0, floor) else { return true; };
    let Some(&emerge) = below.positions.last() else { return true; };
    let emerge = (emerge.x, emerge.y);
    let target = (base.x, base.y);

    // The handoff landing of a stacked flight sits on the flight below's top
    // step — that's exactly where you arrive, so it's reachable by definition.
    if target == emerge {
        return true;
    }

    let hole: HashSet<(i32, i32)> = below.positions.iter().map(|p| (p.x, p.y)).collect();
    // A cell is walkable if it's strictly inside the core (not an exterior wall),
    // isn't part of the slab hole, and isn't one of this stair's step blocks.
    let walkable = |c: (i32, i32)| -> bool {
        c.0 > core.min().x
            && c.0 < core.max().x
            && c.1 > core.min().y
            && c.1 < core.max().y
            && !hole.contains(&c)
            && !blocked.contains(&c)
    };
    // The base must itself be standable, else it's over the void or in a wall.
    if !walkable(target) {
        return false;
    }

    // Flood-fill from the emergence cell (the flight-below top step, which the
    // player stands on at this floor's level) into adjacent walkable cells.
    let mut seen: HashSet<(i32, i32)> = HashSet::from([emerge]);
    let mut stack = vec![emerge];
    while let Some(c) = stack.pop() {
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let n = (c.0 + dx, c.1 + dy);
            if n == target {
                return true;
            }
            if walkable(n) && seen.insert(n) {
                stack.push(n);
            }
        }
    }
    false
}

/// Pick a stair position for a specific floor transition.
/// Considers straight, spiral, and L-shaped candidates across the core rect only.
/// Prefers exterior-wall positions over interior, avoids door-facing walls.
/// When `below` is the flight rising onto this floor, candidates whose base is
/// stranded by that flight's slab hole are filtered out (re-picked) so the new
/// stair always connects to where the player emerges.
fn pick_stair_for_floor(
    frame: &Frame,
    floor: u32,
    wall_segs: &WallSegments,
    occupied: &HashSet<(i32, i32)>,
    door_cells: &HashSet<(i32, i32)>,
    below: Option<&Stairwell>,
    rng: &mut RNG,
) -> Option<(StairKind, Vec<Point2D>, Cardinal)> {
    let run = (frame.wall_height() + 1) as i32;

    // Stairs only in core rect — wings are too small and architecturally odd.
    // Constrain to the lower floor's extent (jetty grows upward, so the lower
    // side is the binding rect).
    let candidate_rects: Vec<usize> = if frame.active_rects(floor).contains(&0)
        && frame.active_rects(floor + 1).contains(&0)
    {
        vec![0]
    } else {
        vec![]
    };

    if candidate_rects.is_empty() {
        return None;
    }

    let mut door_facings: HashSet<Cardinal> = HashSet::new();
    for seg in wall_segs.segments_on_floor(floor) {
        if seg.openings.iter().any(|o| matches!(o.kind, OpeningKind::Door(_))) {
            // `wall_facing` (below) names the geometric *outward* side a stair
            // backs against; `seg.facing` is the wall's INWARD normal, so the
            // matching outward side is its negation. Comparing them in the same
            // frame is what actually keeps a stair off a door-bearing wall.
            door_facings.insert(-seg.facing);
        }
    }

    // Interior facings: sides of the core with adjacent wing rects on this
    // floor. Adjacency is computed at `floor` so jettied geometry stays in sync.
    let mut interior_facings: HashSet<Cardinal> = HashSet::new();
    let core_at_floor = frame.rect_at(0, floor)?;
    for i in 1..frame.rect_count() {
        let Some(wing) = frame.rect_at(i, floor) else { continue; };
        if wing.min().x == core_at_floor.max().x + 1 { interior_facings.insert(Cardinal::East); }
        if wing.max().x + 1 == core_at_floor.min().x { interior_facings.insert(Cardinal::West); }
        if wing.min().y == core_at_floor.max().y + 1 { interior_facings.insert(Cardinal::South); }
        if wing.max().y + 1 == core_at_floor.min().y { interior_facings.insert(Cardinal::North); }
    }

    let mut exterior: Vec<(StairKind, Vec<Point2D>, Cardinal)> = Vec::new();
    let mut interior: Vec<(StairKind, Vec<Point2D>, Cardinal)> = Vec::new();

    for &rect_idx in &candidate_rects {
        let Some(rect_owned) = frame.rect_at(rect_idx, floor) else { continue; };
        let rect = &rect_owned;
        let min = rect.min();

        // --- Straight stair candidates ---
        for (start, dir) in corner_candidates(rect) {
            if !stair_fits_in_rect(start, dir, run, rect) { continue; }
            let positions = stair_positions(start, dir, run);
            if positions.iter().any(|p| occupied.contains(&(p.x, p.y))) { continue; }
            if positions.iter().any(|p| door_cells.contains(&(p.x, p.y))) { continue; }
            let wall_facing = match dir {
                Cardinal::East | Cardinal::West => {
                    if start.y == min.y + 1 { Cardinal::North } else { Cardinal::South }
                }
                Cardinal::North | Cardinal::South => {
                    if start.x == min.x + 1 { Cardinal::West } else { Cardinal::East }
                }
            };
            if door_facings.contains(&wall_facing) { continue; }
            let candidate = (StairKind::Straight, positions, dir);
            if interior_facings.contains(&wall_facing) { interior.push(candidate); }
            else { exterior.push(candidate); }
        }

        // --- U-stair (spiral) candidates ---
        for (anchor, dir) in spiral_anchors(rect) {
            let positions = spiral_positions(anchor, dir);
            if positions.iter().any(|p| occupied.contains(&(p.x, p.y))) { continue; }
            if positions.iter().any(|p| door_cells.contains(&(p.x, p.y))) { continue; }
            let walls = spiral_adjacent_walls(anchor, rect);
            if walls.iter().all(|w| door_facings.contains(w)) { continue; }
            let candidate = (StairKind::Spiral, positions, dir);
            if walls.iter().any(|w| interior_facings.contains(w)) { interior.push(candidate); }
            else { exterior.push(candidate); }
        }

        // --- L-stair candidates ---
        for (start, primary, turn) in l_stair_candidates(rect) {
            let positions = l_stair_positions(start, primary, turn);
            if !positions_fit_in_rect(&positions, rect) { continue; }
            if positions.iter().any(|p| occupied.contains(&(p.x, p.y))) { continue; }
            if positions.iter().any(|p| door_cells.contains(&(p.x, p.y))) { continue; }
            let walls = spiral_adjacent_walls(start, rect);
            if walls.iter().any(|w| door_facings.contains(w)) { continue; }
            let candidate = (StairKind::LShaped, positions, primary);
            if walls.iter().any(|w| interior_facings.contains(w)) { interior.push(candidate); }
            else { exterior.push(candidate); }
        }
    }

    // Filter out candidates whose base is stranded by the flight below's hole.
    // Prefer connected exterior, then connected interior, then fall back to any
    // candidate (so a floor never loses its stair if the check rejects all —
    // which shouldn't happen for a normal open core).
    let connected = |cand: &(StairKind, Vec<Point2D>, Cardinal)| -> bool {
        match below {
            None => true,
            Some(b) => {
                let (base, blocked) = stair_base_and_blocked(cand.0, &cand.1, cand.2);
                base_reachable_from_below(frame, floor, base, &blocked, b)
            }
        }
    };
    let ext_ok: Vec<_> = exterior.iter().filter(|c| connected(c)).cloned().collect();
    let int_ok: Vec<_> = interior.iter().filter(|c| connected(c)).cloned().collect();

    // Steer away from runs that sit flush along an interior boundary wall: such
    // a run covers that boundary's whole archway lane, leaving the door (carved
    // later) no clear slot. A stair perpendicular to a wall only clips one lane
    // cell; one parallel to it covers the lot. So prefer the candidate(s) that
    // cover the *fewest* lane cells — leaving the rest of each boundary open for
    // its archway. Soft min, never empties the pool.
    let boundary_cells = boundary_approach_cells(frame, floor);
    let lane_cover = |cand: &(StairKind, Vec<Point2D>, Cardinal)| -> usize {
        cand.1.iter().filter(|p| boundary_cells.contains(&(p.x, p.y))).count()
    };
    let prefer_clear = |pool: Vec<(StairKind, Vec<Point2D>, Cardinal)>| {
        match pool.iter().map(|c| lane_cover(c)).min() {
            Some(min) => pool.into_iter().filter(|c| lane_cover(c) == min).collect(),
            None => pool,
        }
    };

    let mut candidates = if !ext_ok.is_empty() {
        prefer_clear(ext_ok)
    } else if !int_ok.is_empty() {
        prefer_clear(int_ok)
    } else if !exterior.is_empty() {
        prefer_clear(exterior)
    } else {
        prefer_clear(interior)
    };

    // If the best surviving stair still covers an interior-door approach lane —
    // or no stair fits at all — fall back to a 1x1 ladder, which never runs
    // along a lane. Only then: a clean stair is always preferred.
    let clips_or_empty = candidates.first().map_or(true, |c| lane_cover(c) > 0);
    if clips_or_empty {
        if let Some(ladder) =
            pick_ladder_for_floor(frame, floor, occupied, door_cells, &boundary_cells, below)
        {
            return Some(ladder);
        }
    }
    if candidates.is_empty() {
        return None;
    }

    let idx = rng.rand_i32_range(0, candidates.len() as i32) as usize;
    Some(candidates.swap_remove(idx))
}

/// Pick a 1x1 ladder cell connecting `floor` to `floor + 1`, used as a fallback
/// when no stair keeps the interior-door approach lanes clear. The cell sits one
/// step in from an exterior wall of the core (so a solid block backs the ladder)
/// and clears doorways, interior-boundary approach lanes, already-placed stair
/// footprints, and the flight-below hole. Facing points inward, away from the
/// backing wall. Returns None if no such cell exists (caller keeps the stair).
fn pick_ladder_for_floor(
    frame: &Frame,
    floor: u32,
    occupied: &HashSet<(i32, i32)>,
    door_cells: &HashSet<(i32, i32)>,
    boundary_cells: &HashSet<(i32, i32)>,
    below: Option<&Stairwell>,
) -> Option<(StairKind, Vec<Point2D>, Cardinal)> {
    let core = frame.rect_at(0, floor)?;
    // The ladder spans both floors, so the cell must be interior on the floor
    // above too (jetty grows upward — floor+1 is at least as large).
    let core_above = frame.rect_at(0, floor + 1)?;
    find_ladder_cell(frame, floor, &core, &core_above, occupied, door_cells, boundary_cells, below)
}

/// Shared ladder-cell search. Scans the inner ring of `core` — one step in from
/// an exterior wall, so a solid block backs the ladder — for a 1x1 cell that is
/// strictly interior on both `core` and `core_above`, clear of occupied/door/
/// boundary-lane cells, and reachable from the flight below. Returns the cell
/// with its inward facing (away from the backing wall), or None.
fn find_ladder_cell(
    frame: &Frame,
    floor: u32,
    core: &Rect2D,
    core_above: &Rect2D,
    occupied: &HashSet<(i32, i32)>,
    door_cells: &HashSet<(i32, i32)>,
    boundary_cells: &HashSet<(i32, i32)>,
    below: Option<&Stairwell>,
) -> Option<(StairKind, Vec<Point2D>, Cardinal)> {
    let (min, max) = (core.min(), core.max());

    // Scan every cell on the inner ring — one step in from an exterior wall, so a
    // solid block backs the ladder. Corners alone aren't enough: on a small core
    // the flight-below footprint and the boundary lane can claim all four, and
    // we'd wrongly fall back to a door-clipping stair. The ladder faces inward,
    // away from the wall it hangs on.
    for cell in core.iter() {
        // Must be strictly inside both floors' cores (not on an exterior wall),
        // and against exactly one exterior wall so it has a backing block.
        let inside = |r: &Rect2D| cell.x > r.min().x && cell.x < r.max().x
            && cell.y > r.min().y && cell.y < r.max().y;
        if !inside(core) || !inside(core_above) {
            continue;
        }
        // Facing = away from the backing exterior wall. Inner-ring cells touch a
        // wall on at least one side; interior cells touch none and are skipped.
        let dir = if cell.x == min.x + 1 { Cardinal::East }
            else if cell.x == max.x - 1 { Cardinal::West }
            else if cell.y == min.y + 1 { Cardinal::South }
            else if cell.y == max.y - 1 { Cardinal::North }
            else { continue; };

        let key = (cell.x, cell.y);
        if occupied.contains(&key)
            || door_cells.contains(&key)
            || boundary_cells.contains(&key)
        {
            continue;
        }
        // Don't get stranded by the flight-below hole (empty blocked set: the
        // ladder cell itself is walkable).
        if let Some(b) = below {
            if !base_reachable_from_below(frame, floor, cell, &HashSet::new(), b) {
                continue;
            }
        }
        return Some((StairKind::Ladder, vec![cell], dir));
    }
    None
}

/// Attic stairs sit at a gable-end corner and run along an eave wall (perpendicular
/// to the ridge). Straight-only since they fit naturally against the wall.
/// Rejects candidates whose positions overlap doorways on the floor the stair
/// sits on — critical for 1-story attic buildings where the attic stair shares
/// the same y-layer as the ground-floor door.
fn pick_attic_stair(
    frame: &Frame,
    wall_segs: &WallSegments,
    occupied: &HashSet<(i32, i32)>,
    below: Option<&Stairwell>,
    rng: &mut RNG,
) -> Option<(StairKind, Vec<Point2D>, Cardinal)> {
    let run = (frame.wall_height() + 1) as i32;
    // Attic stairs connect the top regular floor to the attic above it; both
    // share the top-floor extent (jettied if jetty is enabled).
    let top_floor = frame.max_floors().checked_sub(1)?;
    let rect_owned = frame.rect_at(0, top_floor)?;
    let rect = &rect_owned;

    let eave_dirs: &[Cardinal] = if rect.length() >= rect.width() {
        &[Cardinal::North, Cardinal::South]
    } else {
        &[Cardinal::East, Cardinal::West]
    };

    let door_cells = door_cells_on_floor(wall_segs, top_floor);

    let mut candidates: Vec<(StairKind, Vec<Point2D>, Cardinal)> = Vec::new();

    for (start, dir) in corner_candidates(rect) {
        if !eave_dirs.contains(&dir) { continue; }
        if !stair_fits_in_rect(start, dir, run, rect) { continue; }
        let positions = stair_positions(start, dir, run);
        if positions.iter().any(|p| occupied.contains(&(p.x, p.y))) { continue; }
        if positions.iter().any(|p| door_cells.contains(&(p.x, p.y))) { continue; }
        candidates.push((StairKind::Straight, positions, dir));
    }

    if candidates.is_empty() {
        return None;
    }

    // Prefer runs that cover the fewest interior-boundary archway-lane cells, so
    // the archway carved there later isn't buried in the stair (see
    // `boundary_approach_cells`). Soft: keeps the least-covering candidates.
    let boundary_cells = boundary_approach_cells(frame, top_floor);
    let lane_cover = |c: &(StairKind, Vec<Point2D>, Cardinal)| -> usize {
        c.1.iter().filter(|p| boundary_cells.contains(&(p.x, p.y))).count()
    };
    if let Some(min) = candidates.iter().map(|c| lane_cover(c)).min() {
        candidates.retain(|c| lane_cover(c) == min);
    }

    // If the best surviving eave stair still buries an archway lane, drop to a
    // 1x1 ladder instead — a ladder never runs along a lane (see
    // `pick_ladder_for_floor`). The attic shares the top floor's extent, so the
    // ladder's cell-above is that same rect. Only then: a clean stair wins.
    if candidates.first().map_or(true, |c| lane_cover(c) > 0) {
        let door_cells = door_cells_on_floor(wall_segs, top_floor);
        if let Some(ladder) = find_ladder_cell(
            frame, top_floor, rect, rect, occupied, &door_cells, &boundary_cells, below,
        ) {
            return Some(ladder);
        }
    }

    let idx = rng.rand_i32_range(0, candidates.len() as i32) as usize;
    Some(candidates.swap_remove(idx))
}

// ---------------------------------------------------------------------------
// Rendering: place stair blocks per StairKind
// ---------------------------------------------------------------------------

/// Render stair blocks for every stairwell: Straight uses a flat landing + run;
/// Spiral and LShaped fill below the ascending run with solid blocks and use
/// upside-down stairs for the descending/turning run. Clears 2 blocks of
/// headroom above each step.
pub(super) async fn place_stair_blocks(
    ctx: &mut BuildCtx<'_>,
    stairwells: &[Stairwell],
    frame: &Frame,
) {
    let editor: &Editor = &*ctx.editor;
    let material_id = ctx.palette
        .get_material(MaterialRole::PrimaryWood)
        .expect("No primary wood material")
        .clone();

    let mut placer_rng = ctx.rng.derive();
    let mut placer = MaterialPlacer::new(
        Placer::new(&ctx.data.materials, &mut placer_rng),
        material_id,
    );

    for sw in stairwells {
        let base_y = frame.floor_y(sw.floor);
        match sw.kind {
            StairKind::Straight => place_straight_stair(editor, &mut placer, sw, base_y).await,
            StairKind::Spiral   => place_spiral_stair(editor, &mut placer, sw, base_y).await,
            StairKind::LShaped  => place_l_stair(editor, &mut placer, sw, base_y).await,
            StairKind::Ladder   => place_ladder(editor, sw, frame).await,
        }
    }
}

/// Render a 1x1 ladder column from the floor it starts on up to the floor above.
/// The slab hole is already carved by `place_floors`; the ladder faces inward
/// (`sw.direction`), backed by the exterior wall opposite that facing.
async fn place_ladder(editor: &Editor, sw: &Stairwell, frame: &Frame) {
    let Some(&cell) = sw.positions.first() else { return; };
    let base_y = frame.floor_y(sw.floor);
    let top_y = frame.floor_y(sw.floor + 1);
    let facing = sw.direction.to_string();
    for y in base_y..top_y {
        let mut ladder = Block::from_id("minecraft:ladder".into());
        ladder.state = Some(HashMap::from([("facing".to_string(), facing.clone())]));
        editor
            .place_block_forced(&ladder, Point3D::new(cell.x, y, cell.y))
            .await;
    }
}

async fn place_straight_stair(
    editor: &Editor,
    placer: &mut MaterialPlacer<'_>,
    sw: &Stairwell,
    base_y: i32,
) {
    let facing_str = sw.direction.to_string();
    let facing_away_str = (-sw.direction).to_string();

    let stair_state = HashMap::from([
        ("facing".to_string(), facing_str),
    ]);
    let underside_state = HashMap::from([
        ("facing".to_string(), facing_away_str),
        ("half".to_string(), "top".to_string()),
    ]);

    // Position 0 is the landing; positions 1..=run are the steps.
    for (i, pos) in sw.positions.iter().enumerate() {
        if i == 0 {
            continue;
        }
        let step = (i - 1) as i32;
        let y = base_y + step;

        placer.place_block_forced(
            editor,
            Point3D::new(pos.x, y, pos.y),
            BlockForm::Stairs,
            Some(&stair_state),
            None,
        ).await;

        if step > 0 {
            placer.place_block(
                editor,
                Point3D::new(pos.x, y - 1, pos.y),
                BlockForm::Stairs,
                Some(&underside_state),
                None,
            ).await;
        }

        clear_headroom(editor, pos.x, y, pos.y).await;
    }
}

async fn place_spiral_stair(
    editor: &Editor,
    placer: &mut MaterialPlacer<'_>,
    sw: &Stairwell,
    base_y: i32,
) {
    // U-stair: steps 0,1 ascend toward wall; steps 2,3 return on the adjacent column.
    let forward = sw.direction;
    let back = -sw.direction;

    for (i, pos) in sw.positions.iter().enumerate() {
        let y = base_y + i as i32;
        let facing = match i {
            0 | 1 => forward,
            2 => {
                // Face away from the forward run.
                let toward = &sw.positions[1];
                match (toward.x - pos.x, toward.y - pos.y) {
                    (1, 0) => Cardinal::West,
                    (-1, 0) => Cardinal::East,
                    (0, 1) => Cardinal::North,
                    (0, -1) => Cardinal::South,
                    _ => back,
                }
            }
            _ => back,
        };

        let stair_state = HashMap::from([
            ("facing".to_string(), facing.to_string()),
        ]);

        placer.place_block_forced(
            editor,
            Point3D::new(pos.x, y, pos.y),
            BlockForm::Stairs,
            Some(&stair_state),
            None,
        ).await;

        if i < 2 {
            // Forward run: fill solid below.
            for fill_y in base_y..y {
                placer.place_block(
                    editor,
                    Point3D::new(pos.x, fill_y, pos.y),
                    BlockForm::Block,
                    None,
                    None,
                ).await;
            }
        } else if y > base_y {
            // Return run: upside-down stairs facing the forward run.
            let underside_state = HashMap::from([
                ("facing".to_string(), forward.to_string()),
                ("half".to_string(), "top".to_string()),
            ]);
            placer.place_block(
                editor,
                Point3D::new(pos.x, y - 1, pos.y),
                BlockForm::Stairs,
                Some(&underside_state),
                None,
            ).await;
        }

        clear_headroom(editor, pos.x, y, pos.y).await;
    }
}

async fn place_l_stair(
    editor: &Editor,
    placer: &mut MaterialPlacer<'_>,
    sw: &Stairwell,
    base_y: i32,
) {
    // L-stair: steps 0,1 primary direction; steps 2,3 turning 90°.
    let primary = sw.direction;
    let turn_dir = match (sw.positions[2].x - sw.positions[1].x,
                          sw.positions[2].y - sw.positions[1].y) {
        (1, 0) => Cardinal::East,
        (-1, 0) => Cardinal::West,
        (0, 1) => Cardinal::South,
        (0, -1) => Cardinal::North,
        _ => primary,
    };

    for (i, pos) in sw.positions.iter().enumerate() {
        let y = base_y + i as i32;
        let facing = if i < 2 { primary } else { turn_dir };

        let stair_state = HashMap::from([
            ("facing".to_string(), facing.to_string()),
        ]);

        placer.place_block_forced(
            editor,
            Point3D::new(pos.x, y, pos.y),
            BlockForm::Stairs,
            Some(&stair_state),
            None,
        ).await;

        if i < 2 {
            // First run: fill solid below.
            for fill_y in base_y..y {
                placer.place_block(
                    editor,
                    Point3D::new(pos.x, fill_y, pos.y),
                    BlockForm::Block,
                    None,
                    None,
                ).await;
            }
        } else if y > base_y {
            // Second run: upside-down stairs facing opposite the turn.
            let underside_state = HashMap::from([
                ("facing".to_string(), (-turn_dir).to_string()),
                ("half".to_string(), "top".to_string()),
            ]);
            placer.place_block(
                editor,
                Point3D::new(pos.x, y - 1, pos.y),
                BlockForm::Stairs,
                Some(&underside_state),
                None,
            ).await;
        }

        clear_headroom(editor, pos.x, y, pos.y).await;
    }
}

/// Clear 2 blocks of air above a stair step so the player has headroom.
async fn clear_headroom(editor: &Editor, x: i32, y: i32, z: i32) {
    for clear_y in (y + 1)..=(y + 2) {
        editor.place_block_forced(
            &"air".into(),
            Point3D::new(x, clear_y, z),
        ).await;
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::generator::buildings_v2::footprint::Footprint;
    use crate::generator::buildings_v2::footprint::merge::outline_from_rects;

    fn frame_with_core(core: Rect2D, floors: u32) -> Frame {
        let footprint = Footprint::new(outline_from_rects(&[core]), vec![core]);
        Frame::new(footprint, 64, vec![floors], 3)
    }

    fn straight_flight(start: Point2D, dir: Cardinal, run: i32, floor: u32) -> Stairwell {
        Stairwell {
            positions: stair_positions(start, dir, run),
            floor,
            direction: dir,
            kind: StairKind::Straight,
        }
    }

    #[test]
    fn stacked_flight_continues_end_to_end_without_overlap() {
        // Long core so a continued run fits. run = wall_height + 1 = 4.
        let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(20, 8));
        let frame = frame_with_core(core, 3);
        let prev = straight_flight(Point2D::new(1, 4), Cardinal::East, 4, 0);
        let doors = HashSet::new();

        let (kind, positions, dir) =
            try_stack_on_previous(&prev, &frame, 1, &doors).expect("should stack on a long core");

        assert_eq!(kind, StairKind::Straight);
        assert_eq!(dir, Cardinal::East);
        // New landing sits on the previous flight's top step (the handoff);
        // every actual step is past it, so no step shares a cell with prev.
        assert_eq!(positions[0], *prev.positions.last().unwrap());
        let prev_cells: HashSet<(i32, i32)> =
            prev.positions.iter().map(|p| (p.x, p.y)).collect();
        for step in &positions[1..] {
            assert!(
                !prev_cells.contains(&(step.x, step.y)),
                "stacked step {:?} re-enters the lower flight's footprint",
                step,
            );
        }
    }

    fn below_with(positions: Vec<Point2D>) -> Stairwell {
        Stairwell { positions, floor: 0, direction: Cardinal::East, kind: StairKind::Straight }
    }

    #[test]
    fn base_reachable_across_open_floor() {
        // Partial hole line; the base sits in the open part of the floor.
        let frame = frame_with_core(Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 8)), 3);
        let below = below_with(vec![
            Point2D::new(4, 1), Point2D::new(4, 2), Point2D::new(4, 3),
            Point2D::new(4, 4), Point2D::new(4, 5),
        ]);
        assert!(base_reachable_from_below(
            &frame, 1, Point2D::new(6, 3), &HashSet::new(), &below,
        ));
    }

    #[test]
    fn base_stranded_when_boxed_by_hole() {
        // Base in a corner walled off by the hole on its two open sides, with
        // the emergence cell far away — unreachable.
        let frame = frame_with_core(Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 8)), 3);
        let below = below_with(vec![
            Point2D::new(2, 1), Point2D::new(1, 2), Point2D::new(7, 7),
        ]);
        assert!(!base_reachable_from_below(
            &frame, 1, Point2D::new(1, 1), &HashSet::new(), &below,
        ));
    }

    #[test]
    fn stacked_handoff_landing_is_always_reachable() {
        // A stacked flight's landing sits on the flight-below top step (the
        // emergence cell), so it is reachable by definition.
        let frame = frame_with_core(Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 8)), 3);
        let below = below_with(vec![
            Point2D::new(1, 4), Point2D::new(2, 4), Point2D::new(3, 4),
            Point2D::new(4, 4), Point2D::new(5, 4),
        ]);
        // Base == the flight-below top step.
        assert!(base_reachable_from_below(
            &frame, 1, Point2D::new(5, 4), &HashSet::new(), &below,
        ));
    }

    #[test]
    fn stacked_flight_rejected_when_run_would_not_fit() {
        // Short core: continuing end-to-end from the previous top step runs off
        // the rect, so stacking must bail (caller falls back to a fresh pick).
        let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(7, 8));
        let frame = frame_with_core(core, 3);
        let prev = straight_flight(Point2D::new(1, 4), Cardinal::East, 4, 0);
        let doors = HashSet::new();

        assert!(try_stack_on_previous(&prev, &frame, 1, &doors).is_none());
    }
}
