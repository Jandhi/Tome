use std::collections::{HashMap, HashSet};
use crate::editor::Editor;
use crate::generator::materials::Placer;
use crate::generator::nbts::{place_structure, Structure, StructureType};
use crate::geometry::{Point2D, Point3D, is_straight_not_diagonal_point2d, Cardinal};
use crate::noise::RNG;
use crate::generator::BuildClaim;
use crate::generator::districts::WallType;
use log::info;

/// Wall points per desired gate. `N = max(1, ceil(loop_len / this))`.
pub const GATE_TARGET_SPACING: usize = 150;
/// Floor on the ring distance (index distance along the loop) between two selected gates.
pub const MIN_GATE_SPACING: usize = 100;
/// Length of the straight wall run a gate occupies.
pub const GATE_SIZE: i32 = 7;
/// Extra points inspected on each side of the gate run by the flatness scoring terms.
const FLATNESS_PAD: usize = 4;
/// Per-side cap on the straight-run scoring bonus.
const STRAIGHT_RUN_CAP: i32 = 16;

// Score weights — tunable. Start with equal weight on the two flatness terms and a
// half-weight straight-run bonus.
const WALL_TOP_FLATNESS_WEIGHT: f64 = 1.0;
const TERRAIN_FLATNESS_WEIGHT: f64 = 1.0;
const STRAIGHT_RUN_WEIGHT: f64 = 0.5;

#[derive(Debug, Clone, Copy)]
struct GateCandidate {
    index: usize,
    score: f64,
}

pub async fn build_wall_gate(
    wall_points: &Vec<Point3D>,
    editor: &mut Editor,
    _rng: &mut RNG,
    _material_placer: &mut Placer<'_>,
    is_thin: bool,
    is_palisade: bool,
    enhanced_wall_points: Option<&Vec<(Point3D, Vec<Cardinal>, WallType)>>,
    inner_wall_set: Option<&HashSet<Point3D>>,
    structures: & HashMap<StructureType, Structure>,
    gate_height: i32,
) {
    let palisade_gate = structures.get(&"basic_palisade_gate".into()).expect("Structure not found");
    let thin_gate = structures.get(&"basic_thin_gate".into()).expect("Structure not found");
    let wide_gate = structures.get(&"basic_wide_gate".into()).expect("Structure not found");

    let inner_wall_points = inner_wall_set
        .map(|set| set.iter().map(|p| p.drop_y()).collect::<HashSet<Point2D>>())
        .unwrap_or_default();

    // Wide gates (standard-with-inner) are the only ones that need the inner-wall clearance
    // check folded into candidacy.
    let is_wide = !is_thin && !is_palisade;

    let wall_type = if is_palisade {
        "palisade"
    } else if is_thin {
        "thin"
    } else {
        "wide"
    };
    let loop_len = wall_points.len();

    // Phase 1 — enumerate + score candidates.
    let mut candidates = gate_candidates(
        wall_points,
        editor,
        enhanced_wall_points,
        &inner_wall_points,
        is_wide,
    );

    // Phase 2 — select up to N with a spacing floor.
    let closed = loop_is_closed(wall_points);
    let target = ((loop_len as f64 / GATE_TARGET_SPACING as f64).ceil() as usize).max(1);
    let best_score = candidates
        .iter()
        .map(|c| c.score)
        .fold(f64::NEG_INFINITY, f64::max);
    let selected = select_gates(&mut candidates, loop_len, closed);

    info!(
        "[Gate] {} wall loop: len={} closed={} target={} candidates={} selected={} (best_score={:.1})",
        wall_type,
        loop_len,
        closed,
        target,
        candidates.len(),
        selected.len(),
        if candidates.is_empty() { 0.0 } else { best_score },
    );
    if selected.len() < target {
        info!(
            "[Gate] {} wall under-supplied: placed {} of {} targeted gates (only {} candidates passed the validity/water/spacing filters)",
            wall_type,
            selected.len(),
            target,
            candidates.len(),
        );
    }

    // Phase 3 — build (in loop order).
    for i in selected {
        if is_palisade {
            place_palisade_gate(wall_points, editor, i, palisade_gate, gate_height).await;
        } else if is_thin {
            place_thin_gate(wall_points, editor, i, thin_gate, gate_height).await;
        } else {
            let enhanced_points = enhanced_wall_points
                .expect("Enhanced wall points should be provided for this wall type");
            place_wide_gate(enhanced_points, editor, i, wide_gate, gate_height).await;
        }
    }
}

