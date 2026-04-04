use std::collections::{HashMap, HashSet};

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Cardinal, Point2D, Point3D, Rect2D};
use crate::minecraft::BlockForm;
use crate::noise::RNG;
use super::footprint::merge::{walk_edge_cells, concave_corner_cells};
use super::frame::Frame;
use super::walls::WallSegments;

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
}

impl FloorPlan {
    pub fn stairwells_on_floor(&self, floor: u32) -> Vec<&Stairwell> {
        self.stairwells.iter().filter(|s| s.floor == floor).collect()
    }
}

/// Compute the stairwell positions for a straight stair.
/// Position 0 is the corner landing, positions 1..=run are the stair steps.
fn stair_positions(start: Point2D, direction: Cardinal, run: i32) -> Vec<Point2D> {
    let sv: Point2D = direction.into();
    (0..=run).map(|i| start + sv * i).collect()
}

/// Generate corner candidates for a rect: each corner x 2 directions.
/// Start positions are 1 block inset from the rect edges (inside the walls).
fn corner_candidates(rect: &Rect2D) -> Vec<(Point2D, Cardinal)> {
    let min = rect.min();
    let max = rect.max();
    vec![
        (Point2D::new(min.x + 1, max.y - 1), Cardinal::East),
        (Point2D::new(min.x + 1, max.y - 1), Cardinal::North),
        (Point2D::new(min.x + 1, min.y + 1), Cardinal::East),
        (Point2D::new(min.x + 1, min.y + 1), Cardinal::South),
        (Point2D::new(max.x - 1, min.y + 1), Cardinal::West),
        (Point2D::new(max.x - 1, min.y + 1), Cardinal::South),
        (Point2D::new(max.x - 1, max.y - 1), Cardinal::West),
        (Point2D::new(max.x - 1, max.y - 1), Cardinal::North),
    ]
}

/// Check if all stair positions fit inside a rect with 1-block wall inset.
fn stair_fits_in_rect(start: Point2D, direction: Cardinal, run: i32, rect: &Rect2D) -> bool {
    let sv: Point2D = direction.into();
    let min = rect.min();
    let max = rect.max();
    for i in 0..=run {
        let p = start + sv * i;
        if !rect.contains(p) {
            return false;
        }
        if p.x <= min.x || p.x >= max.x || p.y <= min.y || p.y >= max.y {
            return false;
        }
    }
    true
}


/// U-stair positions: 2 steps toward the wall in `dir`, then 2 steps back
/// on the adjacent column. Anchor is the min corner of the 2x2.
fn spiral_positions(anchor: Point2D, dir: Cardinal) -> Vec<Point2D> {
    let (ax, az) = (anchor.x, anchor.y);
    match dir {
        Cardinal::North => vec![
            Point2D::new(ax, az + 1),
            Point2D::new(ax, az),
            Point2D::new(ax + 1, az),
            Point2D::new(ax + 1, az + 1),
        ],
        Cardinal::South => vec![
            Point2D::new(ax, az),
            Point2D::new(ax, az + 1),
            Point2D::new(ax + 1, az + 1),
            Point2D::new(ax + 1, az),
        ],
        Cardinal::East => vec![
            Point2D::new(ax, az),
            Point2D::new(ax + 1, az),
            Point2D::new(ax + 1, az + 1),
            Point2D::new(ax, az + 1),
        ],
        Cardinal::West => vec![
            Point2D::new(ax + 1, az),
            Point2D::new(ax, az),
            Point2D::new(ax, az + 1),
            Point2D::new(ax + 1, az + 1),
        ],
    }
}

