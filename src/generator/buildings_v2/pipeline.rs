//! Top-level building pipeline.
//!
//! `BuildCtx` bundles the four cross-cutting params (editor / data / palette / rng)
//! that every placer needs. `build_house` assembles the full 18-step sequence —
//! footprint → foundation → frame → walls → floors → walls → roof → rooms → furnish —
//! including the non-linear walls↔rooms back-edge, where windows can only be
//! placed after room partitioning reveals which wall cells are interior.

use std::collections::HashSet;

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::Palette;
use crate::geometry::Rect2D;
use crate::noise::RNG;

use super::floors::{FloorPlan, clear_attic_stair_headroom, place_floors};
use super::footprint::{Footprint, SizeClass, find_boundaries};
use super::foundation::place_foundation;
use super::frame::{Frame, generate_frame};
use super::furnish::furnish_rooms;
use super::roof::gable::GablePitch;
use super::roof::place_roof;
use super::rooms::{
    RoomPlan, assign_attic_types, build_rooms, check_building_invariants,
    mark_gable_doorways, mark_windows, place_attic_ladders,
};
use super::walls::{
    WallInfill, WallSegments, build_segments, boundary_cell_set,
    place_doors, place_frame, place_openings, place_wall_infill, place_windows,
};

/// Shared context threaded through every placer stage. Reborrow the fields
/// as needed — e.g. `ctx.editor` coerces to `&Editor` where a shared ref is
/// expected, and to `&mut Editor` where `place_foundation` needs `world_mut()`.
pub struct BuildCtx<'a> {
    pub editor: &'a mut Editor,
    pub data: &'a LoadedData,
    pub palette: &'a Palette,
    pub rng: &'a mut RNG,
}

impl<'a> BuildCtx<'a> {
    pub fn new(
        editor: &'a mut Editor,
        data: &'a LoadedData,
        palette: &'a Palette,
        rng: &'a mut RNG,
    ) -> Self {
        Self { editor, data, palette, rng }
    }
}

/// Everything `build_house` produces. Callers use this for blueprint rendering
/// and stats; the building is already placed in the editor by the time it returns.
pub struct HouseOutput {
    pub footprint: Footprint,
    pub frame: Frame,
    pub wall_segs: WallSegments,
    pub floor_plan: FloorPlan,
    pub room_plan: RoomPlan,
    pub has_attic: bool,
    pub pitch: GablePitch,
    pub size_class: SizeClass,
}

/// Runs the full per-building pipeline. Caller owns footprint generation and
/// plot markup (so a single plot can host multiple buildings) and the final
/// `editor.flush_buffer()`. `plot_bounds` is used for door-distance scoring.
pub async fn build_house(
    ctx: &mut BuildCtx<'_>,
    footprint: Footprint,
    size_class: SizeClass,
    pitch: GablePitch,
    plot_bounds: Rect2D,
) -> Result<HouseOutput, String> {
    // Foundation: terrain analysis + level + stone course. Needs &mut Editor
    // to update the world heightmap.
    let base_y = place_foundation(ctx, &footprint).await;

    // Frame consumes a Footprint; keep the original for later lookups
    // (find_boundaries, filled_points).
    let frame = generate_frame(footprint.clone(), base_y, &size_class, ctx.rng);

    let mut wall_segs = build_segments(&frame);
    let footprint_area = footprint.filled_points().len() as i32;
    let boundary_cells = boundary_cell_set(footprint.rects());
    place_doors(&mut wall_segs, &plot_bounds, footprint_area, &boundary_cells, ctx.rng);

    let has_attic = matches!(pitch, GablePitch::Double);

    let floor_plan = place_floors(ctx, &frame, &wall_segs, has_attic).await;
    place_wall_infill(ctx, &wall_segs, &WallInfill::StoneBase, &WallInfill::Solid).await;
    place_frame(ctx, &frame).await;
    let gable_doorways = place_roof(ctx, &frame, pitch).await;
    if has_attic {
        clear_attic_stair_headroom(ctx, &frame, &floor_plan).await;
    }

    // Rooms are built before windows so window placement can avoid interior
    // wall cells and attic-ladder walls.
    let mut room_plan = build_rooms(ctx, &frame, &wall_segs, &floor_plan, has_attic, size_class).await;
    mark_gable_doorways(&mut room_plan, &gable_doorways);
    let ladder_walls = place_attic_ladders(ctx, &mut room_plan, &frame, &floor_plan, &wall_segs, &gable_doorways).await;
    assign_attic_types(&mut room_plan, size_class, ctx.rng);

    // Windows: the walls↔rooms back-edge. Interior wall cells come from room
    // partitioning; ladder walls come from attic ladder placement. Both must
    // be excluded from window placement.
    let mut interior_wall_cells: HashSet<(i32, i32)> = find_boundaries(footprint.rects())
        .iter()
        .flat_map(|b| b.wall_cells.iter().map(|c| (c.x, c.y)))
        .collect();
    interior_wall_cells.extend(ladder_walls);
    place_windows(&mut wall_segs, &interior_wall_cells, ctx.rng);
    mark_windows(&mut room_plan, &wall_segs);
    place_openings(ctx, &wall_segs).await;

    furnish_rooms(ctx, &mut room_plan, &frame).await;

    check_building_invariants(&frame, &room_plan, &floor_plan)?;

    Ok(HouseOutput {
        footprint,
        frame,
        wall_segs,
        floor_plan,
        room_plan,
        has_attic,
        pitch,
        size_class,
    })
}
