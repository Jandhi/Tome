//! Below-ground cellars.
//!
//! A cellar is a single story carved beneath the core rect, one floor below
//! `base_y`. Unlike above-ground floors it lives outside the `0..max_floors`
//! index range, so it reuses the floor-indexed APIs via `Frame::CELLAR_FLOOR`
//! (see `frame::floor_y`). The cellar is fully enclosed — stone floor slab
//! below, stone retaining walls around the core perimeter, and the existing
//! ground-floor slab as its ceiling — then connected to the ground floor by a
//! straight descending staircase and furnished from the `storage` room list.
//!
//! Excavation is the one genuinely new operation versus the rest of the
//! pipeline: every other module places blocks into air, while this one carves
//! into existing terrain. Eligibility is therefore gated on both size class and
//! a terrain-dryness check so we never open a cellar into a lake or lava.

use std::collections::{HashMap, HashSet};

use crate::editor::Editor;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Placer};
use crate::geometry::{Cardinal, Point2D, Point3D, Rect2D};
use crate::minecraft::BlockForm;
use crate::noise::RNG;

use super::floors::{FloorPlan, StairKind};
use super::footprint::{Footprint, SizeClass};
use super::frame::{Frame, CELLAR_FLOOR};
use super::pipeline::BuildCtx;
use super::rooms::{
    compute_room_interior, CellState, ConstraintMap, Room, RoomPlan, RoomRole,
};
use super::furnish::furnish_rooms;
use super::walls::{segment_cells, WallSegments};
use super::RoomType;

/// Roll cellar eligibility by size class. Larger, grander buildings are far
/// more likely to have a cellar; a cottage only rarely gets a root cellar.
fn rolls_cellar(size_class: SizeClass, rng: &mut RNG) -> bool {
    match size_class {
        SizeClass::Cottage => rng.chance(1, 6),
        SizeClass::House => rng.chance(2, 5),
        SizeClass::Hall => rng.chance(4, 5),
        SizeClass::Manor => true,
    }
}

/// True if any cell in the cellar volume is liquid in the *current* world.
/// Sampling uses `try_get_block`, which returns `None` on synthetic/offline
/// worlds — those are always treated as dry so offline tests build cellars.
fn volume_is_wet(editor: &Editor, interior: &Rect2D, floor_y: i32, ceiling_y: i32) -> bool {
    let water = "minecraft:water".into();
    let lava = "minecraft:lava".into();
    for p in interior.iter() {
        for y in (floor_y - 1)..ceiling_y {
            if let Some(block) = editor.try_get_block(Point3D::new(p.x, y, p.y)) {
                if block.id == water || block.id == lava {
                    return true;
                }
            }
        }
    }
    false
}

/// Straight-stair corner candidates: 8 (start, ascend-direction) pairs inset
/// one cell from the rect edges. Mirrors `floors::stairs::corner_candidates`.
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

/// Whether a straight stair of `run` steps from `start` ascending in `dir`
/// stays strictly inside the rect (never on the perimeter wall ring).
fn stair_fits(start: Point2D, dir: Cardinal, run: i32, rect: &Rect2D) -> bool {
    let sv: Point2D = dir.into();
    let min = rect.min();
    let max = rect.max();
    for i in 0..=run {
        let p = start + sv * i;
        if p.x <= min.x || p.x >= max.x || p.y <= min.y || p.y >= max.y {
            return false;
        }
    }
    true
}

/// Distance past which a target (door or stair cell) no longer counts against a
/// candidate — beyond this the stairwell is plainly clear of it, so we cap the
/// per-factor contribution and let the other factors decide.
const CLEAR_CAP: i32 = 6;

/// Candidates scoring within this many points of the best are all eligible for
/// the final random pick, so equally-good corners still vary between buildings.
const SCORE_TOLERANCE: i32 = 1;

