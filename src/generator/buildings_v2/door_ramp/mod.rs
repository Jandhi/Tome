//! Door-ramp pass: reconcile ground-floor doors with uneven terrain.
//!
//! After walls and doors are placed, the cell directly outside a door
//! (`door_cell + facing`) may not line up with the building's base_y:
//!
//! - **Ascending case** (terrain below sill): the player would step out and
//!   fall. A stone ramp descends one block per step until it meets natural
//!   grade.
//! - **Descending case** (terrain above sill, door buried in hillside): the
//!   door is blocked by ground. A stone ramp ascends one block per step back
//!   up to natural grade, with a carved air channel above for headroom.
//!
//! Layout depends on door style:
//!
//! - **Single door (width=1)**: the ramp runs *parallel* to the wall, picking
//!   whichever along-wall side has room (avoids concave corners). 1 cell wide.
//! - **Double door (width=2)**: the ramp runs *perpendicular* to the wall,
//!   straight outward. The two door-adjacent cells plus one more outward row
//!   form a 2×2 platform at sill height; from there the stairs continue 2
//!   wide outward until they reach grade.
//!
//! Each run is capped at `MAX_RAMP_STEPS`. For flat terrain (`dy == 0`)
//! nothing is emitted.

#[cfg(test)]
mod test;

use std::collections::HashMap;

use crate::editor::{Editor, World};
use crate::generator::materials::{MaterialPlacer, MaterialRole, Placer};
use crate::geometry::{Cardinal, Point2D, Point3D};
use crate::minecraft::{Block, BlockForm, BlockID};

use super::footprint::Footprint;
use super::pipeline::BuildCtx;
use super::walls::{DoorStyle, OpeningKind, WallSegments, segment_cells};

/// Caps how long a single ramp can run. A 5-block staircase already looks
/// awkward; anything larger probably means the door should be placed elsewhere.
const MAX_RAMP_STEPS: i32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RampKind {
    /// Terrain outside the door is below the sill. Steps descend from the
    /// landing toward natural grade.
    Ascending,
    /// Terrain outside the door is above the sill. Steps ascend from the
    /// landing back up to natural grade; air is carved above each step.
    Descending,
}

/// One stair step (or platform tile) in a door ramp. For width=2 ramps, this
/// is the *anchor* cell — the paired cell sits at `cell + wide_dir`.
#[derive(Debug, Clone)]
pub struct RampStep {
    pub cell: Point2D,
    /// Y level of the stair block (or platform tile, if part of the platform).
    pub y: i32,
    /// Terrain Y at the anchor `cell` at plan time.
    pub terrain_y: i32,
    /// Terrain Y at the paired cell (`cell + wide_dir`) for width=2 ramps.
    /// `None` for width=1.
    pub pair_terrain_y: Option<i32>,
}

/// A ramp attached to one door to reconcile its sill with uneven terrain.
#[derive(Debug, Clone)]
pub struct DoorRamp {
    /// Wall cell carrying the door block. For double doors, the leftmost of
    /// the two door cells (the paired cell is `door_cell + wide_dir`).
    pub door_cell: Point2D,
    /// 1 for single doors, 2 for double doors.
    pub width: i32,
    /// Along-wall axis. For width=2 ramps, the second cell of every tile sits
    /// at `cell + wide_dir`. For width=1 this is set to the segment's
    /// along-wall direction but is otherwise unused.
    pub wide_dir: Cardinal,
    /// Anchor cell of the landing (directly outside `door_cell`).
    pub landing_cell: Point2D,
    /// Y of the door sill (= base_y of the ground-floor wall segment).
    pub landing_y: i32,
    /// Terrain Y sampled at `landing_cell` before placement.
    pub landing_terrain_y: i32,
    /// Terrain Y at the paired landing cell, for width=2. `None` for width=1.
    pub landing_pair_terrain_y: Option<i32>,
    /// Outward normal of the wall (the direction that leads away from the
    /// building's interior through the door). Equal to `-seg.facing`.
    pub wall_facing: Cardinal,
    /// Direction the ramp extends from the landing.
    /// - width=1: along-wall side direction (chosen to avoid concave corners).
    /// - width=2: equal to `wall_facing` (perpendicular outward).
    pub side_dir: Cardinal,
    /// `facing` property for each stair block. Points toward the door for
    /// Ascending ramps (player walks toward the door to climb) and away from
    /// it for Descending ramps.
    pub stair_facing: Cardinal,
    pub kind: RampKind,
    /// For width=2 ramps, an extra row of cells at `landing_y` between the
    /// landing and the first stair step. Combined with the landing this
    /// forms the 2×2 sill-height platform. `None` for width=1.
    pub platform_extension: Option<RampStep>,
    pub steps: Vec<RampStep>,
}

