//! Unit tests for the door-ramp planner. These don't touch an `Editor` or
//! `World` — the planner takes a closure for terrain heights, so we can feed
//! synthetic height functions and assert step-by-step.

use std::collections::HashMap;

use crate::geometry::{Cardinal, Point2D, Rect2D};
use crate::generator::buildings_v2::footprint::Footprint;
use crate::generator::buildings_v2::footprint::merge::outline_from_rects;
use crate::generator::buildings_v2::frame::{Frame, generate_frame};
use crate::generator::buildings_v2::footprint::SizeClass;
use crate::generator::buildings_v2::walls::{
    DoorStyle, Opening, OpeningKind, WallSegments, build_segments,
};
use crate::noise::RNG;

use super::{RampKind, plan_door_ramps};

/// Build a single-rect footprint + frame at `base_y = 64` with 1 floor.
fn single_rect_frame(rect: Rect2D) -> (Footprint, Frame) {
    let vertices = outline_from_rects(&[rect]);
    let footprint = Footprint::new(vertices, vec![rect]);
    let mut rng = RNG::new(1);
    let frame = generate_frame(footprint.clone(), 64, &SizeClass::House, &mut rng);
    (footprint, frame)
}

/// Attach a single door to the given cardinal face of the segments. `offset`
/// is the block offset along the chosen segment.
fn add_door(wall_segs: &mut WallSegments, facing: Cardinal, offset: u32) {
    let idx = wall_segs.segments.iter().position(|s| s.floor == 0 && s.facing == facing)
        .expect("no ground-floor segment with that facing");
    wall_segs.segments[idx].openings.push(Opening {
        kind: OpeningKind::Door(DoorStyle::Single),
        offset,
        width: 1,
        height: 2,
        y_offset: 0,
    });
}

/// Attach a 2-wide double door to the given cardinal face.
fn add_double_door(wall_segs: &mut WallSegments, facing: Cardinal, offset: u32) {
    let idx = wall_segs.segments.iter().position(|s| s.floor == 0 && s.facing == facing)
        .expect("no ground-floor segment with that facing");
    wall_segs.segments[idx].openings.push(Opening {
        kind: OpeningKind::Door(DoorStyle::Double),
        offset,
        width: 2,
        height: 2,
        y_offset: 0,
    });
}

fn flat_heights(y: i32) -> impl Fn(Point2D) -> i32 {
    move |_| y
}

/// Sample a heightmap from a `HashMap<Point2D, i32>`, falling back to `default`.
fn map_heights(map: HashMap<Point2D, i32>, default: i32) -> impl Fn(Point2D) -> i32 {
    move |p| *map.get(&p).unwrap_or(&default)
}

#[test]
fn flat_terrain_no_ramp() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 8));
    let (footprint, frame) = single_rect_frame(rect);
    let mut wall_segs = build_segments(&frame);
    add_door(&mut wall_segs, Cardinal::South, 4);

    let ramps = plan_door_ramps(&wall_segs, &footprint, flat_heights(64));
    assert!(ramps.is_empty(), "flat terrain should produce no ramps");
}

#[test]
fn terrain_below_produces_ascending_ramp() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 8));
    let (footprint, frame) = single_rect_frame(rect);
    let mut wall_segs = build_segments(&frame);
    add_door(&mut wall_segs, Cardinal::South, 4);

    // Terrain sits 3 below base_y everywhere — classic valley building.
    let ramps = plan_door_ramps(&wall_segs, &footprint, flat_heights(61));
    assert_eq!(ramps.len(), 1);
    let ramp = &ramps[0];
    assert_eq!(ramp.kind, RampKind::Ascending);
    assert_eq!(ramp.landing_y, 64);
    assert_eq!(ramp.landing_terrain_y, 61);
    // 3 steps, each descending by 1 Y.
    assert_eq!(ramp.steps.len(), 3);
    for (k, step) in ramp.steps.iter().enumerate() {
        assert_eq!(step.y, 64 - (k as i32 + 1));
    }
    // Stair "facing" points back toward the door (player walks toward door to climb).
    assert_eq!(ramp.stair_facing, -ramp.side_dir);
}

#[test]
fn terrain_above_produces_descending_ramp_with_carve() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 8));
    let (footprint, frame) = single_rect_frame(rect);
    let mut wall_segs = build_segments(&frame);
    add_door(&mut wall_segs, Cardinal::South, 4);

    // Door is buried 2 blocks into a hillside.
    let ramps = plan_door_ramps(&wall_segs, &footprint, flat_heights(66));
    assert_eq!(ramps.len(), 1);
    let ramp = &ramps[0];
    assert_eq!(ramp.kind, RampKind::Descending);
    assert_eq!(ramp.landing_y, 64);
    assert_eq!(ramp.steps.len(), 2);
    for (k, step) in ramp.steps.iter().enumerate() {
        assert_eq!(step.y, 64 + k as i32);
    }
    // Stair "facing" points away from door (player walks away from door as stair rises).
    assert_eq!(ramp.stair_facing, ramp.side_dir);
}

#[test]
fn dy_greater_than_max_is_capped() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(20, 8));
    let (footprint, frame) = single_rect_frame(rect);
    let mut wall_segs = build_segments(&frame);
    // Door near the middle of the long south wall (plenty of room either side).
    add_door(&mut wall_segs, Cardinal::South, 10);

    let ramps = plan_door_ramps(&wall_segs, &footprint, flat_heights(50)); // dy = -14
    assert_eq!(ramps.len(), 1);
    // Capped at MAX_RAMP_STEPS = 5.
    assert_eq!(ramps[0].steps.len(), 5);
}

