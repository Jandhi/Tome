//! Floor, ceiling, and stair placement. Lays floor slabs (skipping stair
//! openings and exterior walls), ceilings, and stair blocks, re-carves attic
//! stair headroom after the roof goes on, and paints custom room floors.

use std::collections::HashSet;

use crate::editor::Editor;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Placer};
use crate::geometry::Point3D;
use crate::minecraft::BlockForm;

use super::super::footprint::merge::{concave_corner_cells, walk_edge_cells};
use super::super::frame::Frame;
use super::super::pipeline::BuildCtx;
use super::super::walls::WallSegments;
use super::plan::{FloorPlan, StairKind};
use super::stairs::{place_stair_blocks, select_stairwells};

/// Compute the set of perimeter (exterior wall) cells for a given floor.
/// These cells should not receive floor/ceiling blocks.
fn perimeter_cells(frame: &Frame, floor: u32) -> HashSet<(i32, i32)> {
    let outline = frame.outline_at_floor(floor);
    let n = outline.len();
    let mut cells = HashSet::new();
    for i in 0..n {
        let start = outline[i];
        let end = outline[(i + 1) % n];
        for cell in walk_edge_cells(start, end) {
            cells.insert((cell.x, cell.y));
        }
    }
    for cell in concave_corner_cells(&outline) {
        cells.insert((cell.x, cell.y));
    }
    cells
}

/// Place floor slabs, ceilings, and stairs. Returns a FloorPlan with stairwell info.
/// When `has_attic` is true, an extra stairwell is placed from the top floor
/// into the attic space under a double-pitch roof.
pub async fn place_floors(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
    wall_segs: &WallSegments,
    has_attic: bool,
    skip_ceilings: bool,
) -> FloorPlan {
    let stairwells = select_stairwells(frame, wall_segs, has_attic, ctx.rng);

    // Stairwell openings per Y level — cells to skip when laying floor slabs.
    // A stairwell on floor N needs an opening in the slab of floor N+1.
    let mut openings: HashSet<(i32, i32, i32)> = HashSet::new();
    for sw in &stairwells {
        let slab_y = frame.floor_y(sw.floor + 1) - 1;
        for pos in &sw.positions {
            openings.insert((pos.x, slab_y, pos.y));
        }
    }

    place_floor_slabs(ctx, frame, &openings).await;
    if !skip_ceilings {
        place_ceilings(ctx, frame).await;
    }
    place_stair_blocks(ctx, &stairwells, frame).await;

    FloorPlan::new(stairwells)
}

/// Place floor slabs for every floor, skipping stairwell openings and perimeter
/// (exterior wall) cells. Ground-floor planks overwrite the foundation stone
/// for interior cells.
async fn place_floor_slabs(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
    openings: &HashSet<(i32, i32, i32)>,
) {
    let editor: &Editor = &*ctx.editor;

    let ground_material = ctx.palette
        .get_material(MaterialRole::GroundFloor)
        .expect("No floor material (GroundFloor → PrimaryWood fallback missing)")
        .clone();
    let upper_material = ctx.palette
        .get_material(MaterialRole::UpperFloor)
        .expect("No floor material (UpperFloor → GroundFloor → PrimaryWood fallback missing)")
        .clone();

    let mut ground_rng = ctx.rng.derive();
    let mut ground_placer = MaterialPlacer::new(
        Placer::new(&ctx.data.materials, &mut ground_rng),
        ground_material,
    );
    let mut upper_rng = ctx.rng.derive();
    let mut upper_placer = MaterialPlacer::new(
        Placer::new(&ctx.data.materials, &mut upper_rng),
        upper_material,
    );

    for floor in frame.floors() {
        let perimeter = perimeter_cells(frame, floor);
        let y = frame.floor_y(floor) - 1;
        let points = frame.filled_points_at_floor(floor);
        let placer = if floor == 0 { &mut ground_placer } else { &mut upper_placer };

        for point in &points {
            if openings.contains(&(point.x, y, point.y)) { continue; }
            if perimeter.contains(&(point.x, point.y)) { continue; }
            placer.place_block_forced(
                editor,
                Point3D::new(point.x, y, point.y),
                BlockForm::Block,
                None,
                None,
            ).await;
        }
    }
}