/// Plan door ramps for all ground-floor doors. Pure function — does not
/// mutate anything. `get_terrain_y` samples the terrain height column at a
/// cell.
pub fn plan_door_ramps(
    wall_segs: &WallSegments,
    footprint: &Footprint,
    get_terrain_y: impl Fn(Point2D) -> i32,
) -> Vec<DoorRamp> {
    let mut ramps = Vec::new();
    for (seg, opening) in wall_segs.doors() {
        if seg.floor != 0 { continue; }
        let style = match opening.kind {
            OpeningKind::Door(s) => s,
            _ => continue,
        };

        let cells = segment_cells(seg);
        let door_idx = opening.offset as usize;
        if door_idx >= cells.len() || cells.len() < 2 { continue; }

        // `seg.facing` is the INWARD normal (points from wall toward
        // interior), matching the Minecraft door-block `facing` convention
        // used in `place_openings`. Outward is the opposite direction.
        let outward = -seg.facing;
        let door_cell = cells[door_idx];
        let outward_off: Point2D = outward.into();
        let landing_cell = door_cell + outward_off;
        let landing_y = seg.base_y;

        let landing_terrain_y = get_terrain_y(landing_cell);
        let dy = landing_terrain_y - landing_y;
        if dy == 0 { continue; }
        let kind = if dy < 0 { RampKind::Ascending } else { RampKind::Descending };

        // Along-wall direction, derived from consecutive segment cells.
        let Some(along) = Cardinal::from_point_2d(cells[1] - cells[0]) else { continue };
        let needed = dy.abs().min(MAX_RAMP_STEPS) as usize;

        if matches!(style, DoorStyle::Double) {
            // Double doors get a 2-wide perpendicular-outward stair with a
            // 2×2 platform at sill height.
            if door_idx + 1 >= cells.len() { continue; }
            let wide_dir = along;
            let wide_off: Point2D = wide_dir.into();

            let landing_pair_terrain_y = Some(get_terrain_y(landing_cell + wide_off));

            // Ascending double-doors get a 2x2 sill-level platform (landing +
            // one row outward) before the stairs descend to grade. Descending
            // double-doors don't — the door is below terrain, so stairs run
            // directly up from the landing.
            let (platform_extension, step_run_offset) = match kind {
                RampKind::Ascending => {
                    let plat_anchor = landing_cell + outward_off;
                    let plat_pair = plat_anchor + wide_off;
                    if footprint.contains(plat_anchor) || footprint.contains(plat_pair) {
                        continue;
                    }
                    let ext = RampStep {
                        cell: plat_anchor,
                        y: landing_y,
                        terrain_y: get_terrain_y(plat_anchor),
                        pair_terrain_y: Some(get_terrain_y(plat_pair)),
                    };
                    (Some(ext), 2)
                }
                RampKind::Descending => (None, 1),
            };

            let mut steps = Vec::new();
            for k_step in 1..=needed {
                let run_k = (k_step as i32 - 1) + step_run_offset;
                let cell = landing_cell + outward_off * run_k;
                let pair = cell + wide_off;
                if footprint.contains(cell) || footprint.contains(pair) { break; }
                let y = match kind {
                    RampKind::Ascending => landing_y - k_step as i32,
                    RampKind::Descending => landing_y + (k_step as i32 - 1),
                };
                steps.push(RampStep {
                    cell,
                    y,
                    terrain_y: get_terrain_y(cell),
                    pair_terrain_y: Some(get_terrain_y(pair)),
                });
            }
            if steps.is_empty() { continue; }

            let stair_facing = match kind {
                RampKind::Ascending => -outward,
                RampKind::Descending => outward,
            };

            ramps.push(DoorRamp {
                door_cell,
                width: 2,
                wide_dir,
                landing_cell,
                landing_y,
                landing_terrain_y,
                landing_pair_terrain_y,
                wall_facing: outward,
                side_dir: outward,
                stair_facing,
                kind,
                platform_extension,
                steps,
            });
        } else {
            // Single doors: 1-wide stair parallel to the wall. Pick whichever
            // along-wall side has more room before hitting the footprint
            // (concave corner).
            let count_available = |side: Cardinal| -> usize {
                let off: Point2D = side.into();
                (1..=needed)
                    .take_while(|&k| !footprint.contains(landing_cell + off * k as i32))
                    .count()
            };
            let plus_avail = count_available(along);
            let minus_avail = count_available(-along);
            let (side_dir, avail) = if plus_avail >= minus_avail {
                (along, plus_avail)
            } else {
                (-along, minus_avail)
            };
            if avail == 0 { continue; }

            let stair_facing = match kind {
                RampKind::Ascending => -side_dir,
                RampKind::Descending => side_dir,
            };

            let off: Point2D = side_dir.into();
            let steps: Vec<RampStep> = (1..=avail).map(|k| {
                let cell = landing_cell + off * k as i32;
                let y = match kind {
                    RampKind::Ascending => landing_y - k as i32,
                    RampKind::Descending => landing_y + (k as i32 - 1),
                };
                let terrain_y = get_terrain_y(cell);
                RampStep { cell, y, terrain_y, pair_terrain_y: None }
            }).collect();

            ramps.push(DoorRamp {
                door_cell,
                width: 1,
                wide_dir: along,
                landing_cell,
                landing_y,
                landing_terrain_y,
                landing_pair_terrain_y: None,
                wall_facing: outward,
                side_dir,
                stair_facing,
                kind,
                platform_extension: None,
                steps,
            });
        }
    }
    ramps
}