/// Phase 1: one pass over the ordered loop, keeping every index that passes the
/// validity floor (`is_gate_possible`, water rejection, and — for wide gates — the
/// inner-wall clearance check), scored by flatness and straight-run length.
fn gate_candidates(
    wall_points: &Vec<Point3D>,
    editor: &Editor,
    enhanced_wall_points: Option<&Vec<(Point3D, Vec<Cardinal>, WallType)>>,
    inner_wall_points: &HashSet<Point2D>,
    is_wide: bool,
) -> Vec<GateCandidate> {
    let mut candidates = Vec::new();
    let n = wall_points.len();

    for i in 0..n {
        // Validity floor — unchanged from the old behaviour.
        if !is_gate_possible(wall_points[i], wall_points, GATE_SIZE, i) {
            continue;
        }

        // Hard reject if any point of the gate run sits on / opens into water.
        let mut rejected = false;
        for a in i..i + GATE_SIZE as usize {
            if let Some(enh) = enhanced_wall_points {
                if enh[a].2 == WallType::Water || enh[a].2 == WallType::WaterWall {
                    rejected = true;
                    break;
                }
            }
            if editor.world().is_water(wall_points[a].drop_y()) {
                rejected = true;
                break;
            }
        }
        if rejected {
            continue;
        }

        // Wide gate: the inner-wall clearance check that used to live in the build
        // loop (and silently broke out without resetting the cooldown). Folding it
        // into candidacy means selection never wastes one of its N slots here.
        if is_wide {
            let enh = enhanced_wall_points
                .expect("Enhanced wall points should be provided for wide gates");
            // A wide gate needs a build direction; skip if none.
            let direction = match enh[i + 3].1.first().copied() {
                Some(dir) => dir,
                None => continue,
            };
            let mut blocked = false;
            for a in i..i + GATE_SIZE as usize {
                let inner_wall_point = enh[a].0.drop_y() + Point2D::from(direction) * 5;
                if inner_wall_points.contains(&inner_wall_point) {
                    blocked = true;
                    break;
                }
            }
            if blocked {
                continue;
            }
        }

        let score = score_candidate(wall_points, editor, i);
        candidates.push(GateCandidate { index: i, score });
    }

    candidates
}

/// Higher is better. A gate centred in a flat, long straight rampart over level
/// ground scores best; every defect subtracts.
fn score_candidate(wall_points: &Vec<Point3D>, editor: &Editor, i: usize) -> f64 {
    let n = wall_points.len();
    let lo = i.saturating_sub(FLATNESS_PAD);
    let hi = (i + GATE_SIZE as usize + FLATNESS_PAD).min(n - 1);

    let mut min_y = i32::MAX;
    let mut max_y = i32::MIN;
    let mut min_t = i32::MAX;
    let mut max_t = i32::MIN;
    for j in lo..=hi {
        let y = wall_points[j].y;
        min_y = min_y.min(y);
        max_y = max_y.max(y);
        let t = editor.world().get_height_at(wall_points[j].drop_y());
        min_t = min_t.min(t);
        max_t = max_t.max(t);
    }

    // Wall-top flatness: the ≤1 rule only checks the run's endpoints; this looks
    // wider and penalises bumpy surroundings.
    let wall_top_flatness = -((max_y - min_y) as f64);
    // Terrain flatness: the wall top can be smooth while the ground below slopes.
    let terrain_flatness = -((max_t - min_t) as f64);
    // Straight-run length: prefers gates centred in long flat sections.
    let straight_run = straight_run_extent(wall_points, i) as f64;

    WALL_TOP_FLATNESS_WEIGHT * wall_top_flatness
        + TERRAIN_FLATNESS_WEIGHT * terrain_flatness
        + STRAIGHT_RUN_WEIGHT * straight_run
}

/// How far the axis-aligned straight run extends on both sides of the gate run,
/// each side capped at `STRAIGHT_RUN_CAP`.
fn straight_run_extent(wall_points: &Vec<Point3D>, i: usize) -> i32 {
    let n = wall_points.len();
    let start = wall_points[i];
    let end = wall_points[i + GATE_SIZE as usize - 1];
    // If x is constant across the run, the gate runs along z; otherwise along x.
    let axis_is_z = start.x == end.x;

    let mut left = 0;
    let mut j = i;
    while j > 0 && left < STRAIGHT_RUN_CAP {
        let straight = if axis_is_z {
            wall_points[j - 1].x == wall_points[j].x
        } else {
            wall_points[j - 1].z == wall_points[j].z
        };
        if !straight {
            break;
        }
        left += 1;
        j -= 1;
    }

    let mut right = 0;
    let mut j = i + GATE_SIZE as usize - 1;
    while j + 1 < n && right < STRAIGHT_RUN_CAP {
        let straight = if axis_is_z {
            wall_points[j + 1].x == wall_points[j].x
        } else {
            wall_points[j + 1].z == wall_points[j].z
        };
        if !straight {
            break;
        }
        right += 1;
        j += 1;
    }

    left + right
}