/// Place ceiling blocks at the top of each rect (one block below roof_y),
/// skipping the exterior-wall perimeter.
async fn place_ceilings(ctx: &mut BuildCtx<'_>, frame: &Frame) {
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

    let ground_perimeter = perimeter_cells(frame, 0);
    let mut placed: HashSet<(i32, i32, i32)> = HashSet::new();

    for i in 0..frame.rect_count() {
        let y = frame.roof_y(i) - 2;
        let top_floor = frame.floor_counts()[i].saturating_sub(1);
        let ceil_perimeter = if top_floor == 0 {
            &ground_perimeter
        } else {
            &perimeter_cells(frame, top_floor)
        };
        let Some(rect) = frame.rect_at(i, top_floor) else { continue; };
        for point in rect.iter() {
            if ceil_perimeter.contains(&(point.x, point.y)) { continue; }
            if placed.insert((point.x, y, point.y)) {
                placer.place_block(
                    editor,
                    Point3D::new(point.x, y, point.y),
                    BlockForm::Block,
                    None,
                    None,
                ).await;
            }
        }
    }
}

/// Re-clear air above attic stair positions after the roof has been placed.
/// The roof overwrites the air that `place_floors` cleared, so this must run
/// after `place_roof` to carve headroom through the roof blocks.
pub async fn clear_attic_stair_headroom(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
    floor_plan: &FloorPlan,
) {
    let editor: &Editor = &*ctx.editor;
    if frame.max_floors() < 1 {
        return;
    }
    let top_floor = frame.max_floors() - 1;

    for sw in &floor_plan.stairwells {
        if sw.floor != top_floor {
            continue;
        }
        let base_y = frame.floor_y(sw.floor);

        for (i, pos) in sw.positions.iter().enumerate() {
            let step_offset = match sw.kind {
                StairKind::Straight => {
                    if i == 0 { continue; } // landing, no step block
                    if i == 1 {
                        // Clear the ceiling/floor block above the lowest step
                        // so the player can access the stairs.
                        let ceil_y = frame.roof_y(0) - 2;
                        editor.place_block_forced(
                            &"air".into(),
                            Point3D::new(pos.x, ceil_y, pos.y),
                        ).await;
                    }
                    (i - 1) as i32
                }
                _ => i as i32,
            };
            let y = base_y + step_offset;
            for clear_y in (y + 1)..=(y + 2) {
                editor.place_block_forced(
                    &"air".into(),
                    Point3D::new(pos.x, clear_y, pos.y),
                ).await;
            }
        }

    }
}

/// Place custom room floors (e.g. glazed terracotta) for rooms that have a
/// `floor_type` set. Runs after room type assignment and before furnishing.
pub async fn place_room_floors(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
    room_plan: &super::super::rooms::RoomPlan,
    bctx: &super::super::BuildingContext,
) {
    use crate::minecraft::{Block, Color};
    use super::super::{Culture, FloorType};
    use std::collections::HashMap;

    let editor: &Editor = &*ctx.editor;
    let palette = ctx.palette;

    for room in &room_plan.rooms {
        let floor_type = match room.floor_type {
            Some(ft) => ft,
            None => continue,
        };

        let interior = room.interior;
        if interior.size.x <= 0 || interior.size.y <= 0 { continue; }

        let y = frame.floor_y(room.floor) - 1;

        match floor_type {
            FloorType::Kitchen => {
                match bctx.culture {
                    Culture::Desert => {
                        // Glazed terracotta with 2x2 rotating pattern
                        let color: Color = palette.primary_color
                            .unwrap_or(Color::White);
                        let color_str: String = color.into();
                        let block_id_str = format!("minecraft:{}_glazed_terracotta", color_str);
                        let block_id: crate::minecraft::BlockID = block_id_str.as_str().into();

                        // 2x2 clockwise rotation pattern:
                        //   x=0,z=0 → north    x=1,z=0 → east
                        //   x=0,z=1 → west     x=1,z=1 → south
                        let pattern: [[&str; 2]; 2] = [
                            ["north", "west"],   // x=0: z=0, z=1
                            ["east",  "south"],  // x=1: z=0, z=1
                        ];

                        for point in interior.iter() {
                            let qx = point.x.rem_euclid(2) as usize;
                            let qz = point.y.rem_euclid(2) as usize;
                            let facing = pattern[qx][qz];

                            let state = HashMap::from([
                                ("facing".to_string(), facing.to_string()),
                            ]);
                            let block = Block::new(
                                block_id.clone(),
                                Some(state),
                                None,
                            );
                            editor.place_block_forced(&block, Point3D::new(point.x, y, point.y)).await;
                        }
                    }
                    _ => {
                        // Stone bricks for temperate kitchens
                        let block = Block::from_id("minecraft:stone_bricks".into());
                        for point in interior.iter() {
                            editor.place_block_forced(&block, Point3D::new(point.x, y, point.y)).await;
                        }
                    }
                }
            }
        }
    }
}
