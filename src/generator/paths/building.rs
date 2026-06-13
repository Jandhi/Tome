use std::collections::{HashMap, HashSet};

use crate::{editor::Editor, generator::{BuildClaim, data::LoadedData, materials::{MaterialId, MaterialPlacer, Placer}, paths::path::{Path, PathPriority}}, geometry::{DOWN, Point2D, Point3D, UP, get_surrounding_set}, minecraft::BlockForm, noise::RNG, util::MeanExt};

/// A road must never carve *into* a finished building, a placed structure, or
/// the town wall. When a paved cell coincides with one of those claims we skip
/// it entirely — no air-clear (which would gouge the structure) and no surface
/// (which would overwrite it). The A* router already steers the centreline
/// around these; this guards the widened shoulders and endpoint-snap artefacts
/// that can still reach back onto a footprint.
/// Flip to true to float a marker above every road cell when paving:
/// red concrete = arterial, yellow = collector, white = alley/local, and
/// magenta = a cell the paver skipped because a claim (building/wall) blocks
/// it. Markers sit `DEBUG_MARKER_HEIGHT` above the road surface so the network
/// is readable from the air even where roofs cover the streets.
pub const DEBUG_ROAD_MARKERS: bool = true;
const DEBUG_MARKER_HEIGHT: i32 = 20;

/// How a paved cell must treat the claim already on it.
enum PaveMode {
    /// Free ground — clear headroom air, then lay the surface.
    Clear,
    /// Wall/gate tile — lay the surface across it, but DON'T clear air above
    /// (that would gouge a notch in the wall). Lets a road meet a gate flush.
    SurfaceOnly,
    /// A finished building or placed structure — never touch it.
    Skip,
}

fn pave_mode_for_claim(editor: &Editor, cell: Point2D) -> PaveMode {
    match editor.world().get_claim(cell) {
        Some(BuildClaim::Building(_) | BuildClaim::Structure(_)) => PaveMode::Skip,
        Some(BuildClaim::Wall) => PaveMode::SurfaceOnly,
        _ => PaveMode::Clear,
    }
}

pub async fn build_path(
    editor: &Editor,
    data : &LoadedData,
    path : &Path,
    rng : &mut RNG,
) {
    let mut points_2d = path.points()
        .iter()
        .map(|p| p.drop_y())
        .collect::<HashSet<_>>();

    for point in get_surrounding_set(&points_2d, path.width() - 1).iter().filter(|p| editor.world().is_in_bounds_2d(**p)) {
        points_2d.insert(*point);
    }

    let mut height_by_point = path.points()
        .iter()
        .map(|p| {
            (p.drop_y(), p.y as f32)
        })
        .collect::<HashMap<Point2D, f32>>();

    smooth_road_heights(&points_2d, &mut height_by_point);

    let mut placer = MaterialPlacer::new(
        Placer::new(&data.materials, rng),
        path.material().clone()
    );

    for point in points_2d.iter() {
        let height = height_by_point.get(point).cloned().expect("Height for point should be calculated");
        let int_height = height.floor() as i32;
        let point3d = Point3D {
            x: point.x,
            y: int_height,
            z: point.y,
        };

        let remainder = height - int_height as f32;

        // Force placement: the road surface often lands on a cell that already
        // holds terrain (e.g. the flatten's grass cap), and non-forced placement
        // skips equal-or-denser existing blocks — which silently drops the road,
        // so we force. Claim handling: skip buildings/structures entirely; on a
        // wall/gate tile lay the surface but don't clear the air above (no
        // gouging the wall); otherwise clear headroom and pave normally.
        match pave_mode_for_claim(editor, *point) {
            PaveMode::Skip => continue,
            PaveMode::Clear => {
                for i in 0..=3 {
                    editor.place_block_forced(&"air".into(), point3d + UP * i).await;
                }
            }
            PaveMode::SurfaceOnly => {}
        }

        if remainder > 0.3 {
            placer.place_block_forced(editor, point3d, BlockForm::Slab, None, None).await;
        }

        placer.place_block_forced(editor, point3d + DOWN, BlockForm::Block, None, None).await;
    }
}

/// Smooth `height_by_point` over `points_2d` in place: seed any cell still
/// missing a height from its neighbours (ring by ring, so the empty-`mean()`
/// trap can't collapse heights), then two averaging passes, a one-step bump
/// pass to nudge local minima/maxima, and a final averaging pass. The seeded
/// cells are the widened shoulders that start without a height.
fn smooth_road_heights(points_2d: &HashSet<Point2D>, height_by_point: &mut HashMap<Point2D, f32>) {
    // Seed the widened shoulder cells outward from the known heights before
    // smoothing. `neighbours()` is cardinal-only, so a cell two rings out
    // (width >= 3) can have no seeded neighbour on the first averaging pass;
    // `.mean()` of an empty set returns 0.0, which then propagates and
    // collapses the whole road's height. Filling ring by ring guarantees every
    // cell already has a neighbour height before we average.
    loop {
        let mut to_insert: Vec<(Point2D, f32)> = Vec::new();
        for point in points_2d.iter() {
            if height_by_point.contains_key(point) {
                continue;
            }
            let known: Vec<f32> = point.neighbours().iter()
                .filter_map(|n| height_by_point.get(n).copied())
                .collect();
            if !known.is_empty() {
                to_insert.push((*point, known.into_iter().mean()));
            }
        }
        if to_insert.is_empty() {
            break;
        }
        for (point, height) in to_insert {
            height_by_point.insert(point, height);
        }
    }

    for _ in 0..2 {
        for point in points_2d.iter() {
            height_by_point.insert(*point, [point.neighbours(), vec![*point]].concat().iter()
                .filter(|&neighbour| height_by_point.contains_key(neighbour))
                .map(|neighbour| height_by_point[neighbour])
                .mean());
        }
    }

    for point in points_2d.iter() {
        if point.neighbours().iter().all(|neighbour| !height_by_point.contains_key(neighbour) || height_by_point[neighbour] > height_by_point[&point]) {
            height_by_point.insert(*point, height_by_point[point] + 1.0);
            continue;
        }

        if point.neighbours().iter().all(|neighbour| !height_by_point.contains_key(neighbour) || height_by_point[neighbour] < height_by_point[&point]) {
            height_by_point.insert(*point, height_by_point[point] - 1.0);
            continue;
        }
    }

    for point in points_2d.iter() {
        height_by_point.insert(*point, [point.neighbours(), vec![*point]].concat().iter()
            .filter(|&neighbour| height_by_point.contains_key(neighbour))
            .map(|neighbour| height_by_point[neighbour])
            .mean());
    }
}

