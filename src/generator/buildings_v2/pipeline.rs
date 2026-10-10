//! Top-level building pipeline.
//!
//! `BuildCtx` bundles the four cross-cutting params (editor / data / palette / rng)
//! that every placer needs. `build_house` hands each stage its own named stream
//! of the house seed (`ctx.on(&mut seed.stream(..))`, see `seed`), so no stage's
//! draws shift another's. It assembles the full 18-step sequence —
//! footprint → foundation → frame → walls → floors → walls → roof → rooms → furnish —
//! including the non-linear walls↔rooms back-edge, where windows can only be
//! placed after room partitioning reveals which wall cells are interior.

use crate::util::HashSet;

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::Palette;
use crate::generator::population::AnchorScene;
use crate::geometry::{Cardinal, Point2D, Point3D, Rect2D};
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
use super::{BuildingContext, Culture, HouseSeed};
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

    /// This context with `rng` in place of its own stream, for one stage:
    /// `place_x(&mut ctx.on(&mut seed.stream("x")), ..)`.
    pub fn on<'b>(&'b mut self, rng: &'b mut RNG) -> BuildCtx<'b> {
        BuildCtx { editor: &mut *self.editor, data: self.data, palette: self.palette, rng }
    }
}

/// Everything `build_house` produces. Callers use this for blueprint rendering
/// and stats; the building is already placed in the editor by the time it returns.
pub struct HouseOutput {
    /// The seed every decision of this house drew from (`bctx.seed`, or the
    /// one drawn from the caller's stream).
    pub seed: HouseSeed,
    /// Ground-floor level from the foundation (build-area local): the door
    /// sill, before an engawa raises the building one more block.
    pub base_y: i32,
    /// The side the front door opens out of (`place_doors`' primary door).
    pub front_door: Option<Cardinal>,
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
    /// The cellar's furnishing theme (`storage`, `cellar_wine`, ...) when one was built.
    pub cellar_theme: Option<&'static str>,
    /// Every build-area-local cell the roof stage placed (surface, fill,
    /// gable walls, chimney), so a cutaway can leave the roof out.
    pub roof_cells: Vec<Point3D>,
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
///
/// Every stage draws from its own stream of the house seed (`bctx.seed`, else
/// one draw from `ctx.rng`); the caller's `ctx.rng` is otherwise left alone.
pub async fn build_house(
    ctx: &mut BuildCtx<'_>,
    footprint: Footprint,
    bctx: &BuildingContext,
    plot_bounds: Rect2D,
) -> Result<HouseOutput, String> {
    let size_class = bctx.size_class;
    let roof_style = bctx.roof_style;
    let window_fill = bctx.window_fill;

    // One seed per house; from here on nothing touches the caller's stream.
    // Stages run on `ctx.on(&mut seed.stream(..))`; anything left on plain
    // `ctx` (stages that draw nothing) falls back to the `build` stream.
    let seed = bctx.seed.unwrap_or_else(|| HouseSeed::draw(ctx.rng));
    let mut build_rng = seed.stream("build");
    let ctx = &mut ctx.on(&mut build_rng);

    // Foundation: terrain analysis + level + stone course. Needs &mut Editor
    // to update the world heightmap. `base_y_override` pins the floor (e.g. to a
    // road's height) instead of deriving it from the terrain percentile.
    let base_y = place_foundation(&mut ctx.on(&mut seed.stream("foundation")), &footprint, bctx.base_y_override).await;

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
    // deep-inset footprint, so every engawa floor already sits at that inset —
    // no per-floor overhang. A non-engawa building may instead jetty its uppers.
    let frame = generate_frame(building_footprint.clone(), frame_base_y, &size_class, &mut seed.stream("floors"));
    let frame = if engawa_plan.is_some() {
        frame
    } else if bctx.jetty {
        apply_jetty(frame, &plot_bounds)
    } else {
        frame
    };
    // Vertical variants (both off by default, leaving the frame untouched).
    let multi_storey = frame.max_floors() >= 2;
    let top_floor_extra = u32::from(
        bctx.tall_top_floor && multi_storey && matches!(size_class, SizeClass::Hall | SizeClass::Manor),
    );
    let knee = if bctx.knee_wall && multi_storey { knee_rows(roof_style) } else { 0 };
    let frame = frame.with_vertical_options(top_floor_extra, knee);

    let mut wall_segs = build_segments(&frame);
    let footprint_area = building_footprint.filled_points().len() as i32;
    let boundary_cells = boundary_cell_set(building_footprint.rects());
    let front_door = place_doors(&mut wall_segs, &plot_bounds, footprint_area, &boundary_cells, &mut seed.stream("doors"));

    let has_attic = matches!(roof_style, RoofStyle::Gable(GablePitch::Double));
    let skip_ceilings = matches!(roof_style, RoofStyle::Flat);

    let _terrace_door_cells = if matches!(roof_style, RoofStyle::Flat) {
        place_terrace_doors(&mut wall_segs, &frame)
    } else {
        Vec::new()
    };

    let floor_plan = place_floors(&mut ctx.on(&mut seed.stream("floors.blocks")), seed, &frame, &wall_segs, has_attic, skip_ceilings).await;
    // Japanese walls are white shoji panels over a timber baseboard, divided by
    // vertical wood beams; other cultures keep the stone-base / solid fill.
    let (ground_infill, upper_infill) = match bctx.culture {
        Culture::Japanese => (WallInfill::TimberPanels, WallInfill::TimberPanels),
        _ => (WallInfill::StoneBase, WallInfill::Solid),
    };
    place_wall_infill(&mut ctx.on(&mut seed.stream("walls.blocks")), &wall_segs, &ground_infill, &upper_infill).await;

    // Resolve the timber pattern now that the frame is known — auto-pick
    // filters out patterns whose studs wouldn't fit the longest wall segment.
    // Draws from its own `timber` stream, so pinning a pattern shifts nothing
    // else. Decorative timber framing is a
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
        TimberPattern::pick(size_class, max_seg_len, &mut seed.stream("timber"))
    });
    place_frame(&mut ctx.on(&mut seed.stream("frame.blocks")), &frame, &timber_pattern).await;
    ctx.editor.start_recording();
    let (gable_doorways, roof_heightmaps) = place_roof(&mut ctx.on(&mut seed.stream("roof.blocks")), seed, &frame, roof_style).await;
    let roof_cells = ctx.editor.stop_recording();
    if has_attic {
        clear_attic_stair_headroom(ctx, &frame, &floor_plan).await;
    }

    // Engawa: lay the wooden veranda deck around the inset building and skirt it
    // with a pent roof at the ground-floor ceiling. Runs after the main roof so
    // the skirt overlays cleanly; the deck planks overwrite the foundation
    // course in the perimeter ring.
    if let Some(plan) = &engawa_plan {
        engawa::place_engawa(&mut ctx.on(&mut seed.stream("engawa.blocks")), &frame, plan).await;
    }

    // Rooms are built before windows so window placement can avoid interior
    // wall cells and attic-ladder walls.
    let mut room_plan = build_rooms(&mut ctx.on(&mut seed.stream("rooms")), &frame, &wall_segs, &floor_plan, has_attic, size_class).await;
    mark_gable_doorways(&mut room_plan, &gable_doorways);
    let ladder_walls = place_attic_ladders(ctx, &mut room_plan, &frame, &floor_plan, &wall_segs, &gable_doorways).await;
    assign_attic_types(&mut room_plan, size_class, &mut seed.stream("attic"));

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
    place_windows(&mut wall_segs, &interior_wall_cells, &mut seed.stream("windows"));
    mark_windows(&mut room_plan, &wall_segs);
    place_openings(&mut ctx.on(&mut seed.stream("curtains")), &wall_segs, window_fill, bctx.culture).await;

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
    place_door_ramps(&mut ctx.on(&mut seed.stream("ramps")), &door_ramps).await;

    assign_room_floors(&mut room_plan);
    place_room_floors(ctx, &frame, &room_plan, bctx).await;


    // Furnish, then harvest the NPC anchor scenes the placed furniture offers
    // (validated against the final per-room CellState inside furnish_rooms).
    // Rooftop terraces and the cellar add their own anchors below.
    let mut npc_anchors = furnish_rooms(&mut ctx.on(&mut seed.stream("furnish")), &mut room_plan, &frame, &roof_heightmaps).await;

    // Flat roofs are open terraces — decorate the deck (shade, seating, plants)
    // once the interior is furnished. Keeps the ladder exit clear.
    if matches!(roof_style, RoofStyle::Flat) {
        npc_anchors.extend(decorate_rooftops(&mut ctx.on(&mut seed.stream("rooftop")), &frame, roof_ladder_wall).await);
    }

    // A few sparse props against the outside walls (barrels, pots, …) so the
    // house reads as lived-in. Skips doors, roads, and claimed cells.
    decorate_exterior_walls(&mut ctx.on(&mut seed.stream("exterior")), &building_footprint, &wall_segs).await;

    check_building_invariants(&frame, &room_plan, &floor_plan, &roof_heightmaps)?;

    // Cellar runs last: it carves below the finished building on its own
    // `cellar` stream, without disturbing the room_plan that blueprint/invariant
    // code iterates.
    // Cellar uses `building_footprint` (the inset walls for an engawa) so its
    // retaining walls sit under the actual walls, not out under the veranda deck.
    let cellar = cellar::maybe_build_cellar(&mut ctx.on(&mut seed.stream("cellar")), &frame, &building_footprint, &wall_segs, &floor_plan, &room_plan, size_class).await;
    let has_cellar = cellar.is_some();
    let cellar_theme = cellar.as_ref().map(|(_, _, theme)| *theme);
    let cellar_stair = cellar.map(|(stair, anchors, _)| {
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
        seed,
        base_y,
        front_door,
        footprint,
        frame,
        wall_segs,
        floor_plan,
        room_plan,
        door_ramps,
        door_entrances,
        has_attic,
        has_cellar,
        cellar_theme,
        roof_cells,
        cellar_stair,
        roof_style,
        size_class,
        timber_pattern,
        npc_anchors,
    })
}

