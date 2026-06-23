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
use crate::generator::population::AnchorScene;
use crate::geometry::{Point2D, Rect2D};
use crate::noise::RNG;

use super::cellar;
use super::engawa;
use super::door_ramp::{DoorRamp, place_door_ramps, plan_door_ramps_from_world};
use super::floors::{FloorPlan, clear_attic_stair_headroom, place_floors};
use super::footprint::{Footprint, SizeClass, find_boundaries};
use super::foundation::place_foundation;
use crate::generator::BuildClaim;
use crate::generator::buildings::BuildingID;
use super::frame::{Frame, apply_jetty, generate_frame};
use super::furnish::{decorate_rooftops, furnish_rooms};
use super::exterior::decorate_exterior_walls;
use super::{BuildingContext, Culture};
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
    place_wall_infill, place_windows, segment_cells,
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
    /// Exterior entrance cell per ground-floor door (bottom of the ramp if any,
    /// else the cell outside the door). Used to start door→road connectors.
    pub door_entrances: Vec<Point2D>,
    pub has_attic: bool,
    pub has_cellar: bool,
    /// Cellar descending-stair cells (position 0 is the cellar landing), if a
    /// cellar was built. Surfaced for blueprint/debug inspection.
    pub cellar_stair: Option<Vec<Point2D>>,
    pub roof_style: RoofStyle,
    pub size_class: SizeClass,
    pub timber_pattern: TimberPattern,
    /// Candidate per-room NPC standing positions (solo scenes), emitted after
    /// furnishing. The settlement's population pass picks how many to staff.
    pub npc_anchors: Vec<AnchorScene>,
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

    // Engawa: inset the walled footprint by one on every open-air side and raise
    // it one block onto a decked platform. Japanese only; `plan_engawa` gates on
    // the inset rects staying usable and returns `None` (build plain) otherwise.
    // The building proper (frame, walls, rooms, main roof) is built from the
    // inset `building_footprint`; the nominal `footprint` still drives the
    // foundation, terrain claim, and the deck/skirt extent.
    let engawa_plan = if bctx.engawa && bctx.culture == Culture::Japanese {
        engawa::plan_engawa(&footprint)
    } else {
        None
    };
    let building_footprint = engawa_plan
        .as_ref()
        .map(|e| e.building_footprint.clone())
        .unwrap_or_else(|| footprint.clone());
    let frame_base_y = if engawa_plan.is_some() { base_y + 1 } else { base_y };

    // Frame consumes a Footprint; keep `building_footprint` for later lookups
    // (find_boundaries, filled_points). The frame is generated from the
    // ground-inset footprint; for an engawa, `apply_overhang` then grafts on the
    // upper-floor extents (inset by one, overhanging the ground floor).
    let frame = generate_frame(building_footprint.clone(), frame_base_y, &size_class, ctx.rng);
    let frame = if let Some(plan) = &engawa_plan {
        engawa::apply_overhang(frame, plan)
    } else if bctx.jetty {
        apply_jetty(frame, &plot_bounds)
    } else {
        frame
    };

    let mut wall_segs = build_segments(&frame);
    let footprint_area = building_footprint.filled_points().len() as i32;
    let boundary_cells = boundary_cell_set(building_footprint.rects());
    place_doors(&mut wall_segs, &plot_bounds, footprint_area, &boundary_cells, ctx.rng);

    let has_attic = matches!(roof_style, RoofStyle::Gable(GablePitch::Double));
    let skip_ceilings = matches!(roof_style, RoofStyle::Flat);

    let _terrace_door_cells = if matches!(roof_style, RoofStyle::Flat) {
        place_terrace_doors(&mut wall_segs, &frame)
    } else {
        Vec::new()
    };

    let floor_plan = place_floors(ctx, &frame, &wall_segs, has_attic, skip_ceilings).await;
    // Japanese walls are white shoji panels over a timber baseboard, divided by
    // vertical wood beams; other cultures keep the stone-base / solid fill.
    let (ground_infill, upper_infill) = match bctx.culture {
        Culture::Japanese => (WallInfill::TimberPanels, WallInfill::TimberPanels),
        _ => (WallInfill::StoneBase, WallInfill::Solid),
    };
    place_wall_infill(ctx, &wall_segs, &ground_infill, &upper_infill).await;

    // Resolve the timber pattern now that the frame is known — auto-pick
    // filters out patterns whose studs wouldn't fit the longest wall segment.
    // Use a derived RNG so adding the auto-pick path doesn't shift the main
    // stream that rooms/furnish later draw from. Decorative timber framing is a
    // Medieval feature; other cultures keep the plain skeleton (baseline corner
    // posts + crossbeams only).
    let timber_pattern = bctx.timber_pattern.unwrap_or_else(|| {
        if bctx.culture != Culture::Medieval {
            return TimberPattern::Plain;
        }
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

    // Engawa: lay the wooden veranda deck around the inset building and skirt it
    // with a pent roof at the ground-floor ceiling. Runs after the main roof so
    // the skirt overlays cleanly; the deck planks overwrite the foundation
    // course in the perimeter ring.
    if let Some(plan) = &engawa_plan {
        engawa::place_engawa(ctx, &frame, plan).await;
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
    let mut interior_wall_cells: HashSet<(i32, i32)> = find_boundaries(building_footprint.rects())
        .iter()
        .flat_map(|b| b.wall_cells.iter().map(|c| (c.x, c.y)))
        .collect();
    interior_wall_cells.extend(ladder_walls);
    if let Some(wall_cell) = roof_ladder_wall {
        interior_wall_cells.insert(wall_cell);
    }
    place_windows(&mut wall_segs, &interior_wall_cells, ctx.rng);
    mark_windows(&mut room_plan, &wall_segs);
    place_openings(ctx, &wall_segs, window_fill, bctx.culture).await;

    // Reconcile doors with terrain: run parallel stair ramps along the wall
    // for doors where `base_y` doesn't match outside-terrain. Skipped for an
    // engawa — its doors open straight onto the raised veranda deck, so a ramp
    // down to the surrounding ground would cut through the deck.
    let door_ramps = if engawa_plan.is_some() {
        Vec::new()
    } else {
        plan_door_ramps_from_world(&wall_segs, &building_footprint, ctx.editor.world())
    };
    // Save the real exterior entrance per door (bottom of the ramp, if any) for
    // the settlement's door→road connectors.
    let door_entrances = super::door_ramp::door_entrances(&wall_segs, &door_ramps);
    place_door_ramps(ctx, &door_ramps).await;

    assign_room_floors(&mut room_plan);
    place_room_floors(ctx, &frame, &room_plan, bctx).await;


    // Furnish, then harvest the NPC anchor scenes the placed furniture offers
    // (validated against the final per-room CellState inside furnish_rooms).
    // Rooftop terraces and the cellar add their own anchors below.
    let mut npc_anchors = furnish_rooms(ctx, &mut room_plan, &frame, &roof_heightmaps).await;

    // Flat roofs are open terraces — decorate the deck (shade, seating, plants)
    // once the interior is furnished. Keeps the ladder exit clear.
    if matches!(roof_style, RoofStyle::Flat) {
        npc_anchors.extend(decorate_rooftops(ctx, &frame, roof_ladder_wall).await);
    }

    // A few sparse props against the outside walls (barrels, pots, …) so the
    // house reads as lived-in. Skips doors, roads, and claimed cells.
    decorate_exterior_walls(ctx, &building_footprint, &wall_segs).await;

    check_building_invariants(&frame, &room_plan, &floor_plan, &roof_heightmaps)?;

    // Cellar runs last: it carves below the finished building using a derived
    // RNG, so it neither perturbs the main stream nor disturbs the room_plan
    // that blueprint/invariant code iterates.
    // Cellar uses `building_footprint` (the inset walls for an engawa) so its
    // retaining walls sit under the actual walls, not out under the veranda deck.
    let cellar = cellar::maybe_build_cellar(ctx, &frame, &building_footprint, &wall_segs, &floor_plan, &room_plan, size_class).await;
    let has_cellar = cellar.is_some();
    let cellar_stair = cellar.map(|(stair, anchors)| {
        npc_anchors.extend(anchors);
        stair
    });

    // Drop any anchor whose feet sit in a doorway — the interior cell directly
    // behind a door (where someone stepping through stands) or the exterior cell
    // directly in front. An NPC anchored there blocks ingress/egress.
    let door_keepout = door_keepout_cells(&wall_segs);
    npc_anchors.retain(|scene| {
        !scene.slots.iter().any(|slot| {
            door_keepout.contains(&(slot.pos.y, Point2D::new(slot.pos.x, slot.pos.z)))
        })
    });

    // Claim the structural footprint (the actual house cells, no buffer) so a
    // later building's foundation blend won't raise earth/grass into this house
    // — `blend_terrain` skips Building-claimed cells.
    let building_idx = ctx.editor.world().buildings.len();
    for p in footprint.filled_points() {
        ctx.editor.world_mut().claim(p, BuildClaim::Building(BuildingID(building_idx)));
    }
    // The engawa deck can bump just outside the nominal footprint at junctions;
    // claim those cells too so a neighbour's terrain blend doesn't bury the deck.
    if let Some(plan) = &engawa_plan {
        for &p in &plan.deck_cells {
            ctx.editor.world_mut().claim(p, BuildClaim::Building(BuildingID(building_idx)));
        }
    }

    Ok(HouseOutput {
        footprint,
        frame,
        wall_segs,
        floor_plan,
        room_plan,
        door_ramps,
        door_entrances,
        has_attic,
        has_cellar,
        cellar_stair,
        roof_style,
        size_class,
        timber_pattern,
        npc_anchors,
    })
}

/// `(floor_base_y, cell)` pairs that NPC anchors must avoid: the interior step
/// and the exterior step of every door. `seg.facing` is the wall's INWARD normal
/// (see `WallSegment::facing`), so the interior side is `door_cell + facing` and
/// the exterior side is `door_cell - facing`. Floor-aware so an upper-room cell
/// directly above a ground-floor door is still eligible.
fn door_keepout_cells(wall_segs: &WallSegments) -> HashSet<(i32, Point2D)> {
    let mut out = HashSet::new();
    for (seg, opening) in wall_segs.doors() {
        let seg_cells = segment_cells(seg);
        let inward: Point2D = seg.facing.into();
        for w in 0..opening.width as usize {
            let idx = opening.offset as usize + w;
            if let Some(&door_cell) = seg_cells.get(idx) {
                out.insert((seg.base_y, door_cell + inward));
                out.insert((seg.base_y, door_cell - inward));
            }
        }
    }
    out
}
