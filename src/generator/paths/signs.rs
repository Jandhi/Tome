//! Street signs at road intersections.
//!
//! Where two *different* named roads meet, stands a fingerpost on a nearby verge
//! corner: one standing sign per *distinct* road, stacked directly on top of one
//! another, each reading that road's name and a `<--` / `-->` / `<-->` arrow
//! pointing the way(s) the road runs. Run *after* the road network is
//! grouped into named roads (so paths carry a `road_id`) and *after* paving, but
//! *before* the open-space pass — the post cell is claimed as a path so
//! plazas/nooks/parks/yards treat it as road and never furnish over it.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use crate::editor::Editor;
use crate::generator::BuildClaim;
use crate::geometry::{get_surrounding_set, Point2D, Point3D};
use crate::minecraft::Block;

use super::path::{Path, PathType};

/// Minimum separation (squared XZ distance) between two posts, so a cluster of
/// close junctions (or a wide multi-cell crossing) yields one post, not a thicket.
const MIN_GAP_SQ: i32 = 100; // 10 blocks

/// How far out from an intersection's centroid to hunt for a verge corner.
const MAX_SEARCH_RADIUS: i32 = 5;

/// How far along each cardinal to look for the road that exit leads to.
const EXIT_SCAN: i32 = 8;

/// Cardinal directions probed for road exits: N, E, S, W (index 0..=3).
const DIR_OFFSET: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

/// Stand a fingerpost at every junction of two different roads.
///
/// `road_labels` is the geometric per-cell road numbering (one number = one
/// continuous physical road). Finds the cells whose 3×3 neighbourhood spans ≥2
/// distinct roads, clusters those into one intersection each, works out which
/// road each cardinal exit leads to, and stands a stacked-sign post on the
/// nearest valid verge corner. The post cell is claimed `Path(Pavement)` here so
/// the later open-space furnishing won't overwrite it. `paths` is used only to
/// keep posts off the paved footprint. Returns the cells where posts were stood.
pub async fn place_street_signs(
    editor: &mut Editor,
    paths: &[Path],
    road_labels: &HashMap<Point2D, u32>,
    road_names: &HashMap<u32, String>,
) -> Vec<Point2D> {
    let cell_road = road_labels;
    if cell_road.is_empty() {
        return Vec::new();
    }

    // Junction cells: a centreline cell whose 3×3 neighbourhood spans ≥2 roads.
    // Record the set of roads meeting there so we know which to look for per exit.
    let mut junctions: Vec<(Point2D, BTreeSet<u32>)> = Vec::new();
    for (&cell, &rid) in cell_road {
        let mut ids: BTreeSet<u32> = BTreeSet::new();
        ids.insert(rid);
        for dx in -1..=1 {
            for dz in -1..=1 {
                if let Some(&other) = cell_road.get(&Point2D::new(cell.x + dx, cell.y + dz)) {
                    ids.insert(other);
                }
            }
        }
        if ids.len() >= 2 {
            junctions.push((cell, ids));
        }
    }
    if junctions.is_empty() {
        return Vec::new();
    }

    let intersections = cluster(&junctions);
    let paved = paved_cells(paths);

    let mut placed: Vec<Point2D> = Vec::new();
    for (centroid, ids) in intersections {
        let exits = road_exits(centroid, cell_road, &ids);
        if exits.is_empty() {
            continue;
        }
        if let Some(cell) = pick_sign_cell(editor, centroid, &paved, &placed) {
            place_fingerpost(editor, cell, &exits, road_names).await;
            // Claim the footprint as a path so open spaces won't override it.
            editor
                .world_mut()
                .claim(cell, BuildClaim::Path(PathType::Pavement));
            placed.push(cell);
        }
    }
    placed
}

/// Which road each cardinal exit of the intersection leads to. Scans outward
/// along each cardinal (with ±1 perpendicular tolerance, since the centroid may
/// sit just off a centreline) and takes the first cell belonging to one of the
/// intersection's roads. Returns `(cardinal index, road_id)` per found exit; a
/// straight through-road yields two exits (both ways) sharing its number.
fn road_exits(
    centroid: Point2D,
    cell_road: &HashMap<Point2D, u32>,
    own_ids: &BTreeSet<u32>,
) -> Vec<(usize, u32)> {
    let mut exits = Vec::new();
    for (di, &(dx, dz)) in DIR_OFFSET.iter().enumerate() {
        let (px, pz) = (dz, dx); // perpendicular to the scan direction
        'scan: for r in 2..=EXIT_SCAN {
            for s in -1..=1 {
                let c = Point2D::new(centroid.x + dx * r + px * s, centroid.y + dz * r + pz * s);
                if let Some(&rid) = cell_road.get(&c) {
                    if own_ids.contains(&rid) {
                        exits.push((di, rid));
                        break 'scan;
                    }
                }
            }
        }
    }
    exits
}

