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
use crate::geometry::{Point2D, Rect2D};
use crate::noise::RNG;

use super::cellar;
use super::door_ramp::{DoorRamp, place_door_ramps, plan_door_ramps_from_world};
use super::floors::{FloorPlan, clear_attic_stair_headroom, place_floors};
use super::footprint::{Footprint, SizeClass, find_boundaries};
use super::foundation::place_foundation;
use crate::generator::BuildClaim;
use crate::generator::buildings::BuildingID;
use super::frame::{Frame, apply_jetty, generate_frame};
use super::furnish::furnish_rooms;
use super::BuildingContext;
use super::roof::RoofStyle;
use super::roof::gable::GablePitch;
use super::roof::{place_roof, place_roof_ladder};
use super::floors::place_room_floors;
use super::rooms::{
    RoomPlan, assign_attic_types, assign_room_floors, build_rooms,
    check_building_invariants, mark_gable_doorways, mark_windows,
    place_attic_ladders,
};
use super::walls::{
    TimberPattern, WallInfill, WallSegments, build_segments, boundary_cell_set,
    place_doors, place_frame, place_openings, place_terrace_doors,
    place_wall_infill, place_windows,
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
    pub door_ramps: Vec<DoorRamp>,
    pub has_attic: bool,
    pub has_cellar: bool,
    /// Cellar descending-stair cells (position 0 is the cellar landing), if a
    /// cellar was built. Surfaced for blueprint/debug inspection.
    pub cellar_stair: Option<Vec<Point2D>>,
    pub roof_style: RoofStyle,
    pub size_class: SizeClass,
    pub timber_pattern: TimberPattern,
}

/// Runs the full per-building pipeline. Caller owns footprint generation and
/// plot markup (so a single plot can host multiple buildings) and the final
/// `editor.flush_buffer()`. `plot_bounds` is used for door-distance scoring.
pub async fn build_house(
    ctx: &mut BuildCtx<'_>,
    footprint: Footprint,
    bctx: &BuildingContext,
    plot_bounds: Rect2D,
) -> Result<HouseOutput, String> {
    let size_class = bctx.size_class;
    let roof_style = bctx.roof_style;
    let window_fill = bctx.window_fill;


    // Foundation: terrain analysis + level + stone course. Needs &mut Editor
    // to update the world heightmap. `base_y_override` pins the floor (e.g. to a
    // road's height) instead of deriving it from the terrain percentile.
    let base_y = place_foundation(ctx, &footprint, bctx.base_y_override).await;

    // Frame consumes a Footprint; keep the original for later lookups
    // (find_boundaries, filled_points).
    let frame = generate_frame(footprint.clone(), base_y, &size_class, ctx.rng);
    let frame = if bctx.jetty { apply_jetty(frame, &plot_bounds) } else { frame };

    let mut wall_segs = build_segments(&frame);
    let footprint_area = footprint.filled_points().len() as i32;
    let boundary_cells = boundary_cell_set(footprint.rects());
    place_doors(&mut wall_segs, &plot_bounds, footprint_area, &boundary_cells, ctx.rng);

    let has_attic = matches!(roof_style, RoofStyle::Gable(GablePitch::Double));
    let skip_ceilings = matches!(roof_style, RoofStyle::Flat);

    let _terrace_door_cells = if matches!(roof_style, RoofStyle::Flat) {
        place_terrace_doors(&mut wall_segs, &frame)
    } else {
        Vec::new()
    };

    let floor_plan = place_floors(ctx, &frame, &wall_segs, has_attic, skip_ceilings).await;
    place_wall_infill(ctx, &wall_segs, &WallInfill::StoneBase, &WallInfill::Solid).await;

    // Resolve the timber pattern now that the frame is known — auto-pick
    // filters out patterns whose studs wouldn't fit the longest wall segment.
    // Use a derived RNG so adding the auto-pick path doesn't shift the main
    // stream that rooms/furnish later draw from.
    let timber_pattern = bctx.timber_pattern.unwrap_or_else(|| {
        let max_seg_len = wall_segs.segments.iter()
            .map(|s| s.length.max(0) as u32)
            .max()
            .unwrap_or(0);
        let mut timber_rng = ctx.rng.derive();
        TimberPattern::pick(size_class, max_seg_len, &mut timber_rng)
    });
    place_frame(ctx, &frame, &timber_pattern).await;
    let (gable_doorways, roof_heightmaps) = place_roof(ctx, &frame, roof_style).await;
    if has_attic {
        clear_attic_stair_headroom(ctx, &frame, &floor_plan).await;
    }

    // Rooms are built before windows so window placement can avoid interior
    // wall cells and attic-ladder walls.
    let mut room_plan = build_rooms(ctx, &frame, &wall_segs, &floor_plan, has_attic, size_class).await;
    mark_gable_doorways(&mut room_plan, &gable_doorways);
    let ladder_walls = place_attic_ladders(ctx, &mut room_plan, &frame, &floor_plan, &wall_segs, &gable_doorways).await;
    assign_attic_types(&mut room_plan, size_class, ctx.rng);

    let roof_ladder_wall = if matches!(roof_style, RoofStyle::Flat) {
        place_roof_ladder(ctx, &frame, &floor_plan, &mut room_plan).await
    } else {
        None
    };


    // Windows: the walls↔rooms back-edge. Interior wall cells come from room
    // partitioning; ladder walls come from attic ladder placement. Both must
    // be excluded from window placement.
    let mut interior_wall_cells: HashSet<(i32, i32)> = find_boundaries(footprint.rects())
        .iter()
        .flat_map(|b| b.wall_cells.iter().map(|c| (c.x, c.y)))
        .collect();
    interior_wall_cells.extend(ladder_walls);
    if let Some(wall_cell) = roof_ladder_wall {
        interior_wall_cells.insert(wall_cell);
    }
    place_windows(&mut wall_segs, &interior_wall_cells, ctx.rng);
    mark_windows(&mut room_plan, &wall_segs);
    place_openings(ctx, &wall_segs, window_fill).await;

    // Reconcile doors with terrain: run parallel stair ramps along the wall
    // for doors where `base_y` doesn't match outside-terrain.
    let door_ramps = plan_door_ramps_from_world(&wall_segs, &footprint, ctx.editor.world());
    place_door_ramps(ctx, &door_ramps).await;

    assign_room_floors(&mut room_plan);
    place_room_floors(ctx, &frame, &room_plan, bctx).await;


    furnish_rooms(ctx, &mut room_plan, &frame, &roof_heightmaps).await;

    check_building_invariants(&frame, &room_plan, &floor_plan)?;

    // Cellar runs last: it carves below the finished building using a derived
    // RNG, so it neither perturbs the main stream nor disturbs the room_plan
    // that blueprint/invariant code iterates.
    let cellar_stair = cellar::maybe_build_cellar(ctx, &frame, &footprint, &wall_segs, &floor_plan, &room_plan, size_class).await;
    let has_cellar = cellar_stair.is_some();

    // Claim the structural footprint (the actual house cells, no buffer) so a
    // later building's foundation blend won't raise earth/grass into this house
    // — `blend_terrain` skips Building-claimed cells.
    let building_idx = ctx.editor.world().buildings.len();
    for p in footprint.filled_points() {
        ctx.editor.world_mut().claim(p, BuildClaim::Building(BuildingID(building_idx)));
    }

    Ok(HouseOutput {
        footprint,
        frame,
        wall_segs,
        floor_plan,
        room_plan,
        door_ramps,
        has_attic,
        has_cellar,
        cellar_stair,
        roof_style,
        size_class,
        timber_pattern,
    })
}
