//! Flat roofs: a slab roof deck per rect with a styled perimeter parapet, plus
//! the roof-access ladder that climbs from the top floor onto the deck.

use crate::editor::Editor;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Placer};
use crate::geometry::{Point2D, Point3D};
use crate::minecraft::{Block, BlockForm};

use super::super::frame::Frame;
use super::super::pipeline::BuildCtx;
use super::dome::{is_dome_eligible, place_dome};
use super::heightmap::RoofHeightmap;
use super::top_floor_rects;

/// Visual style for flat-roof parapets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParapetStyle {
    /// Alternating full blocks and gaps (classic battlement).
    Crenellated,
    /// 2-block pillars at corners, 1-block walls between.
    CornerPillars,
    /// Thin wall blocks instead of full blocks.
    ThinWalls,
    /// Full blocks at intervals, top slabs filling the gaps between.
    SlabTopped,
}

/// Flat roof: places a solid slab layer at roof_y for each rect, with a
/// 1-block-high parapet wall around the perimeter using PrimaryStone.
pub(super) async fn place_flat_roof(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
) -> (Vec<Point2D>, Vec<RoofHeightmap>) {
    let editor: &Editor = &*ctx.editor;
    let data = ctx.data;
    let palette = ctx.palette;
    let rng = &mut *ctx.rng;

    let rects = top_floor_rects(frame);
    let rects = &rects[..];

    let mut placer_rng = rng.derive();
    let roof_material = palette
        .get_material(MaterialRole::PrimaryRoof)
        .unwrap_or_else(|| palette.get_material(MaterialRole::PrimaryStone).expect("No roof or stone material"))
        .clone();
    let mut roof_placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng),
        roof_material,
    );

    let mut parapet_rng = rng.derive();
    // Parapet draws on the Accent role, which backs off to PrimaryStone when a
    // palette defines no accent (so plain sandstone styles are unchanged). A
    // style that *does* set an accent (e.g. green terracotta) gets a coloured
    // crown — see the solid-band branch below.
    let has_accent = palette.materials.contains_key(&MaterialRole::Accent);
    let parapet_material = palette
        .get_material(MaterialRole::Accent)
        .expect("No accent or stone material for parapet")
        .clone();
    let mut parapet_placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut parapet_rng),
        parapet_material,
    );

    // Pick a parapet style for this building
    let parapet_style = match rng.rand_i32_range(0, 4) {
        0 => ParapetStyle::Crenellated,
        1 => ParapetStyle::CornerPillars,
        2 => ParapetStyle::ThinWalls,
        _ => ParapetStyle::SlabTopped,
    };

    // Map each point to the roof_y of the tallest rect containing it, and track
    // which cells belong to a dome. A dome is its own enclosed structure, so a
    // flat rect walls off (parapets) against a dome at the *same* height — but a
    // *taller* dome already has its own wall, so the flat rect leaves that edge
    // open and butts against the wall instead.
    let mut point_roof_y: std::collections::HashMap<Point2D, i32> = std::collections::HashMap::new();
    let mut dome_cells: std::collections::HashSet<Point2D> = std::collections::HashSet::new();
    for i in 0..rects.len() {
        let ry = frame.roof_y(i);
        let is_dome = is_dome_eligible(&rects[i]);
        for point in rects[i].iter() {
            let entry = point_roof_y.entry(point).or_insert(ry);
            if ry > *entry { *entry = ry; }
            if is_dome { dome_cells.insert(point); }
        }
    }

    // Collect parapet cells per rect (with their roof_y) before placing,
    // so we can detect corners for CornerPillars style.
    let mut parapet_cells: Vec<(Point2D, i32)> = Vec::new();

    // Place roof blocks and identify parapet cells per rect
    for i in 0..rects.len() {
        let rect = &rects[i];
        let roof_y = frame.roof_y(i);

        // Square rects get a dome instead of the flat deck + parapet.
        if is_dome_eligible(rect) {
            place_dome(editor, rect, roof_y - 2).await;
            continue;
        }

        for point in rect.iter() {
            // Roof surface: full block replaces where the ceiling would be
            roof_placer
                .place_block(editor, point.add_y(roof_y - 2), BlockForm::Block, None, None)
                .await;

            // Parapet: place on cells at the footprint border OR where a
            // neighbor cell belongs to a lower rect.
            let neighbors = [
                Point2D::new(point.x - 1, point.y),
                Point2D::new(point.x + 1, point.y),
                Point2D::new(point.x, point.y - 1),
                Point2D::new(point.x, point.y + 1),
                Point2D::new(point.x - 1, point.y - 1),
                Point2D::new(point.x + 1, point.y - 1),
                Point2D::new(point.x - 1, point.y + 1),
                Point2D::new(point.x + 1, point.y + 1),
            ];
            let needs_parapet = neighbors.iter().any(|n| {
                match point_roof_y.get(n) {
                    None => true,
                    // Lower neighbour, or a same-height dome (its own structure).
                    // A taller dome (ny > roof_y) is left open — its wall divides.
                    Some(&ny) => ny < roof_y || (ny == roof_y && dome_cells.contains(n)),
                }
            });

            if needs_parapet {
                parapet_cells.push((point, roof_y));
            }
        }
    }

    // Build a set for fast corner detection
    let parapet_set: std::collections::HashSet<Point2D> =
        parapet_cells.iter().map(|(p, _)| *p).collect();

    // A parapet cell is a corner if it has parapet neighbors on two
    // perpendicular cardinal axes (L-shaped or more).
    let is_corner = |p: Point2D| -> bool {
        let has_x = parapet_set.contains(&Point2D::new(p.x - 1, p.y))
                 || parapet_set.contains(&Point2D::new(p.x + 1, p.y));
        let has_z = parapet_set.contains(&Point2D::new(p.x, p.y - 1))
                 || parapet_set.contains(&Point2D::new(p.x, p.y + 1));
        has_x && has_z
    };

    // Place parapet blocks according to style
    use std::collections::HashMap;
    for &(point, roof_y) in &parapet_cells {
        // Accented styles render a clean solid band instead of the random
        // sandstone parapet styles: a full block on every parapet cell with a
        // raised cap at corners. All Block form, so an accent material with no
        // slab/wall variant (terracotta) never leaves gaps.
        if has_accent {
            parapet_placer
                .place_block(editor, point.add_y(roof_y - 1), BlockForm::Block, None, None)
                .await;
            if is_corner(point) {
                parapet_placer
                    .place_block(editor, point.add_y(roof_y), BlockForm::Block, None, None)
                    .await;
            }
            continue;
        }
        let checkerboard = (point.x + point.y) % 2 == 0;
        match parapet_style {
            ParapetStyle::Crenellated => {
                // Base block always, slab on top of alternating cells
                parapet_placer
                    .place_block(editor, point.add_y(roof_y - 1), BlockForm::Block, None, None)
                    .await;
                if checkerboard {
                    parapet_placer
                        .place_block(editor, point.add_y(roof_y), BlockForm::Slab, None, None)
                        .await;
                }
            }
            ParapetStyle::CornerPillars => {
                // Always 1 block; corners get a slab on top
                parapet_placer
                    .place_block(editor, point.add_y(roof_y - 1), BlockForm::Block, None, None)
                    .await;
                if is_corner(point) {
                    parapet_placer
                        .place_block(editor, point.add_y(roof_y), BlockForm::Slab, None, None)
                        .await;
                }
            }
            ParapetStyle::ThinWalls => {
                // Thin wall blocks; corners get full blocks for connection
                if is_corner(point) {
                    parapet_placer
                        .place_block(editor, point.add_y(roof_y - 1), BlockForm::Block, None, None)
                        .await;
                } else {
                    parapet_placer
                        .place_block(editor, point.add_y(roof_y - 1), BlockForm::Wall, None, None)
                        .await;
                }
            }
            ParapetStyle::SlabTopped => {
                // Full blocks at intervals, slabs between
                if checkerboard {
                    parapet_placer
                        .place_block(editor, point.add_y(roof_y - 1), BlockForm::Block, None, None)
                        .await;
                } else {
                    let slab_state = HashMap::from([("type".to_string(), "top".to_string())]);
                    parapet_placer
                        .place_block(editor, point.add_y(roof_y - 1), BlockForm::Slab, Some(&slab_state), None)
                        .await;
                }
            }
        }
    }

    // Return trivial heightmaps (height 0)
    let per_rect_heightmaps: Vec<RoofHeightmap> = rects.iter().map(|rect| {
        let min = rect.min();
        let max = rect.max();
        let width = (max.x - min.x + 1) as usize;
        let depth = (max.y - min.y + 1) as usize;
        RoofHeightmap::new(min.x, min.y, width, depth)
    }).collect();

    (Vec::new(), per_rect_heightmaps)
}

