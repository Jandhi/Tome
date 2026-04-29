//! Door-ramp pass: reconcile ground-floor doors with uneven terrain.
//!
//! After walls and doors are placed, the cell directly outside a door
//! (`door_cell + facing`) may not line up with the building's base_y:
//!
//! - **Ascending case** (terrain below sill): the player would step out and
//!   fall. A stone ramp runs parallel to the wall, descending one block per
//!   cell until it meets natural grade.
//! - **Descending case** (terrain above sill, door buried in hillside): the
//!   door is blocked by ground. A stone ramp runs parallel to the wall,
//!   ascending one block per cell back up to natural grade, with a carved
//!   air channel above for headroom.
//!
//! The ramp picks whichever side along the wall has room (doesn't collide with
//! the footprint at a concave corner) and runs for `|dy|` steps, capped at
//! `MAX_RAMP_STEPS`. For flat terrain (`dy == 0`) nothing is emitted.

#[cfg(test)]
mod test;

use std::collections::HashMap;

use crate::editor::{Editor, World};
use crate::generator::materials::{MaterialPlacer, MaterialRole, Placer};
use crate::geometry::{Cardinal, Point2D, Point3D};
use crate::minecraft::{Block, BlockForm, BlockID};

use super::footprint::Footprint;
use super::pipeline::BuildCtx;
use super::walls::{OpeningKind, WallSegments, segment_cells};

/// Caps how long a single ramp can run. A 5-block parallel staircase already
/// looks awkward; anything larger probably means the door should be placed
/// elsewhere instead.
const MAX_RAMP_STEPS: i32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RampKind {
    /// Terrain outside the door is below the sill. Steps descend from the door
    /// landing along the wall, meeting natural grade at the far end.
    Ascending,
    /// Terrain outside the door is above the sill. Steps ascend from the door
    /// landing along the wall; each step has air carved above it for headroom.
    Descending,
}

/// One stair step in a door ramp.
#[derive(Debug, Clone)]
pub struct RampStep {
    pub cell: Point2D,
    /// Y level of the stair block.
    pub y: i32,
    /// Terrain Y at `cell` at plan time. Used for fill/carve bounds.
    pub terrain_y: i32,
}

/// A ramp attached to one door to reconcile its sill with uneven terrain.
#[derive(Debug, Clone)]
pub struct DoorRamp {
    /// Wall cell carrying the door block.
    pub door_cell: Point2D,
    /// Cell directly outside the door (sits at `landing_y`).
    pub landing_cell: Point2D,
    /// Y of the door sill (= base_y of the ground-floor wall segment).
    pub landing_y: i32,
    /// Terrain Y sampled at `landing_cell` before placement.
    pub landing_terrain_y: i32,
    /// Outward normal of the wall (the direction that leads away from the
    /// building's interior through the door). Equal to `-seg.facing`.
    pub wall_facing: Cardinal,
    /// Along-wall direction the stair run extends, away from the door.
    pub side_dir: Cardinal,
    /// `facing` property for each stair block. Points toward the door for
    /// Ascending ramps (player walks toward door to climb) and away from it
    /// for Descending ramps.
    pub stair_facing: Cardinal,
    pub kind: RampKind,
    pub steps: Vec<RampStep>,
}

/// Plan door ramps for all ground-floor doors. Pure function — does not mutate
/// anything. `get_terrain_y` samples the terrain height column at a cell.
pub fn plan_door_ramps(
    wall_segs: &WallSegments,
    footprint: &Footprint,
    get_terrain_y: impl Fn(Point2D) -> i32,
) -> Vec<DoorRamp> {
    let mut ramps = Vec::new();
    for (seg, opening) in wall_segs.doors() {
        if seg.floor != 0 { continue; }
        if !matches!(opening.kind, OpeningKind::Door(_)) { continue; }

        let cells = segment_cells(seg);
        let door_idx = opening.offset as usize;
        if door_idx >= cells.len() || cells.len() < 2 { continue; }

        // `seg.facing` is the INWARD normal (points from wall toward interior),
        // matching the Minecraft door-block `facing` convention used in
        // `place_openings`. Outward is the opposite direction.
        let outward = -seg.facing;
        let door_cell = cells[door_idx];
        let facing_offset: Point2D = outward.into();
        let landing_cell = door_cell + facing_offset;
        let landing_y = seg.base_y;

        let landing_terrain_y = get_terrain_y(landing_cell);
        let dy = landing_terrain_y - landing_y;
        if dy == 0 { continue; }

        // Along-wall direction, derived from consecutive segment cells.
        let Some(along) = Cardinal::from_point_2d(cells[1] - cells[0]) else { continue };
        let side_offset = |c: Cardinal| -> Point2D { c.into() };

        // For each candidate side, count how many consecutive cells away from
        // the door are NOT inside the footprint (avoids running the ramp into
        // a concave corner).
        let needed = dy.abs().min(MAX_RAMP_STEPS) as usize;
        let count_available = |side: Cardinal| -> usize {
            let off = side_offset(side);
            (1..=needed).take_while(|&k| !footprint.contains(landing_cell + off * k as i32)).count()
        };
        let plus_avail = count_available(along);
        let minus_avail = count_available(-along);
        let (side_dir, avail) = if plus_avail >= minus_avail {
            (along, plus_avail)
        } else {
            (-along, minus_avail)
        };
        if avail == 0 { continue; }

        let kind = if dy < 0 { RampKind::Ascending } else { RampKind::Descending };
        let stair_facing = match kind {
            RampKind::Ascending => -side_dir,
            RampKind::Descending => side_dir,
        };

        let off = side_offset(side_dir);
        let steps: Vec<RampStep> = (1..=avail).map(|k| {
            let cell = landing_cell + off * k as i32;
            let y = match kind {
                RampKind::Ascending => landing_y - k as i32,
                RampKind::Descending => landing_y + k as i32,
            };
            let terrain_y = get_terrain_y(cell);
            RampStep { cell, y, terrain_y }
        }).collect();

        ramps.push(DoorRamp {
            door_cell,
            landing_cell,
            landing_y,
            landing_terrain_y,
            wall_facing: outward,
            side_dir,
            stair_facing,
            kind,
            steps,
        });
    }
    ramps
}