/// Merge junction cells that touch (8-neighbourhood) into one intersection each,
/// returning the centroid and the union of road numbers per intersection.
fn cluster(junctions: &[(Point2D, BTreeSet<u32>)]) -> Vec<(Point2D, BTreeSet<u32>)> {
    let cells: HashSet<Point2D> = junctions.iter().map(|(c, _)| *c).collect();
    let ids_at: HashMap<Point2D, &BTreeSet<u32>> =
        junctions.iter().map(|(c, ids)| (*c, ids)).collect();

    let mut seen: HashSet<Point2D> = HashSet::new();
    let mut out: Vec<(Point2D, BTreeSet<u32>)> = Vec::new();
    for (start, _) in junctions {
        if !seen.insert(*start) {
            continue;
        }
        let mut comp: Vec<Point2D> = Vec::new();
        let mut queue: VecDeque<Point2D> = VecDeque::new();
        queue.push_back(*start);
        while let Some(c) = queue.pop_front() {
            comp.push(c);
            for dx in -1..=1 {
                for dz in -1..=1 {
                    let n = Point2D::new(c.x + dx, c.y + dz);
                    if cells.contains(&n) && seen.insert(n) {
                        queue.push_back(n);
                    }
                }
            }
        }
        let mut ids: BTreeSet<u32> = BTreeSet::new();
        let (mut sx, mut sz) = (0, 0);
        for c in &comp {
            sx += c.x;
            sz += c.y;
            if let Some(s) = ids_at.get(c) {
                ids.extend(s.iter().copied());
            }
        }
        let n = comp.len() as i32;
        out.push((Point2D::new(sx / n, sz / n), ids));
    }
    out
}

/// The widened paved footprint of `paths`, mirroring the shoulder pass in
/// `build_paths_merged` (same as the lights module) so signs stay off the road.
fn paved_cells(paths: &[Path]) -> HashSet<Point2D> {
    let mut paved: HashSet<Point2D> = HashSet::new();
    for path in paths {
        let centre: HashSet<Point2D> = path.points().iter().map(|p| p.drop_y()).collect();
        paved.extend(get_surrounding_set(&centre, path.width().saturating_sub(1)));
        paved.extend(centre);
    }
    paved
}

/// Find a verge corner near `centroid` for a post: search outward in rings,
/// trying the farthest (most diagonal) cells of each ring first so the post lands
/// on a corner rather than hard against the kerb. Skips paved, out of bounds,
/// water, already-claimed, or post-crowded cells.
fn pick_sign_cell(
    editor: &Editor,
    centroid: Point2D,
    paved: &HashSet<Point2D>,
    placed: &[Point2D],
) -> Option<Point2D> {
    let world = editor.world();
    for r in 1..=MAX_SEARCH_RADIUS {
        // Cells on the ring at Chebyshev distance `r`, corners first.
        let mut ring: Vec<Point2D> = Vec::new();
        for dx in -r..=r {
            for dz in -r..=r {
                if dx.abs().max(dz.abs()) == r {
                    ring.push(Point2D::new(centroid.x + dx, centroid.y + dz));
                }
            }
        }
        ring.sort_by_key(|c| {
            let (dx, dz) = ((c.x - centroid.x).abs(), (c.y - centroid.y).abs());
            std::cmp::Reverse(dx.min(dz))
        });
        for cell in ring {
            if !world.is_in_bounds_2d(cell) || paved.contains(&cell) || world.is_water(cell) {
                continue;
            }
            if !matches!(
                world.get_claim(cell),
                Some(BuildClaim::None | BuildClaim::Nature)
            ) {
                continue;
            }
            if placed.iter().all(|p| p.distance_squared(&cell) >= MIN_GAP_SQ) {
                return Some(cell);
            }
        }
    }
    None
}