/// Smallest Manhattan distance from any stairwell footprint cell to any target
/// cell, capped at `CLEAR_CAP`. Returns `CLEAR_CAP` when there are no targets
/// (nothing to avoid) and `0` when the footprint sits right on a target.
fn min_clearance(footprint: &[Point2D], targets: &[Point2D]) -> i32 {
    let mut best = CLEAR_CAP;
    for f in footprint {
        for t in targets {
            let d = (f.x - t.x).abs() + (f.y - t.y).abs();
            if d < best {
                best = d;
            }
        }
    }
    best
}

/// Pick a straight descending stair within the core rect by scoring every
/// fitting corner candidate on how far its ground-floor stairwell opening stays
/// from (a) ground-floor doorway cells and (b) the main staircase, then randomly
/// choosing among the highest scorers. Candidates whose opening would land
/// directly on a doorway cell or overlap the main stair are rejected outright.
/// Returns (positions, ascend-direction) where positions[0] is the bottom
/// landing on the cellar floor and positions[1..=run] ascend toward the ground.
fn pick_stair(
    rect: &Rect2D,
    run: i32,
    door_cells: &[Point2D],
    stair_cells: &[Point2D],
    rng: &mut RNG,
) -> Option<(Vec<Point2D>, Cardinal)> {
    let mut scored: Vec<(i32, Vec<Point2D>, Cardinal)> = Vec::new();
    for (start, dir) in corner_candidates(rect) {
        if !stair_fits(start, dir, run, rect) {
            continue;
        }
        let sv: Point2D = dir.into();
        let positions: Vec<Point2D> = (0..=run).map(|i| start + sv * i).collect();
        // positions[0] is the cellar landing; positions[1..] are the cells whose
        // slab is cut away, so they're what actually opens onto the floor above.
        let footprint = &positions[1..];

        let stair_clear = min_clearance(footprint, stair_cells);
        if stair_clear == 0 {
            continue; // would collide with the main staircase
        }
        let door_clear = min_clearance(footprint, door_cells);
        if door_clear == 0 {
            continue; // opening would sit directly in a doorway cell
        }
        scored.push((door_clear + stair_clear, positions, dir));
    }
    if scored.is_empty() {
        return None;
    }
    let best = scored.iter().map(|(s, _, _)| *s).max().unwrap();
    let mut top: Vec<(Vec<Point2D>, Cardinal)> = scored
        .into_iter()
        .filter(|(s, _, _)| *s >= best - SCORE_TOLERANCE)
        .map(|(_, p, d)| (p, d))
        .collect();
    let idx = rng.rand_i32_range(0, top.len() as i32) as usize;
    Some(top.swap_remove(idx))
}

/// Ground-floor doorway cells the cellar stair must dodge: the interior cell one
/// step inward from each floor-0 door opening (where someone stands entering).
fn ground_floor_door_cells(wall_segs: &WallSegments) -> Vec<Point2D> {
    let mut cells = Vec::new();
    for (seg, opening) in wall_segs.doors() {
        if seg.floor != 0 {
            continue;
        }
        let seg_cells = segment_cells(seg);
        let inward: Point2D = (-seg.facing).into();
        for w in 0..opening.width as usize {
            let idx = opening.offset as usize + w;
            if let Some(&door_cell) = seg_cells.get(idx) {
                cells.push(door_cell + inward);
            }
        }
    }
    cells
}

/// Core-side approach cells for every floor-0 interior doorway that borders the
/// core. These inter-rect (core↔wing) doors live in the room plan, not in
/// `wall_segs`, so without this the cellar stair can run straight across a
/// wing's doorway. Each door cell sits on a core edge; the approach is the
/// neighbouring cell just inside the core.
fn interior_door_approach_cells(room_plan: &RoomPlan, core: &Rect2D) -> Vec<Point2D> {
    let min = core.min();
    let max = core.max();
    let mut cells = Vec::new();
    for &(floor, rect_a, rect_b, dc) in &room_plan.interior_doors {
        if floor != 0 || (rect_a != 0 && rect_b != 0) {
            continue;
        }
        let approach = if dc.y == min.y {
            Point2D::new(dc.x, dc.y + 1)
        } else if dc.y == max.y {
            Point2D::new(dc.x, dc.y - 1)
        } else if dc.x == min.x {
            Point2D::new(dc.x + 1, dc.y)
        } else if dc.x == max.x {
            Point2D::new(dc.x - 1, dc.y)
        } else {
            continue;
        };
        cells.push(approach);
    }
    cells
}