/// Plan ramps from a World's heightmap (convenience wrapper).
pub fn plan_door_ramps_from_world(
    wall_segs: &WallSegments,
    footprint: &Footprint,
    world: &World,
) -> Vec<DoorRamp> {
    plan_door_ramps(wall_segs, footprint, |p| world.get_ocean_floor_height_at(p))
}

/// Place the planned ramps into the editor. Ascending ramps fill stone under
/// the landing and steps; descending ramps carve air above them. All stair
/// blocks use `PrimaryStone` from the palette.
pub async fn place_door_ramps(ctx: &mut BuildCtx<'_>, ramps: &[DoorRamp]) {
    if ramps.is_empty() { return; }

    let editor: &Editor = &*ctx.editor;
    let stone_id = ctx.palette
        .get_material(MaterialRole::PrimaryStone)
        .expect("No primary stone material")
        .clone();

    let mut placer_rng = ctx.rng.derive();
    let mut stone = MaterialPlacer::new(
        Placer::new(&ctx.data.materials, &mut placer_rng),
        stone_id,
    );
    let air = Block::new(BlockID::default(), None, None);

    for ramp in ramps {
        place_landing(editor, &mut stone, &air, ramp).await;
        place_steps(editor, &mut stone, &air, ramp).await;
    }
}

async fn place_landing(
    editor: &Editor,
    stone: &mut MaterialPlacer<'_>,
    air: &Block,
    ramp: &DoorRamp,
) {
    let cell = ramp.landing_cell;
    match ramp.kind {
        RampKind::Ascending => {
            // Fill stone from terrain up to (but not including) landing_y.
            for fill_y in ramp.landing_terrain_y..ramp.landing_y {
                stone.place_block(
                    editor,
                    Point3D::new(cell.x, fill_y, cell.y),
                    BlockForm::Block,
                    None,
                    None,
                ).await;
            }
        }
        RampKind::Descending => {
            // Carve air from landing_y up to terrain+2 (headroom above door).
            let top = ramp.landing_terrain_y + 2;
            for carve_y in ramp.landing_y..=top {
                editor.place_block_forced(
                    air,
                    Point3D::new(cell.x, carve_y, cell.y),
                ).await;
            }
            // Guarantee a solid floor immediately under the sill.
            stone.place_block_forced(
                editor,
                Point3D::new(cell.x, ramp.landing_y - 1, cell.y),
                BlockForm::Block,
                None,
                None,
            ).await;
        }
    }
}

async fn place_steps(
    editor: &Editor,
    stone: &mut MaterialPlacer<'_>,
    air: &Block,
    ramp: &DoorRamp,
) {
    let stair_state = HashMap::from([
        ("facing".to_string(), ramp.stair_facing.to_string()),
    ]);

    for step in &ramp.steps {
        match ramp.kind {
            RampKind::Ascending => {
                // Fill stone beneath the step up to just under the stair.
                for fill_y in step.terrain_y..step.y {
                    stone.place_block(
                        editor,
                        Point3D::new(step.cell.x, fill_y, step.cell.y),
                        BlockForm::Block,
                        None,
                        None,
                    ).await;
                }
            }
            RampKind::Descending => {
                // Carve air above the step up to terrain+2 for headroom.
                let top = step.terrain_y + 2;
                for carve_y in (step.y + 1)..=top {
                    editor.place_block_forced(
                        air,
                        Point3D::new(step.cell.x, carve_y, step.cell.y),
                    ).await;
                }
                // Make sure the block directly under the stair is solid —
                // terrain may be exactly at step.y (natural support) or below
                // (needs fill after the carve).
                if step.terrain_y < step.y {
                    for fill_y in step.terrain_y..step.y {
                        stone.place_block(
                            editor,
                            Point3D::new(step.cell.x, fill_y, step.cell.y),
                            BlockForm::Block,
                            None,
                            None,
                        ).await;
                    }
                }
            }
        }
        // Stair block itself.
        stone.place_block_forced(
            editor,
            Point3D::new(step.cell.x, step.y, step.cell.y),
            BlockForm::Stairs,
            Some(&stair_state),
            None,
        ).await;
    }
}