/// Stand a fingerpost at `cell`: one standing sign per distinct road, stacked
/// directly on top of one another. A through-road that leaves the junction both
/// ways gets one `<-->` sign, not two — no road number ever repeats on the post.
/// Standing signs float in Java edition (no support needed), so the bottom sign
/// sits on the ground and the rest climb straight up from it — no fence post.
async fn place_fingerpost(
    editor: &Editor,
    cell: Point2D,
    exits: &[(usize, u32)],
    road_names: &HashMap<u32, String>,
) {
    let signs = signs_from_exits(exits);
    if signs.is_empty() {
        return;
    }
    let ground = editor.world().add_height(cell);

    // Signs stacked from the ground up, one block apart.
    for (i, &(rid, rotation, arrow)) in signs.iter().enumerate() {
        let sign_y = ground.y + i as i32;
        let name = road_names
            .get(&rid)
            .cloned()
            .unwrap_or_else(|| format!("Road {rid}"));
        editor
            .place_block_forced(
                &standing_sign_block(&name, rotation, arrow),
                Point3D::new(cell.x, sign_y, cell.y),
            )
            .await;
    }
}

/// Collapse the per-exit list into one entry per *distinct* road, each carrying
/// its sign rotation and direction arrow. Sorted by road id for determinism.
fn signs_from_exits(exits: &[(usize, u32)]) -> Vec<(u32, u8, &'static str)> {
    let mut dirs: BTreeMap<u32, BTreeSet<usize>> = BTreeMap::new();
    for &(di, rid) in exits {
        dirs.entry(rid).or_default().insert(di);
    }
    dirs.into_iter()
        .map(|(rid, ds)| {
            let (rot, arrow) = sign_facing(&ds);
            (rid, rot, arrow)
        })
        .collect()
}

/// Pick the standing-sign rotation and `<--` / `-->` / `<-->` arrow for a road
/// from the cardinal directions it leaves the junction by (0=N,1=E,2=S,3=W). The
/// sign faces *across* its road so the arrow on the front face points along the
/// real road on the ground. Rotations: 0 faces south, 12 faces east.
fn sign_facing(dirs: &BTreeSet<usize>) -> (u8, &'static str) {
    let (n, e, s, w) =
        (dirs.contains(&0), dirs.contains(&1), dirs.contains(&2), dirs.contains(&3));

    // Through-roads run both ways: arrow points both directions, sign set
    // perpendicular to the road. Facing south, left=west/right=east; facing
    // east, left=south/right=north.
    if e && w {
        return (0, "<-->");
    }
    if n && s {
        return (12, "<-->");
    }
    // A road that bends through the junction (two perpendicular exits) still
    // leaves two ways.
    if dirs.len() >= 2 {
        return (0, "<-->");
    }
    // Single exit: the arrow points the one way the road heads.
    match dirs.iter().next().copied().unwrap_or(2) {
        0 => (12, "-->"), // north is to the right when the sign faces east
        2 => (12, "<--"), // south is to the left
        1 => (0, "-->"),  // east is to the right when the sign faces south
        _ => (0, "<--"),  // west is to the left
    }
}

/// A standing `oak_sign` with the road `name` on the second line and a direction
/// arrow on the third (first and last lines blank, so the text sits centred).
/// Messages are plain SNBT strings — no surrounding double quotes, so nothing
/// prints a literal `"`. The back face is read from the opposite side, so its
/// arrow is mirrored (`-->` ⇄ `<--`) to keep pointing down the same road.
fn standing_sign_block(name: &str, rotation: u8, arrow: &str) -> Block {
    let back_arrow = match arrow {
        "-->" => "<--",
        "<--" => "-->",
        other => other, // "<-->" is symmetric
    };
    let face = |a: &str| {
        format!(
            "{{messages:[{},{},{},{}]}}",
            snbt_str(""),
            snbt_str(name),
            snbt_str(a),
            snbt_str(""),
        )
    };
    let data = format!("{{front_text:{},back_text:{}}}", face(arrow), face(back_arrow));
    let mut state = HashMap::new();
    state.insert("rotation".to_string(), rotation.to_string());
    Block::new("oak_sign".into(), Some(state), Some(data))
}

/// Wrap a string as a single-quoted SNBT literal, escaping `\` and `'` so names
/// with an apostrophe (e.g. "King's Road") don't break the sign data.
fn snbt_str(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{escaped}'")
}