/// If the ground floor has a straight main staircase, return a cellar stair that
/// stacks beneath it — same ascend direction, but shifted one cell along the
/// travel axis so the cellar steps stagger against the main steps rather than
/// landing directly underneath them. The two flights then read as one continuous
/// stair core (descend the main flight, then the cellar flight below it, both
/// facing the same way) with a clean one-step handoff. The shifted run must
/// still fit strictly inside the core; if it doesn't, or the main stair isn't
/// straight, we return None and the scored fallback placement is used instead.
fn stacked_under_main_stair(
    floor_plan: &FloorPlan,
    core: &Rect2D,
    run: i32,
) -> Option<(Vec<Point2D>, Cardinal)> {
    let sw = floor_plan
        .stairwells_on_floor(0)
        .into_iter()
        .find(|sw| sw.kind == StairKind::Straight)?;
    let dir = sw.direction;
    let sv: Point2D = dir.into();
    // Offset one cell up-slope from the main flight's bottom landing so the
    // flights are staggered by a single step instead of perfectly co-located.
    let start = *sw.positions.first()? + sv;
    if !stair_fits(start, dir, run, core) {
        return None;
    }
    let positions = (0..=run).map(|i| start + sv * i).collect();
    Some((positions, dir))
}

/// Decide whether to add a cellar and, if so, excavate, build, and furnish it.
/// Runs at the end of the pipeline; uses a derived RNG so it doesn't perturb
/// the main stream that drives the rest of the building. Returns the descending
/// stair's cell positions (position 0 is the cellar landing) when a cellar was
/// built, or None otherwise.
pub async fn maybe_build_cellar(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
    footprint: &Footprint,
    wall_segs: &WallSegments,
    floor_plan: &FloorPlan,
    room_plan: &RoomPlan,
    size_class: SizeClass,
) -> Option<Vec<Point2D>> {
    let mut rng = ctx.rng.derive();
    if !rolls_cellar(size_class, &mut rng) {
        return None;
    }

    let rects = footprint.rects();
    let core = rects[0];

    // The stairwell breaks up through the ground floor, so it must dodge what's
    // in the room above it: the doorway approach cells and the main staircase.
    let mut door_cells = ground_floor_door_cells(wall_segs);
    door_cells.extend(interior_door_approach_cells(room_plan, &core));
    let main_stair_cells = floor_plan.stair_cells_on_floor(0);
    let stair_cells: Vec<Point2D> = main_stair_cells
        .iter()
        .map(|&(x, y)| Point2D::new(x, y))
        .collect();

    let h = frame.wall_height() as i32;
    let run = h + 1;
    let floor_y = frame.floor_y(CELLAR_FLOOR); // walkable cellar surface (base_y - run)
    let slab_y = floor_y - 1; // stone floor block
    let ceiling_y = frame.ceiling_y(CELLAR_FLOOR); // = base_y - 1 (ground-floor slab)

    let interior = compute_room_interior(rects, 0);
    if interior.size.x <= 0 || interior.size.y <= 0 {
        return None;
    }

    // Prefer stacking directly beneath a straight main staircase so the two
    // flights form one continuous core; otherwise fall back to a clearance-
    // scored placement that dodges doorways and the main stair.
    let stair = stacked_under_main_stair(floor_plan, &core, run)
        .or_else(|| pick_stair(&core, run, &door_cells, &stair_cells, &mut rng))?;
    if volume_is_wet(ctx.editor, &interior, floor_y, ceiling_y) {
        return None;
    }

    excavate(ctx, &core, slab_y, floor_y, ceiling_y, &mut rng).await;
    let (positions, dir) = stair;
    place_descending_stair(ctx, &positions, dir, floor_y, &main_stair_cells, &mut rng).await;

    furnish_cellar(ctx, frame, core, interior, &positions).await;
    Some(positions)
}