/// Phase 2: greedy best-first selection of up to `N` candidates, enforcing a ring
/// distance floor of `MIN_GATE_SPACING` between selected gates. Returns selected
/// indices in loop order.
fn select_gates(candidates: &mut Vec<GateCandidate>, loop_len: usize, closed: bool) -> Vec<usize> {
    let n = ((loop_len as f64 / GATE_TARGET_SPACING as f64).ceil() as usize).max(1);

    // Sort by score desc; stable tie-break by ascending index.
    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.index.cmp(&b.index))
    });

    let mut selected: Vec<usize> = Vec::new();
    for c in candidates.iter() {
        if selected
            .iter()
            .all(|&s| ring_distance(c.index, s, loop_len, closed) >= MIN_GATE_SPACING)
        {
            selected.push(c.index);
            if selected.len() == n {
                break;
            }
        }
    }

    selected.sort_unstable();
    selected
}

/// Index distance along the loop. Circular when the loop is closed (the wall is a
/// ring), otherwise plain linear distance.
fn ring_distance(i: usize, j: usize, loop_len: usize, closed: bool) -> usize {
    let d = if i > j { i - j } else { j - i };
    if closed {
        d.min(loop_len.saturating_sub(d))
    } else {
        d
    }
}

/// Whether the ordered loop closes back on itself (first and last points adjacent).
/// `order_wall_points` can return an open run from its reversal fallback.
fn loop_is_closed(wall_points: &Vec<Point3D>) -> bool {
    if wall_points.len() < 2 {
        return false;
    }
    let first = wall_points[0];
    let last = wall_points[wall_points.len() - 1];
    (first.x - last.x).abs() <= 1 && (first.z - last.z).abs() <= 1
}

async fn place_palisade_gate(
    wall_points: &Vec<Point3D>,
    editor: &mut Editor,
    i: usize,
    palisade_gate: &Structure,
    gate_height: i32,
) {
    let air = "air".into();
    let point = wall_points[i];
    let middle_point = Point3D::new(
        wall_points[i + 2].x,
        editor.world().get_height_at(wall_points[i + 2].drop_y()),
        wall_points[i + 2].z,
    );
    let direction = if point.x == wall_points[i + 6].x {
        Cardinal::North
    } else {
        Cardinal::East
    };
    let neighbours: Vec<Point2D> = if direction == Cardinal::East {
        ((middle_point.x - 2)..=(middle_point.x + 2))
            .flat_map(|x| ((middle_point.z - 1)..=(middle_point.z + 1)).map(move |z| Point2D { x, y: z }))
            .collect()
    } else {
        ((middle_point.x - 1)..=(middle_point.x + 1))
            .flat_map(|x| ((middle_point.z - 2)..=(middle_point.z + 2)).map(move |z| Point2D { x, y: z }))
            .collect()
    };
    let height = middle_point.y;
    for neighbour in neighbours.iter() {
        editor.world_mut().claim(*neighbour, BuildClaim::Gate);
        for h in height..height + gate_height {
            editor.place_block_forced(&air, neighbour.add_y(h)).await;
        }
    }
    info!("Placing palisade gate at: {:?}", middle_point);
    place_structure(editor, None, palisade_gate, middle_point, direction, None, None, false, false)
        .await
        .expect("Failed to place gate");
    editor.world_mut().gate_locations.push((middle_point, direction));
}