/// Build several routed paths as one melded surface. Heights are smoothed across
/// the *whole* network so junctions blend instead of stepping, and the surface
/// is laid in a single pass so a later road can never bury an earlier one. Where
/// paths overlap, the lower height and the higher-priority material win.
///
/// Returns the set of cells (as the slab block's exact `Point3D`) where a
/// half-step slab was laid — the half-block grade lips. Callers that align
/// buildings to the road (e.g. seating a front door's floor) use this to raise a
/// house a block over a fronting slab and to clear stray slab lips at doorways,
/// without having to read the placed road back out of the editor's block cache.
pub async fn build_paths_merged(
    editor: &Editor,
    data: &LoadedData,
    paths: &[Path],
    rng: &mut RNG,
) -> HashSet<Point3D> {
    let mut slab_cells: HashSet<Point3D> = HashSet::new();
    if paths.is_empty() {
        return slab_cells;
    }

    let rank = |p: PathPriority| match p {
        PathPriority::High => 2u8,
        PathPriority::Medium => 1,
        PathPriority::Low => 0,
    };

    // Centreline cells: height (lower wins on overlap) + the highest-priority
    // covering path's material.
    let mut height_by_point: HashMap<Point2D, f32> = HashMap::new();
    let mut material_by_point: HashMap<Point2D, MaterialId> = HashMap::new();
    let mut rank_by_point: HashMap<Point2D, u8> = HashMap::new();
    for path in paths {
        let r = rank(path.priority());
        for p in path.points() {
            let c = p.drop_y();
            height_by_point.entry(c).and_modify(|h| *h = h.min(p.y as f32)).or_insert(p.y as f32);
            if rank_by_point.get(&c).map_or(true, |&existing| r >= existing) {
                rank_by_point.insert(c, r);
                material_by_point.insert(c, path.material().clone());
            }
        }
    }

    // Widen each path by its own (width - 1) and fold the shoulders into the
    // paved set, inheriting the higher-priority material on overlap.
    let mut points_2d: HashSet<Point2D> = height_by_point.keys().copied().collect();
    for path in paths {
        let r = rank(path.priority());
        let centre: HashSet<Point2D> = path.points().iter().map(|p| p.drop_y()).collect();
        for cell in get_surrounding_set(&centre, path.width() - 1) {
            if !editor.world().is_in_bounds_2d(cell) {
                continue;
            }
            points_2d.insert(cell);
            if rank_by_point.get(&cell).map_or(true, |&existing| r > existing) {
                rank_by_point.insert(cell, r);
                material_by_point.insert(cell, path.material().clone());
            }
        }
    }

    smooth_road_heights(&points_2d, &mut height_by_point);

    let mut placer = Placer::new(&data.materials, rng);
    let fallback = paths[0].material().clone();

    for point in points_2d.iter() {
        let height = height_by_point.get(point).cloned().expect("Height for point should be calculated");
        let int_height = height.floor() as i32;
        let point3d = Point3D { x: point.x, y: int_height, z: point.y };
        let remainder = height - int_height as f32;
        let material = material_by_point.get(point).unwrap_or(&fallback);

        let mode = pave_mode_for_claim(editor, *point);

        if DEBUG_ROAD_MARKERS {
            let marker = if matches!(mode, PaveMode::Skip) {
                "magenta_concrete"
            } else {
                match rank_by_point.get(point) {
                    Some(2) => "red_concrete",
                    Some(1) => "yellow_concrete",
                    _ => "white_concrete",
                }
            };
            editor.place_block_forced(&marker.into(), point3d + UP * DEBUG_MARKER_HEIGHT).await;
        }

        // Claim handling: skip buildings/structures; on a wall/gate tile lay the
        // surface but don't clear air above (no gouging the wall); otherwise
        // clear headroom and pave normally.
        match mode {
            PaveMode::Skip => continue,
            PaveMode::Clear => {
                for i in 0..=3 {
                    editor.place_block_forced(&"air".into(), point3d + UP * i).await;
                }
            }
            PaveMode::SurfaceOnly => {}
        }

        if remainder > 0.3 {
            placer.place_block_forced(editor, point3d, material, BlockForm::Slab, None, None).await;
            slab_cells.insert(point3d);
        }

        placer.place_block_forced(editor, point3d + DOWN, material, BlockForm::Block, None, None).await;
    }

    slab_cells
}