/// Generate 2x2 U-stair candidates at each corner of a rect.
/// Returns (anchor, direction toward wall) pairs.
/// Requires at least a 4x4 rect (2x2 interior space with wall inset).
fn spiral_anchors(rect: &Rect2D) -> Vec<(Point2D, Cardinal)> {
    let min = rect.min();
    let max = rect.max();
    if max.x - min.x < 4 || max.y - min.y < 4 {
        return vec![];
    }
    vec![
        (Point2D::new(min.x + 1, min.y + 1), Cardinal::North),
        (Point2D::new(max.x - 2, min.y + 1), Cardinal::North),
        (Point2D::new(min.x + 1, max.y - 2), Cardinal::South),
        (Point2D::new(max.x - 2, max.y - 2), Cardinal::South),
    ]
}

/// Which two walls a spiral anchor is nearest to in its rect.
fn spiral_adjacent_walls(anchor: Point2D, rect: &Rect2D) -> [Cardinal; 2] {
    let min = rect.min();
    let max = rect.max();
    let x_wall = if anchor.x - min.x <= max.x - 1 - anchor.x {
        Cardinal::West
    } else {
        Cardinal::East
    };
    let z_wall = if anchor.y - min.y <= max.y - 1 - anchor.y {
        Cardinal::North
    } else {
        Cardinal::South
    };
    [x_wall, z_wall]
}

/// L-stair positions: 2 steps in primary direction, then 2 steps turning 90°.
fn l_stair_positions(start: Point2D, primary: Cardinal, turn: Cardinal) -> Vec<Point2D> {
    let pd: Point2D = primary.into();
    let td: Point2D = turn.into();
    let corner = start + pd;
    vec![
        start,
        corner,
        corner + td,
        corner + td * 2,
    ]
}

/// Generate L-stair candidates at each corner of a rect.
/// Each corner produces 2 candidates: 2 steps toward the corner, then 2 steps turning away.
fn l_stair_candidates(rect: &Rect2D) -> Vec<(Point2D, Cardinal, Cardinal)> {
    let min = rect.min();
    let max = rect.max();
    vec![
        // SW corner: walk West into corner, turn South away
        (Point2D::new(min.x + 2, min.y + 1), Cardinal::West, Cardinal::South),
        // SW corner: walk North into corner, turn East away
        (Point2D::new(min.x + 1, min.y + 2), Cardinal::North, Cardinal::East),
        // SE corner: walk East into corner, turn South away
        (Point2D::new(max.x - 2, min.y + 1), Cardinal::East, Cardinal::South),
        // SE corner: walk North into corner, turn West away
        (Point2D::new(max.x - 1, min.y + 2), Cardinal::North, Cardinal::West),
        // NW corner: walk West into corner, turn North away
        (Point2D::new(min.x + 2, max.y - 1), Cardinal::West, Cardinal::North),
        // NW corner: walk South into corner, turn East away
        (Point2D::new(min.x + 1, max.y - 2), Cardinal::South, Cardinal::East),
        // NE corner: walk East into corner, turn North away
        (Point2D::new(max.x - 2, max.y - 1), Cardinal::East, Cardinal::North),
        // NE corner: walk South into corner, turn West away
        (Point2D::new(max.x - 1, max.y - 2), Cardinal::South, Cardinal::West),
    ]
}