async fn place_thin_gate(
    wall_points: &Vec<Point3D>,
    editor: &mut Editor,
    i: usize,
    thin_gate: &Structure,
    gate_height: i32,
) {
    let air = "air".into();
    let point = wall_points[i];
    let middle_point = Point3D::new(
        wall_points[i + 3].x,
        editor.world().get_height_at(wall_points[i + 3].drop_y()),
        wall_points[i + 3].z,
    );
    let direction = if point.x == wall_points[i + 6].x {
        Cardinal::North
    } else {
        Cardinal::East
    };
    let neighbours: Vec<Point2D> = if direction == Cardinal::North || direction == Cardinal::South {
        ((middle_point.x - 3)..=(middle_point.x + 3))
            .flat_map(|x| ((middle_point.z - 1)..=(middle_point.z + 1)).map(move |z| Point2D { x, y: z }))
            .collect()
    } else {
        ((middle_point.x - 1)..=(middle_point.x + 1))
            .flat_map(|x| ((middle_point.z - 3)..=(middle_point.z + 3)).map(move |z| Point2D { x, y: z }))
            .collect()
    };
    let height = middle_point.y;
    for neighbour in neighbours.iter() {
        editor.world_mut().claim(*neighbour, BuildClaim::Gate);
        for h in height..height + gate_height {
            editor.place_block_forced(&air, neighbour.add_y(h)).await;
        }
    }
    let mirror_x = direction == Cardinal::North || direction == Cardinal::South;
    info!("Placing thin gate at: {:?}", middle_point);
    place_structure(editor, None, thin_gate, middle_point, direction, None, None, mirror_x, false)
        .await
        .expect("Failed to place gate");
    editor.world_mut().gate_locations.push((middle_point, direction));
}

async fn place_wide_gate(
    enhanced_points: &Vec<(Point3D, Vec<Cardinal>, WallType)>,
    editor: &mut Editor,
    i: usize,
    wide_gate: &Structure,
    gate_height: i32,
) {
    let air = "air".into();
    // Candidacy guarantees a direction and inner-wall clearance.
    let direction = enhanced_points[i + 3].1[0];
    let middle_point = enhanced_points[i + 3].0.drop_y() + Point2D::from(direction) * 2;
    info!("Building gate at {:?}", middle_point);
    let neighbours: Vec<Point2D> = ((middle_point.x - 3)..=(middle_point.x + 3))
        .flat_map(|x| ((middle_point.y - 3)..=(middle_point.y + 3)).map(move |y| Point2D { x, y }))
        .collect();
    let height = editor.world().get_height_at(middle_point);
    for neighbour in neighbours.iter() {
        editor.world_mut().claim(*neighbour, BuildClaim::Gate);
        for h in height..height + gate_height {
            editor.place_block_forced(&air, neighbour.add_y(h)).await;
        }
    }
    let mirror_x = direction == Cardinal::North || direction == Cardinal::South;
    info!("Placing wide gate at: {:?}", middle_point);
    place_structure(
        editor,
        None,
        wide_gate,
        middle_point.add_y(height),
        direction.rotate_right(),
        None,
        None,
        mirror_x,
        false,
    )
    .await
    .expect("Failed to place gate");
    editor
        .world_mut()
        .gate_locations
        .push((middle_point.add_y(height), direction));
}

