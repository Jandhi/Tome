mod stairs;

use std::collections::HashSet;

use crate::editor::Editor;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Placer};
use crate::geometry::{Cardinal, Point2D, Point3D};
use crate::minecraft::BlockForm;
use super::footprint::merge::{walk_edge_cells, concave_corner_cells};
use super::frame::Frame;
use super::pipeline::BuildCtx;
use super::walls::WallSegments;

use stairs::{place_stair_blocks, select_stairwells};

/// Whether a stairwell is a straight run or a compact spiral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StairKind {
    Straight,
    Spiral,
    LShaped,
}

/// A stairwell connecting one floor to the floor above.
#[derive(Debug, Clone)]
pub struct Stairwell {
    /// The (x,z) positions occupied by the stairwell.
    /// Straight: position 0 is the landing, 1..=run are steps.
    /// Spiral: 4 cells in CW rotation order, each one step higher.
    pub positions: Vec<Point2D>,
    /// Floor index this stairwell starts on (goes up to floor + 1).
    pub floor: u32,
    /// Direction the stairs ascend toward (straight) or initial facing (spiral).
    pub direction: Cardinal,
    /// Stair type.
    pub kind: StairKind,
}

/// Result of floor/stair placement, consumed by the interior module.
pub struct FloorPlan {
    pub stairwells: Vec<Stairwell>,
    /// Bottom-of-stair landing cells: (floor, x, z). For straight stairs this is
    /// the flat landing cell at position 0. For spiral / L-shaped stairs there is
    /// no flat landing in the stair footprint, so this is the cell directly in
    /// front of the lowest step (one cell back from positions[0] in the direction
    /// opposite the ascent), where the player stands before stepping up.
    pub stair_bottoms: HashSet<(u32, i32, i32)>,
    /// Top-of-stair cells: (floor+1, x, z) for the last position of each stairwell.
    pub stair_tops: HashSet<(u32, i32, i32)>,
}

impl FloorPlan {
    pub fn new(stairwells: Vec<Stairwell>) -> Self {
        let mut stair_bottoms: HashSet<(u32, i32, i32)> = stairwells.iter()
            .filter_map(|sw| sw.positions.first().map(|p| (sw.floor, p.x, p.y)))
            .collect();
        // Spiral / L-shaped stairs have a stair block at positions[0], not a flat
        // landing — also reserve the cell in front of it so furniture can't block
        // the entry. The "front" is opposite the ascent direction (i.e. opposite
        // the second-lowest step from the lowest step).
        for sw in &stairwells {
            if let Some(approach) = sw.bottom_approach() {
                stair_bottoms.insert((sw.floor, approach.x, approach.y));
            }
        }
        let stair_tops: HashSet<(u32, i32, i32)> = stairwells.iter()
            .filter_map(|sw| sw.positions.last().map(|p| (sw.floor + 1, p.x, p.y)))
            .collect();
        Self { stairwells, stair_bottoms, stair_tops }
    }

    pub fn stairwells_on_floor(&self, floor: u32) -> Vec<&Stairwell> {
        self.stairwells.iter().filter(|s| s.floor == floor).collect()
    }

    /// All (x, z) cells occupied by the physical stair blocks of stairwells
    /// that START on the given floor. A cell returned here should be marked
    /// `Blocked` on that floor (except cells called out in `stair_bottoms`,
    /// which stay `BlockedReachable` so the approach/landing remains
    /// adjacent to walkable neighbors).
    ///
    /// Stairs that start on a different floor do **not** contribute — they
    /// have no physical presence on this floor. This is what distinguishes
    /// the main stair's cells on floor 0 from the attic stair's cells,
    /// which only exist on floor 1.
    pub fn stair_cells_on_floor(&self, floor: u32) -> HashSet<(i32, i32)> {
        self.stairwells.iter()
            .filter(|sw| sw.floor == floor)
            .flat_map(|sw| sw.positions.iter().map(|p| (p.x, p.y)))
            .collect()
    }
}

impl Stairwell {
    /// The cell on the lower floor where the player stands before stepping onto
    /// the lowest stair block. Straight stairs already have a flat landing at
    /// positions[0], so they return None. Spiral and L-shaped stairs need an
    /// extra cell — the one adjacent to positions[0] in the direction opposite
    /// the ascent (i.e. opposite positions[1]).
    pub fn bottom_approach(&self) -> Option<Point2D> {
        if self.kind == StairKind::Straight {
            return None;
        }
        let p0 = *self.positions.first()?;
        let back: Point2D = (-self.direction).into();
        Some(p0 + back)
    }
}

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
    place_ceilings(ctx, frame).await;
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
    let material_id = ctx.palette
        .get_material(MaterialRole::PrimaryWood)
        .expect("No primary wood material")
        .clone();

    let mut placer_rng = ctx.rng.derive();
    let mut placer = MaterialPlacer::new(
        Placer::new(&ctx.data.materials, &mut placer_rng),
        material_id,
    );

    for floor in frame.floors() {
        let perimeter = perimeter_cells(frame, floor);
        let y = frame.floor_y(floor) - 1;
        let points = frame.filled_points_at_floor(floor);

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

    let rects = frame.footprint().rects();
    let ground_perimeter = perimeter_cells(frame, 0);
    let mut placed: HashSet<(i32, i32, i32)> = HashSet::new();

    for i in 0..rects.len() {
        let y = frame.roof_y(i) - 2;
        let top_floor = frame.floor_counts()[i].saturating_sub(1);
        let ceil_perimeter = if top_floor == 0 {
            &ground_perimeter
        } else {
            &perimeter_cells(frame, top_floor)
        };
        for point in rects[i].iter() {
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