/// Check if all positions fit inside a rect with 1-block wall inset.
fn positions_fit_in_rect(positions: &[Point2D], rect: &Rect2D) -> bool {
    let min = rect.min();
    let max = rect.max();
    positions.iter().all(|p| p.x > min.x && p.x < max.x && p.y > min.y && p.y < max.y)
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

/// Pick a stair position for a specific floor transition.
/// Considers both straight and spiral candidates.
/// Tries all rects active on both floors, avoids occupied positions and door-facing walls.
/// Prefers exterior walls over interior walls.
fn pick_stair_for_floor(
    frame: &Frame,
    floor: u32,
    wall_segs: &WallSegments,
    occupied: &HashSet<(i32, i32)>,
    rng: &mut RNG,
) -> Option<(StairKind, Vec<Point2D>, Cardinal)> {
    let rects = frame.footprint().rects();
    let run = (frame.wall_height() + 1) as i32;

    // Only place stairs in the core rect (index 0) — wings are too small
    // and stairs there feel architecturally odd.
    let candidate_rects: Vec<usize> = if frame.active_rects(floor).contains(&0)
        && frame.active_rects(floor + 1).contains(&0)
    {
        vec![0]
    } else {
        vec![]
    };

    if candidate_rects.is_empty() {
        return None;
    }

    // Collect which wall facings have doors — never place stairs along those walls.
    let mut door_facings: HashSet<Cardinal> = HashSet::new();
    for seg in wall_segs.segments_on_floor(floor) {
        if seg.openings.iter().any(|o| matches!(o.kind, super::walls::OpeningKind::Door(_))) {
            door_facings.insert(seg.facing);
        }
    }

    // Determine which sides of the core rect have adjacent wing rects (interior walls).
    let mut interior_facings: HashSet<Cardinal> = HashSet::new();
    let core = &rects[0];
    for i in 1..rects.len() {
        if !frame.active_rects(floor).contains(&i) {
            continue;
        }
        let wing = &rects[i];
        if wing.min().x == core.max().x + 1 { interior_facings.insert(Cardinal::East); }
        if wing.max().x + 1 == core.min().x { interior_facings.insert(Cardinal::West); }
        if wing.min().y == core.max().y + 1 { interior_facings.insert(Cardinal::South); }
        if wing.max().y + 1 == core.min().y { interior_facings.insert(Cardinal::North); }
    }

    let mut exterior: Vec<(StairKind, Vec<Point2D>, Cardinal)> = Vec::new();
    let mut interior: Vec<(StairKind, Vec<Point2D>, Cardinal)> = Vec::new();

    for &rect_idx in &candidate_rects {
        let rect = &rects[rect_idx];
        let min = rect.min();

        // --- Straight stair candidates ---
        for (start, dir) in corner_candidates(rect) {
            if !stair_fits_in_rect(start, dir, run, rect) { continue; }
            let positions = stair_positions(start, dir, run);
            if positions.iter().any(|p| occupied.contains(&(p.x, p.y))) { continue; }
            let wall_facing = match dir {
                Cardinal::East | Cardinal::West => {
                    if start.y == min.y + 1 { Cardinal::North } else { Cardinal::South }
                }
                Cardinal::North | Cardinal::South => {
                    if start.x == min.x + 1 { Cardinal::West } else { Cardinal::East }
                }
            };
            if door_facings.contains(&wall_facing) { continue; }
            let candidate = (StairKind::Straight, positions, dir);
            if interior_facings.contains(&wall_facing) { interior.push(candidate); }
            else { exterior.push(candidate); }
        }

        // --- U-stair (spiral) candidates ---
        for (anchor, dir) in spiral_anchors(rect) {
            let positions = spiral_positions(anchor, dir);
            if positions.iter().any(|p| occupied.contains(&(p.x, p.y))) { continue; }
            let walls = spiral_adjacent_walls(anchor, rect);
            if walls.iter().all(|w| door_facings.contains(w)) { continue; }
            let candidate = (StairKind::Spiral, positions, dir);
            if walls.iter().any(|w| interior_facings.contains(w)) { interior.push(candidate); }
            else { exterior.push(candidate); }
        }

        // --- L-stair candidates ---
        for (start, primary, turn) in l_stair_candidates(rect) {
            let positions = l_stair_positions(start, primary, turn);
            if !positions_fit_in_rect(&positions, rect) {
                continue;
            }
            if positions.iter().any(|p| occupied.contains(&(p.x, p.y))) {
                continue;
            }
            let walls = spiral_adjacent_walls(start, rect);
            if walls.iter().any(|w| door_facings.contains(w)) {
                continue;
            }
            let candidate = (StairKind::LShaped, positions, primary);
            if walls.iter().any(|w| interior_facings.contains(w)) {
                interior.push(candidate);
            } else {
                exterior.push(candidate);
            }
        }
    }

    // Prefer exterior walls but fall back to interior
    let mut candidates = if !exterior.is_empty() { exterior } else { interior };
    if candidates.is_empty() {
        return None;
    }

    let idx = rng.rand_i32_range(0, candidates.len() as i32) as usize;
    Some(candidates.swap_remove(idx))
}

/// Pick a stair position for the attic (above the top regular floor).
/// Attic stairs are placed at a gable end (1 block in from the gable wall),
/// running along the eave wall (parallel to the ridge) toward the building center.
/// Only straight stairs are used since they fit naturally against the wall.
fn pick_attic_stair(
    frame: &Frame,
    occupied: &HashSet<(i32, i32)>,
    rng: &mut RNG,
) -> Option<(StairKind, Vec<Point2D>, Cardinal)> {
    let rects = frame.footprint().rects();
    let run = (frame.wall_height() + 1) as i32;
    let rect = &rects[0]; // core only

    // Ridge runs along the longer dimension. Attic stairs go perpendicular to
    // the ridge (along the eave walls), starting at a gable end corner.
    let eave_dirs: &[Cardinal] = if rect.length() >= rect.width() {
        &[Cardinal::North, Cardinal::South]
    } else {
        &[Cardinal::East, Cardinal::West]
    };

    let mut candidates: Vec<(StairKind, Vec<Point2D>, Cardinal)> = Vec::new();

    for (start, dir) in corner_candidates(rect) {
        if !eave_dirs.contains(&dir) { continue; }
        if !stair_fits_in_rect(start, dir, run, rect) { continue; }
        let positions = stair_positions(start, dir, run);
        if positions.iter().any(|p| occupied.contains(&(p.x, p.y))) { continue; }
        candidates.push((StairKind::Straight, positions, dir));
    }

    if candidates.is_empty() {
        return None;
    }

    let idx = rng.rand_i32_range(0, candidates.len() as i32) as usize;
    Some(candidates.swap_remove(idx))
}

/// Place floor slabs and stairs. Returns a FloorPlan with stairwell info.
/// When `has_attic` is true, an extra stairwell is placed from the top floor
/// into the attic space under a double-pitch roof.
pub async fn place_floors(
    editor: &Editor,
    frame: &Frame,
    wall_segs: &WallSegments,
    has_attic: bool,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
) -> FloorPlan {
    let mut stairwells = Vec::new();

    // For each floor transition, pick a stair position.
    // Track occupied positions so stairs don't overlap.
    let mut occupied: HashSet<(i32, i32)> = HashSet::new();

    for floor in 0..frame.max_floors().saturating_sub(1) {
        if let Some((kind, positions, direction)) = pick_stair_for_floor(
            frame, floor, wall_segs, &occupied, rng,
        ) {
            for pos in &positions {
                occupied.insert((pos.x, pos.y));
            }
            stairwells.push(Stairwell {
                positions,
                floor,
                direction,
                kind,
            });
        }
    }

    // Attic stairwell: from top regular floor up through the ceiling.
    // The ceiling at roof_y - 2 serves as the attic floor.
    if has_attic && frame.max_floors() >= 1 {
        let top_floor = frame.max_floors() - 1;
        if let Some((kind, positions, direction)) = pick_attic_stair(frame, &occupied, rng) {
            for pos in &positions {
                occupied.insert((pos.x, pos.y));
            }
            stairwells.push(Stairwell {
                positions,
                floor: top_floor,
                direction,
                kind,
            });
        }
    }

    // Collect stairwell openings per Y level for skipping floor slabs.
    // A stairwell on floor N needs an opening in the floor above (floor N+1).
    let mut openings: HashSet<(i32, i32, i32)> = HashSet::new();
    for sw in &stairwells {
        let upper_floor = sw.floor + 1;
        let slab_y = frame.floor_y(upper_floor) - 1;
        for pos in &sw.positions {
            openings.insert((pos.x, slab_y, pos.y));
        }
    }

    // Place floor slabs
    let floor_material_id = palette
        .get_material(MaterialRole::PrimaryWood)
        .expect("No primary wood material")
        .clone();

    let mut placer_rng = rng.derive();
    let mut placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng),
        floor_material_id.clone(),
    );

    // Floor blocks for all floors (skip stairwell openings and perimeter/exterior wall cells).
    // Ground floor planks overwrite the foundation stone for interior cells.
    for floor in frame.floors() {
        let perimeter = perimeter_cells(frame, floor);
        let y = frame.floor_y(floor) - 1;
        let points = frame.filled_points_at_floor(floor);

        for point in &points {
            if openings.contains(&(point.x, y, point.y)) {
                continue;
            }
            if perimeter.contains(&(point.x, point.y)) {
                continue;
            }
            placer.place_block_forced(
                editor,
                Point3D::new(point.x, y, point.y),
                BlockForm::Block,
                None,
                None,
            ).await;
        }
    }

    // Ceiling blocks at top of each rect (skip perimeter)
    let rects = frame.footprint().rects();
    let ground_perimeter = perimeter_cells(frame, 0);
    let mut placed: HashSet<(i32, i32, i32)> = HashSet::new();
    for i in 0..rects.len() {
        let y = frame.roof_y(i) - 2;
        let top_floor = frame.floor_counts()[i].saturating_sub(1);
        let ceil_perimeter = if top_floor == 0 { &ground_perimeter } else { &perimeter_cells(frame, top_floor) };
        for point in rects[i].iter() {
            if ceil_perimeter.contains(&(point.x, point.y)) {
                continue;
            }
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

    // Place stair blocks (same material as floors)
    let mut stair_rng = rng.derive();
    let mut stair_placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut stair_rng),
        floor_material_id.clone(),
    );

    for sw in &stairwells {
        let base_y = frame.floor_y(sw.floor);

        match sw.kind {
            StairKind::Straight => {
                let facing_str = sw.direction.to_string();
                let facing_away_str = (-sw.direction).to_string();

                let stair_state = HashMap::from([
                    ("facing".to_string(), facing_str.clone()),
                ]);
                let underside_state = HashMap::from([
                    ("facing".to_string(), facing_away_str.clone()),
                    ("half".to_string(), "top".to_string()),
                ]);

                // Position 0 is the corner landing, positions 1..=run are steps
                for (i, pos) in sw.positions.iter().enumerate() {
                    if i == 0 {
                        continue;
                    }
                    let step = (i - 1) as i32;
                    let y = base_y + step;

                    stair_placer.place_block_forced(
                        editor,
                        Point3D::new(pos.x, y, pos.y),
                        BlockForm::Stairs,
                        Some(&stair_state),
                        None,
                    ).await;

                    if step > 0 {
                        stair_placer.place_block(
                            editor,
                            Point3D::new(pos.x, y - 1, pos.y),
                            BlockForm::Stairs,
                            Some(&underside_state),
                            None,
                        ).await;
                    }

                    for clear_y in (y + 1)..=(y + 2) {
                        editor.place_block_forced(
                            &"air".into(),
                            Point3D::new(pos.x, clear_y, pos.y),
                        ).await;
                    }
                }
            }
            StairKind::Spiral => {
                // U-stair: steps 0,1 go toward wall, steps 2,3 come back
                let forward = sw.direction;
                let back = -sw.direction;

                for (i, pos) in sw.positions.iter().enumerate() {
                    let y = base_y + i as i32;
                    let facing = match i {
                        0 | 1 => forward,
                        2 => {
                            // Face away from the forward run
                            let toward = &sw.positions[1];
                            match (toward.x - pos.x, toward.y - pos.y) {
                                (1, 0) => Cardinal::West,
                                (-1, 0) => Cardinal::East,
                                (0, 1) => Cardinal::North,
                                (0, -1) => Cardinal::South,
                                _ => back,
                            }
                        }
                        _ => back,
                    };

                    let stair_state = HashMap::from([
                        ("facing".to_string(), facing.to_string()),
                    ]);

                    // Stair block
                    stair_placer.place_block_forced(
                        editor,
                        Point3D::new(pos.x, y, pos.y),
                        BlockForm::Stairs,
                        Some(&stair_state),
                        None,
                    ).await;

                    // Forward run: fill below with wood. Back run: upside-down stairs facing forward.
                    if i < 2 {
                        for fill_y in base_y..y {
                            stair_placer.place_block(
                                editor,
                                Point3D::new(pos.x, fill_y, pos.y),
                                BlockForm::Block,
                                None,
                                None,
                            ).await;
                        }
                    } else if y > base_y {
                        let underside_state = HashMap::from([
                            ("facing".to_string(), forward.to_string()),
                            ("half".to_string(), "top".to_string()),
                        ]);
                        stair_placer.place_block(
                            editor,
                            Point3D::new(pos.x, y - 1, pos.y),
                            BlockForm::Stairs,
                            Some(&underside_state),
                            None,
                        ).await;
                    }

                    // Clear air above for headroom
                    for clear_y in (y + 1)..=(y + 2) {
                        editor.place_block_forced(
                            &"air".into(),
                            Point3D::new(pos.x, clear_y, pos.y),
                        ).await;
                    }
                }
            }
            StairKind::LShaped => {
                // L-stair: steps 0,1 in primary direction, steps 2,3 turning away
                let primary = sw.direction;
                let turn_dir = match (sw.positions[2].x - sw.positions[1].x,
                                      sw.positions[2].y - sw.positions[1].y) {
                    (1, 0) => Cardinal::East,
                    (-1, 0) => Cardinal::West,
                    (0, 1) => Cardinal::South,
                    (0, -1) => Cardinal::North,
                    _ => primary,
                };

                for (i, pos) in sw.positions.iter().enumerate() {
                    let y = base_y + i as i32;
                    let facing = if i < 2 { primary } else { turn_dir };

                    let stair_state = HashMap::from([
                        ("facing".to_string(), facing.to_string()),
                    ]);

                    stair_placer.place_block_forced(
                        editor,
                        Point3D::new(pos.x, y, pos.y),
                        BlockForm::Stairs,
                        Some(&stair_state),
                        None,
                    ).await;

                    if i < 2 {
                        // First run: fill below with wood
                        for fill_y in base_y..y {
                            stair_placer.place_block(
                                editor,
                                Point3D::new(pos.x, fill_y, pos.y),
                                BlockForm::Block,
                                None,
                                None,
                            ).await;
                        }
                    } else if y > base_y {
                        // Second run: upside-down stairs facing opposite
                        let underside_state = HashMap::from([
                            ("facing".to_string(), (-turn_dir).to_string()),
                            ("half".to_string(), "top".to_string()),
                        ]);
                        stair_placer.place_block(
                            editor,
                            Point3D::new(pos.x, y - 1, pos.y),
                            BlockForm::Stairs,
                            Some(&underside_state),
                            None,
                        ).await;
                    }

                    // Clear air above for headroom
                    for clear_y in (y + 1)..=(y + 2) {
                        editor.place_block_forced(
                            &"air".into(),
                            Point3D::new(pos.x, clear_y, pos.y),
                        ).await;
                    }
                }
            }
        }
    }

    FloorPlan { stairwells }
}

/// Re-clear air above attic stair positions after the roof has been placed.
/// The roof overwrites the air that `place_floors` cleared, so this must run
/// after `place_roof` to carve headroom through the roof blocks.
pub async fn clear_attic_stair_headroom(
    editor: &Editor,
    frame: &Frame,
    floor_plan: &FloorPlan,
) {
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