/// Carve the cellar void and enclose it: stone floor slab under the whole core
/// rect, stone retaining walls around its perimeter, and air through the
/// interior. The ground-floor slab (already placed at `ceiling_y`) is the roof.
async fn excavate(
    ctx: &mut BuildCtx<'_>,
    core: &Rect2D,
    slab_y: i32,
    floor_y: i32,
    ceiling_y: i32,
    rng: &mut RNG,
) {
    let editor: &Editor = &*ctx.editor;
    let stone_id = ctx
        .palette
        .get_material(MaterialRole::PrimaryStone)
        .expect("No primary stone material for cellar")
        .clone();
    let mut placer_rng = rng.derive();
    let mut stone = MaterialPlacer::new(Placer::new(&ctx.data.materials, &mut placer_rng), stone_id);

    for p in core.iter() {
        // Solid stone floor under the entire rect (also seals under the walls).
        stone
            .place_block_forced(editor, Point3D::new(p.x, slab_y, p.y), BlockForm::Block, None, None)
            .await;

        let on_edge = core.on_edge(p);
        for y in floor_y..ceiling_y {
            let pos = Point3D::new(p.x, y, p.y);
            if on_edge {
                // Retaining wall holds back the surrounding soil.
                stone
                    .place_block_forced(editor, pos, BlockForm::Block, None, None)
                    .await;
            } else {
                editor.place_block_forced(&"air".into(), pos).await;
            }
        }
    }
}

/// Place a straight staircase descending from the ground floor to the cellar
/// landing, carving a stairwell shaft up through the ground-floor slab.
async fn place_descending_stair(
    ctx: &mut BuildCtx<'_>,
    positions: &[Point2D],
    dir: Cardinal,
    base_y: i32,
    main_stair_cells: &HashSet<(i32, i32)>,
    rng: &mut RNG,
) {
    let editor: &Editor = &*ctx.editor;
    let wood_id = ctx
        .palette
        .get_material(MaterialRole::PrimaryWood)
        .expect("No primary wood material for cellar stair")
        .clone();
    let mut placer_rng = rng.derive();
    let mut wood = MaterialPlacer::new(Placer::new(&ctx.data.materials, &mut placer_rng), wood_id);

    let ground_y = base_y + (positions.len() as i32 - 1); // top step sits at base_y-1 == ground slab
    let stair_state = HashMap::from([("facing".to_string(), dir.to_string())]);
    let underside_state = HashMap::from([
        ("facing".to_string(), (-dir).to_string()),
        ("half".to_string(), "top".to_string()),
    ]);

    // positions[0] is the bottom landing (no block); 1..=run ascend.
    for (i, pos) in positions.iter().enumerate() {
        if i == 0 {
            continue;
        }
        let step = (i - 1) as i32;
        let y = base_y + step;

        wood.place_block_forced(
            editor,
            Point3D::new(pos.x, y, pos.y),
            BlockForm::Stairs,
            Some(&stair_state),
            None,
        )
        .await;
        if step > 0 {
            wood.place_block(
                editor,
                Point3D::new(pos.x, y - 1, pos.y),
                BlockForm::Stairs,
                Some(&underside_state),
                None,
            )
            .await;
        }

        // Carve the shaft above this step up to the ground floor, removing the
        // ground-floor slab where it would otherwise cap the stairwell. In a
        // column shared with the main staircase (the stacked case) stop one
        // below ground level: that still cuts the floor-slab opening but leaves
        // the main step at floor height, which is the continuation of this
        // descent rather than an obstruction to clear.
        let top = if main_stair_cells.contains(&(pos.x, pos.y)) {
            ground_y - 1
        } else {
            ground_y
        };
        for clear_y in (y + 1)..=top {
            editor
                .place_block_forced(&"air".into(), Point3D::new(pos.x, clear_y, pos.y))
                .await;
        }
    }
}