/// Knee-wall rows that leave one visible block of framed infill between the
/// top floor's ceiling beam and the eave. Slab-pitched eaves already sit one
/// row above the ceiling, so one row does it; stair-pitched eaves (and the
/// irimoya, which uses the Stairs surface) hang down onto the ceiling row and
/// would hide a single row, so they lift two (the upper one sits behind the
/// eave). Flat roofs take none: their deck is the top floor's ceiling.
fn knee_rows(roof_style: RoofStyle) -> u32 {
    use super::roof::hipped::HippedPitch;
    match roof_style {
        RoofStyle::Flat => 0,
        RoofStyle::Gable(GablePitch::Slab) | RoofStyle::Hipped(HippedPitch::Slab) => 1,
        RoofStyle::Gable(GablePitch::Stairs | GablePitch::Double)
        | RoofStyle::Hipped(HippedPitch::Stairs)
        | RoofStyle::Irimoya => 2,
    }
}

/// `(floor_base_y, cell)` pairs that NPC anchors must avoid: the interior step
/// and the exterior step of every door. `seg.facing` is the wall's INWARD normal
/// (see `WallSegment::facing`), so the interior side is `door_cell + facing` and
/// the exterior side is `door_cell - facing`. Floor-aware so an upper-room cell
/// directly above a ground-floor door is still eligible.
fn door_keepout_cells(wall_segs: &WallSegments) -> HashSet<(i32, Point2D)> {
    let mut out = HashSet::default();
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