/// Place a ladder from the top floor up to the flat roof.
/// Picks a wall-adjacent cell that doesn't conflict with stairs.
/// Marks the ladder cell as UnblockedReachable on the top floor.
/// Returns the wall cell behind the ladder (if any) so callers can exclude
/// it from window placement.
pub async fn place_roof_ladder(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
    floor_plan: &super::super::floors::FloorPlan,
    room_plan: &mut super::super::rooms::RoomPlan,
) -> Option<(i32, i32)> {
    use super::super::rooms::CellState;
    let editor: &Editor = &*ctx.editor;
    let rects = top_floor_rects(frame);
    let rects = &rects[..];

    // Climb onto the tallest *flat* rect. Domed rects have no walkable terrace,
    // so they're excluded; if every top rect is a dome there's nowhere to go.
    let tallest_rect_idx = match (0..rects.len())
        .filter(|&i| !is_dome_eligible(&rects[i]))
        .max_by_key(|&i| frame.floor_counts()[i])
    {
        Some(i) => i,
        None => return None,
    };
    let tallest_rect = &rects[tallest_rect_idx];
    let roof_y = frame.roof_y(tallest_rect_idx);
    let top_floor = frame.floor_counts()[tallest_rect_idx] - 1;
    let top_floor_y = frame.floor_y(top_floor);

    // Cells to avoid: stair blocks on the top floor + stair air above
    let stair_cells = floor_plan.stair_cells_on_floor(top_floor);
    let stair_avoid: std::collections::HashSet<(i32, i32)> = stair_cells.iter().copied()
        .chain(floor_plan.stair_air_above.iter()
            .filter(|(f, _, _)| *f == top_floor)
            .map(|(_, x, z)| (*x, *z)))
        .chain(floor_plan.stair_tops.iter()
            .filter(|(f, _, _)| *f == top_floor)
            .map(|(_, x, z)| (*x, *z)))
        .chain(floor_plan.stair_bottoms.iter()
            .filter(|(f, _, _)| *f == top_floor)
            .map(|(_, x, z)| (*x, *z)))
        .collect();

    // Parapet cells: cells on the edge of this rect at this roof level.
    // Use the top-floor filled set (which accounts for jettied extents) rather
    // than the ground footprint, since the parapet sits at the top-floor level.
    let footprint_set: std::collections::HashSet<Point2D> =
        frame.filled_points_at_floor(top_floor).into_iter().collect();
    let parapet_set: std::collections::HashSet<Point2D> = tallest_rect.iter()
        .filter(|p| {
            [Point2D::new(p.x-1,p.y), Point2D::new(p.x+1,p.y),
             Point2D::new(p.x,p.y-1), Point2D::new(p.x,p.y+1),
             Point2D::new(p.x-1,p.y-1), Point2D::new(p.x+1,p.y-1),
             Point2D::new(p.x-1,p.y+1), Point2D::new(p.x+1,p.y+1)]
                .iter().any(|n| !footprint_set.contains(n))
        })
        .collect();

    // Candidates: interior cells adjacent to a parapet wall, not on stairs.
    // Ladder faces toward the wall (inward-facing, back against the wall).
    // Returns (ladder_pos, wall_pos, facing).
    let interior_set: std::collections::HashSet<Point2D> = tallest_rect.iter()
        .filter(|p| !parapet_set.contains(p))
        .collect();
    let mut candidates: Vec<(Point2D, Point2D, &str)> = interior_set.iter()
        .filter(|p| !stair_avoid.contains(&(p.x, p.y)))
        .filter_map(|&p| {
            if parapet_set.contains(&Point2D::new(p.x + 1, p.y)) { Some((p, Point2D::new(p.x + 1, p.y), "west")) }
            else if parapet_set.contains(&Point2D::new(p.x - 1, p.y)) { Some((p, Point2D::new(p.x - 1, p.y), "east")) }
            else if parapet_set.contains(&Point2D::new(p.x, p.y + 1)) { Some((p, Point2D::new(p.x, p.y + 1), "north")) }
            else if parapet_set.contains(&Point2D::new(p.x, p.y - 1)) { Some((p, Point2D::new(p.x, p.y - 1), "south")) }
            else { None }
        })
        .collect();

    // Prefer ladder positions away from the building's corners so the ladder
    // hugs the middle of an exterior wall instead of cutting in at a far edge.
    let corners = [
        tallest_rect.min(),
        Point2D::new(tallest_rect.max().x, tallest_rect.min().y),
        Point2D::new(tallest_rect.min().x, tallest_rect.max().y),
        tallest_rect.max(),
    ];
    candidates.sort_by_key(|(pos, _, _)| {
        let min_corner_dist = corners.iter()
            .map(|c| (pos.x - c.x).abs() + (pos.y - c.y).abs())
            .min()
            .unwrap_or(0);
        std::cmp::Reverse(min_corner_dist)
    });

    let (ladder_pos, wall_pos, facing) = if let Some(&(pos, wall, facing)) = candidates.first() {
        (pos, wall, facing)
    } else {
        return None;
    };

    // Place ladder from top floor up through the roof slab
    for y in top_floor_y..(roof_y - 1) {
        let mut ladder = Block::from_id("minecraft:ladder".into());
        ladder.state = Some(std::collections::HashMap::from([
            ("facing".to_string(), facing.to_string()),
        ]));
        editor.place_block_forced(&ladder, Point3D::new(ladder_pos.x, y, ladder_pos.y)).await;
    }

    // Mark ladder cell as UnblockedReachable on the top floor
    for room in &mut room_plan.rooms {
        if room.floor == top_floor && room.rect_index == tallest_rect_idx {
            if room.interior.contains(ladder_pos) {
                room.constraints.set((ladder_pos.x, ladder_pos.y), CellState::UnblockedReachable);
            }
        }
    }

    Some((wall_pos.x, wall_pos.y))
}