/// Build a one-room plan for the cellar and run the shared furnishing pass over
/// it using the `storage` furniture list. Stair cells are constrained out so no
/// furniture lands on the steps or hangs over the shaft.
async fn furnish_cellar(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
    core: Rect2D,
    interior: Rect2D,
    positions: &[Point2D],
) {
    let mut constraints = ConstraintMap::new(&interior);
    for (i, pos) in positions.iter().enumerate() {
        let cell = (pos.x, pos.y);
        if i == 0 {
            // Landing: walkable but not placeable.
            constraints.set(cell, CellState::UnblockedReachable);
        } else {
            constraints.set(cell, CellState::Blocked);
        }
        constraints.set_ceiling(cell);
    }

    let room = Room {
        rect: core,
        rect_index: 0,
        floor: CELLAR_FLOOR,
        role: RoomRole::Cellar,
        room_type: RoomType::Storage,
        interior,
        constraints,
        furniture: Vec::new(),
        floor_type: None,
    };

    let mut plan = RoomPlan { rooms: vec![room], interior_doors: Vec::new() };
    furnish_rooms(ctx, &mut plan, frame, &[]).await;
}

#[cfg(test)]
mod test {
    use super::*;

    fn manhattan(a: Point2D, b: Point2D) -> i32 {
        (a.x - b.x).abs() + (a.y - b.y).abs()
    }

    /// The scored pick keeps the stairwell opening clear of a ground-floor
    /// doorway cell — when far corners exist it never lands in front of the door.
    #[test]
    fn pick_stair_keeps_clear_of_doors() {
        let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(10, 10));
        let run = 4;
        let door = Point2D::new(5, 9); // interior cell in front of a south door
        let doors = vec![door];
        let no_stairs: Vec<Point2D> = Vec::new();
        for seed in 0..200i64 {
            let mut rng = RNG::new(seed);
            let (positions, _) = pick_stair(&rect, run, &doors, &no_stairs, &mut rng)
                .expect("roomy rect yields a stair");
            let clear = positions[1..].iter().map(|f| manhattan(*f, door)).min().unwrap();
            assert!(clear >= 2, "seed {seed}: stairwell within {clear} of door {door:?}");
        }
    }

    /// The scored pick never overlaps the main staircase cells.
    #[test]
    fn pick_stair_avoids_main_stair() {
        let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(10, 10));
        let run = 4;
        let no_doors: Vec<Point2D> = Vec::new();
        let stairs: Vec<Point2D> = (1..=4).map(|i| Point2D::new(1, i)).collect();
        for seed in 0..200i64 {
            let mut rng = RNG::new(seed);
            let (positions, _) = pick_stair(&rect, run, &no_doors, &stairs, &mut rng)
                .expect("roomy rect yields a stair");
            let footprint: Vec<(i32, i32)> = positions[1..].iter().map(|p| (p.x, p.y)).collect();
            for s in &stairs {
                assert!(
                    !footprint.contains(&(s.x, s.y)),
                    "seed {seed}: cellar stair overlaps main stair at {s:?}",
                );
            }
        }
    }

    /// With nothing to avoid, a roomy rect always yields a fitting stair.
    #[test]
    fn pick_stair_succeeds_without_constraints() {
        let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(10, 10));
        let none: Vec<Point2D> = Vec::new();
        let mut rng = RNG::new(1);
        assert!(pick_stair(&rect, 4, &none, &none, &mut rng).is_some());
    }
}