/// The exterior entrance cell for every ground-floor door — the cell a player
/// stands on when they step out and reach natural grade. For a door with a ramp
/// this is one cell past the bottom of the ramp; for a flat door it's the
/// landing directly outside. Used to start door→road connector paths from the
/// real entrance instead of guessing.
pub fn door_entrances(wall_segs: &WallSegments, ramps: &[DoorRamp]) -> Vec<Point2D> {
    use std::collections::HashMap;
    let ramp_by_door: HashMap<Point2D, &DoorRamp> =
        ramps.iter().map(|r| (r.door_cell, r)).collect();

    let mut entrances = Vec::new();
    for (seg, opening) in wall_segs.doors() {
        if seg.floor != 0 {
            continue;
        }
        let cells = segment_cells(seg);
        let door_idx = opening.offset as usize;
        let Some(&door_cell) = cells.get(door_idx) else { continue };
        let outward: Point2D = (-seg.facing).into();
        let landing = door_cell + outward;

        let entrance = match ramp_by_door.get(&door_cell) {
            // One cell past the last step, where the ramp meets grade.
            Some(ramp) => match ramp.steps.last() {
                Some(last) => last.cell + Point2D::from(ramp.side_dir),
                None => ramp.landing_cell,
            },
            None => landing,
        };
        entrances.push(entrance);
    }
    entrances
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
/// the landing and steps; descending ramps carve air above them. All blocks
/// use `PrimaryStone` from the palette.
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
        if let Some(ext) = &ramp.platform_extension {
            place_platform_tile(editor, &mut stone, &air, ramp, ext).await;
        }
        place_steps(editor, &mut stone, &air, ramp).await;
    }
}

/// Returns the cell-and-terrain pairs for a tile, accounting for width.
fn tile_cells(
    ramp: &DoorRamp,
    anchor: Point2D,
    terrain_anchor: i32,
    terrain_pair: Option<i32>,
) -> Vec<(Point2D, i32)> {
    let mut out = Vec::with_capacity(ramp.width as usize);
    out.push((anchor, terrain_anchor));
    if ramp.width == 2 {
        let off: Point2D = ramp.wide_dir.into();
        let pair_terrain = terrain_pair.expect("width=2 ramp missing pair terrain");
        out.push((anchor + off, pair_terrain));
    }
    out
}

async fn place_landing(
    editor: &Editor,
    stone: &mut MaterialPlacer<'_>,
    air: &Block,
    ramp: &DoorRamp,
) {
    let cells = tile_cells(
        ramp,
        ramp.landing_cell,
        ramp.landing_terrain_y,
        ramp.landing_pair_terrain_y,
    );
    for (cell, terrain_y) in cells {
        place_sill_tile(editor, stone, air, ramp, cell, terrain_y).await;
    }
}

/// A tile at sill height — used for the landing and (for double doors) the
/// platform extension. Ascending: fill stone from terrain up to the sill.
/// Descending: carve air for headroom and put a stone block under the sill.
async fn place_sill_tile(
    editor: &Editor,
    stone: &mut MaterialPlacer<'_>,
    air: &Block,
    ramp: &DoorRamp,
    cell: Point2D,
    terrain_y: i32,
) {
    let y = ramp.landing_y;
    match ramp.kind {
        RampKind::Ascending => {
            for fill_y in terrain_y..y {
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
            let top = terrain_y + 2;
            for carve_y in y..=top {
                editor.place_block_forced(
                    air,
                    Point3D::new(cell.x, carve_y, cell.y),
                ).await;
            }
            stone.place_block_forced(
                editor,
                Point3D::new(cell.x, y - 1, cell.y),
                BlockForm::Block,
                None,
                None,
            ).await;
        }
    }
}

async fn place_platform_tile(
    editor: &Editor,
    stone: &mut MaterialPlacer<'_>,
    air: &Block,
    ramp: &DoorRamp,
    ext: &RampStep,
) {
    let cells = tile_cells(ramp, ext.cell, ext.terrain_y, ext.pair_terrain_y);
    for (cell, terrain_y) in cells {
        place_sill_tile(editor, stone, air, ramp, cell, terrain_y).await;
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
        let cells = tile_cells(ramp, step.cell, step.terrain_y, step.pair_terrain_y);
        for (cell, terrain_y) in cells {
            match ramp.kind {
                RampKind::Ascending => {
                    for fill_y in terrain_y..step.y {
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
                    let top = terrain_y + 2;
                    for carve_y in (step.y + 1)..=top {
                        editor.place_block_forced(
                            air,
                            Point3D::new(cell.x, carve_y, cell.y),
                        ).await;
                    }
                    if terrain_y < step.y {
                        for fill_y in terrain_y..step.y {
                            stone.place_block(
                                editor,
                                Point3D::new(cell.x, fill_y, cell.y),
                                BlockForm::Block,
                                None,
                                None,
                            ).await;
                        }
                    }
                }
            }
            stone.place_block_forced(
                editor,
                Point3D::new(cell.x, step.y, cell.y),
                BlockForm::Stairs,
                Some(&stair_state),
                None,
            ).await;
        }
    }
}