#[test]
fn concave_corner_blocks_preferred_side() {
    // L-shape: inner corner on the south wall at x=6.
    // Core 0..=6 × 0..=4 + wing 7..=9 × 2..=4. The south wall of the core is
    // the segment south of the concave corner — extending the ramp past x=6
    // would walk into the wing's footprint.
    let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 4));
    let wing = Rect2D::from_points(Point2D::new(7, 2), Point2D::new(9, 4));
    let rects = vec![core, wing];
    let vertices = outline_from_rects(&rects);
    let footprint = Footprint::new(vertices, rects);
    let mut rng = RNG::new(1);
    let frame = generate_frame(footprint.clone(), 64, &SizeClass::House, &mut rng);
    let mut wall_segs = build_segments(&frame);

    // Put a door on the north wall of the wing (floor 0, facing North).
    // The wing's north wall runs along z=2 from x=7 to x=9 (above y=2), so the
    // north-facing segment cells are at (7..=8, 1) — 2 cells. The concave corner
    // of the outline is at (6, 2) dual-grid, so the segment extending into -x
    // toward it.
    //
    // Add a door at offset=0 (leftmost cell), so the natural ramp side is +x
    // (toward the wing interior — but that's outside the footprint in the -z
    // direction, still safe) vs -x (toward the concave corner and into the
    // core's exterior strip at z=1).
    let door_idx = wall_segs.segments.iter().position(|s| s.floor == 0 && s.facing == Cardinal::North)
        .expect("no north-facing ground segment");
    // Door at the leftmost cell of the north wing wall.
    wall_segs.segments[door_idx].openings.push(Opening {
        kind: OpeningKind::Door(DoorStyle::Single),
        offset: 0,
        width: 1,
        height: 2,
        y_offset: 0,
    });

    // Ask for a 4-step ramp but the wall is only 2-3 cells long; the planner
    // must truncate to whatever fits without crossing the footprint.
    let ramps = plan_door_ramps(&wall_segs, &footprint, flat_heights(60));
    assert_eq!(ramps.len(), 1);
    let ramp = &ramps[0];
    // Must not have placed steps inside the footprint.
    for step in &ramp.steps {
        assert!(!footprint.contains(step.cell),
            "ramp step at {:?} is inside the footprint", step.cell);
    }
}

#[tokio::test]
async fn place_writes_stair_blocks_on_sloped_world() {
    // End-to-end smoke: plan a ramp from a synthetic sloped world, place it,
    // then read back the blocks through the editor to confirm the ramp landed.
    use crate::editor::World;
    use crate::geometry::{Point3D, Rect3D};
    use crate::generator::buildings_v2::BuildCtx;
    use crate::generator::buildings_v2::door_ramp::place_door_ramps;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;

    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(63, 127, 63));
    let world = World::synthetic(build_area, 64);
    let mut editor = world.get_offline_editor();

    // Build a 9×9 rect centered so the south wall's outside row sits at z=18.
    let rect = Rect2D::from_points(Point2D::new(10, 10), Point2D::new(18, 17));
    let (footprint, frame) = single_rect_frame(rect);
    let mut wall_segs = build_segments(&frame);
    add_door(&mut wall_segs, Cardinal::North, 4); // outward = South; landing at z=18

    // Planner reads `get_ocean_floor_height_at`, which on the synthetic world
    // returns 64 everywhere by default. Feed dy=-2 via a closure so we exercise
    // the placer without fiddling with internal heightmaps.
    let ramps = plan_door_ramps(&wall_segs, &footprint, |p| if p.y >= 18 { 62 } else { 64 });
    assert_eq!(ramps.len(), 1, "expected a single ascending ramp");
    assert_eq!(ramps[0].kind, RampKind::Ascending);

    let data = LoadedData::load().expect("Failed to load data");
    let palette_id: PaletteId = "medieval_spruce".into();
    let palette = data.palettes.get(&palette_id).expect("Palette not found").clone();
    let mut rng = RNG::new(1);
    let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
    place_door_ramps(&mut ctx, &ramps).await;

    // Each step position should now read back as a stairs block (cached in the editor).
    for step in &ramps[0].steps {
        let block = ctx.editor.get_block(Point3D::new(step.cell.x, step.y, step.cell.y));
        assert!(block.id.as_str().contains("stairs"),
            "expected stair block at {:?} y={}, got {:?}", step.cell, step.y, block.id.as_str());
    }
}

#[test]
fn ramp_terrain_y_matches_sampled_heights() {
    // Sloped terrain: height decreases as z increases.
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 4));
    let (footprint, frame) = single_rect_frame(rect);
    let mut wall_segs = build_segments(&frame);
    add_door(&mut wall_segs, Cardinal::South, 4);

    // Height function: y=64 at z=0, drops 1 per z.
    let heights: HashMap<Point2D, i32> = (0..=10).flat_map(|x|
        (-1..=10).map(move |z| (Point2D::new(x, z), 64 - z))
    ).collect();
    let ramps = plan_door_ramps(&wall_segs, &footprint, map_heights(heights, 64));
    assert_eq!(ramps.len(), 1);
    let ramp = &ramps[0];
    // Landing is at south wall cell + south offset, so one z past the wall.
    // Each step's `terrain_y` should match the sampled height at its cell.
    for step in &ramp.steps {
        let expected = 64 - step.cell.y;
        assert_eq!(step.terrain_y, expected,
            "step at {:?} has terrain_y={} expected={}", step.cell, step.terrain_y, expected);
    }
}