pub fn is_gate_possible(
    point: Point3D,
    wall_list: &Vec<Point3D>,
    gate_size: i32,
    index: usize,
) -> bool {
    // Check if the point is a valid gate position
    if index + gate_size as usize > wall_list.len() {
        return false; // Not enough points to form a gate, doesnt loop
    }
    // Check if the point is straight and not diagonal
    if is_straight_not_diagonal_point2d(
        Point2D { x: point.x, y: point.z },
        Point2D { x: wall_list[index + gate_size as usize - 1].x, y: wall_list[index + gate_size as usize - 1].z },
        gate_size - 1,
    ) && (point.y - wall_list[index + gate_size as usize - 1].y).abs() <= 1 {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::World;
    use crate::geometry::Rect3D;

    /// Build an offline editor over a flat synthetic world, then overwrite the
    /// heightmap from the supplied (x, terrain-height) profile laid along z=1.
    fn editor_with_terrain(terrain: &[i32]) -> Editor {
        let n = terrain.len() as i32;
        let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(n + 2, 320, 4));
        let world = World::synthetic(build_area, 64);
        let mut editor = world.get_offline_editor();

        let mut heights = HashSet::new();
        for (x, &h) in terrain.iter().enumerate() {
            heights.insert(Point3D::new(x as i32, h, 1));
        }
        editor.world_mut().set_heights(&heights);
        editor
    }

    /// Straight wall along x at z=1, with the given wall-top heights.
    fn wall_along_x(tops: &[i32]) -> Vec<Point3D> {
        tops.iter()
            .enumerate()
            .map(|(x, &y)| Point3D::new(x as i32, y, 1))
            .collect()
    }

    fn cand(index: usize, score: f64) -> GateCandidate {
        GateCandidate { index, score }
    }

    #[test]
    fn n_math_uses_ceil() {
        // Spread plenty of well-separated candidates (every 200) so the spacing
        // floor never limits the count; only the N target should.
        let make = |loop_len: usize| {
            let mut cands: Vec<GateCandidate> = (0..loop_len / 200)
                .map(|k| cand(k * 200, (loop_len - k) as f64))
                .collect();
            select_gates(&mut cands, loop_len, true).len()
        };
        assert_eq!(make(300), 1); // ceil(300/1000) = 1
        assert_eq!(make(1000), 1); // ceil(1000/1000) = 1
        assert_eq!(make(2500), 3); // ceil(2500/1000) = 3
        assert_eq!(make(2001), 3); // ceil(2001/1000) = 3
    }

    #[test]
    fn spacing_floor_excludes_neighbours() {
        // Two top candidates 40 apart -> only one survives the 150 floor.
        let mut close = vec![cand(100, 10.0), cand(140, 9.0)];
        let sel = select_gates(&mut close, 5000, true);
        assert_eq!(sel.len(), 1);
        assert_eq!(sel[0], 100); // the higher-scoring one wins

        // 200 apart -> both fit.
        let mut far = vec![cand(100, 10.0), cand(300, 9.0)];
        let sel = select_gates(&mut far, 5000, true);
        assert_eq!(sel, vec![100, 300]);
    }

    #[test]
    fn ring_distance_wraps_on_closed_loops() {
        // Candidates near index 0 and near the end are close around the ring.
        let mut cands = vec![cand(5, 10.0), cand(95, 9.0)];
        let sel = select_gates(&mut cands, 100, true);
        assert_eq!(sel.len(), 1, "wrap distance 10 < {MIN_GATE_SPACING}, only one gate");

        // On an open loop the same indices are far apart (distance 90).
        let mut cands = vec![cand(5, 10.0), cand(95, 9.0)];
        let sel = select_gates(&mut cands, 100, false);
        assert_eq!(sel.len(), 1, "open-loop distance 90 still < 150");

        // Open loop, indices far enough apart and a loop long enough for N>=2 -> both fit.
        let mut cands = vec![cand(5, 10.0), cand(200, 9.0)];
        let sel = select_gates(&mut cands, 2000, false);
        assert_eq!(sel, vec![5, 200]);
    }

    #[test]
    fn flat_beats_bumpy() {
        // Section A (x 0..30): flat wall top. Section B (x 30..60): wall top
        // alternates by 1 — still gate-valid (endpoints equal) but bumpier.
        let mut tops = vec![74; 60];
        for x in 30..60 {
            if x % 2 == 1 {
                tops[x] = 75;
            }
        }
        let editor = editor_with_terrain(&[64; 60]); // flat ground everywhere
        let wall = wall_along_x(&tops);

        let cands = gate_candidates(&wall, &editor, None, &HashSet::new(), false);
        assert!(!cands.is_empty());
        let best = cands
            .iter()
            .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap())
            .unwrap();
        // The best candidate must sit fully inside the flat section's flatness window.
        assert!(
            best.index + GATE_SIZE as usize + FLATNESS_PAD <= 30,
            "best candidate {} fell outside the flat section",
            best.index
        );
    }

    #[test]
    fn barely_valid_candidates_still_selected() {
        // Wall top steps by exactly 1 at every other point: each gate run has
        // endpoints differing by <= 1, so it is barely valid but never flat.
        let tops: Vec<i32> = (0..40).map(|x| 64 + (x % 2)).collect();
        let editor = editor_with_terrain(&[64; 40]);
        let wall = wall_along_x(&tops);

        let mut cands = gate_candidates(&wall, &editor, None, &HashSet::new(), false);
        assert!(!cands.is_empty(), "barely-valid spots must still be candidates");
        let sel = select_gates(&mut cands, wall.len(), false);
        assert!(!sel.is_empty(), "a gate must still be placed when only weak spots exist");
    }

    #[test]
    fn water_run_is_rejected() {
        let editor = editor_with_terrain(&[64; 20]);
        let wall = wall_along_x(&[74; 20]);

        // Mark a stretch as WaterWall via enhanced points; runs covering it are rejected.
        let enhanced: Vec<(Point3D, Vec<Cardinal>, WallType)> = wall
            .iter()
            .enumerate()
            .map(|(x, &p)| {
                let wt = if (8..12).contains(&x) {
                    WallType::WaterWall
                } else {
                    WallType::Standard
                };
                (p, vec![Cardinal::East], wt)
            })
            .collect();

        let cands = gate_candidates(&wall, &editor, Some(&enhanced), &HashSet::new(), false);
        // No candidate's 7-run may overlap indices 8..12.
        for c in &cands {
            let overlaps = (c.index..c.index + GATE_SIZE as usize).any(|a| (8..12).contains(&a));
            assert!(!overlaps, "candidate {} overlaps the water stretch", c.index);
        }
    }
}
