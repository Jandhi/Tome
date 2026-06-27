//! Street lighting for the road network.
//!
//! Posts a fence-and-lantern lamp on the verge beside every road — arterials,
//! collectors, and alleys — spaced evenly by arc length and staggered side to
//! side. Run *after* [`build_paths_merged`](super::build_paths_merged) so the
//! pavement is already down and we only have to step just off its edge.

use std::collections::{HashMap, HashSet};

use crate::editor::Editor;
use crate::generator::BuildClaim;
use crate::geometry::{get_surrounding_set, Point2D, Point3D, DOWN, UP};
use crate::minecraft::Block;

use super::path::Path;

/// Arc-length distance between consecutive lamps along a road. Lamps alternate
/// sides each interval, so a single side sees a lamp roughly every `2 * SPACING`.
const SPACING: f64 = 10.0;

/// Minimum separation (squared XZ distance) enforced between any two lamps
/// across every road. A candidate closer than this to an already-placed lamp is
/// skipped — kills clustering where paths overlap at junctions *and* where
/// separate roads run close to one another.
const MIN_GAP_SQ: i32 = 64; // 8 blocks

/// Fence blocks in the vertical post (heights `ground..ground + POST_FENCES`).
/// An arm fence sits on top reaching toward the road, with the lantern hung
/// beneath it.
const POST_FENCES: i32 = 5;

/// Place street lights beside every road in `paths`.
///
/// Walks each centreline by arc length, drops an anchor every [`SPACING`]
/// blocks, steps perpendicular off the pavement onto the verge (alternating
/// side each anchor), and stands a fence-post lamp there. Candidates that
/// land on pavement, water, out of bounds, on a claimed cell, or too close to
/// an existing lamp are skipped (the opposite side is tried first).
///
/// `lantern` is the lamp block chosen city-wide by the caller (e.g.
/// `minecraft:lantern`, or `minecraft:soul_lantern` for a cold settlement); it
/// is hung beneath the arm, so any state on it is replaced with `hanging=true`.
///
/// Returns the verge cells where lamps were stood (their base, at ground). The
/// caller can claim them; placement here mirrors [`build_paths_merged`](super::build_paths_merged),
/// which also leaves claiming to the caller.
pub async fn place_street_lights(editor: &Editor, paths: &[Path], lantern: &Block) -> Vec<Point2D> {
    let anchors = verge_anchors(editor, paths, SPACING, MIN_GAP_SQ);
    let mut placed: Vec<Point2D> = Vec::with_capacity(anchors.len());
    for (cell, toward_road) in anchors {
        place_lamp(editor, cell, toward_road, lantern).await;
        placed.push(cell);
    }
    placed
}

/// Walk every road centreline by arc length and return a verge anchor every
/// `spacing` blocks: the verge cell, paired with a unit step from it back
/// toward the road. Anchors alternate sides each interval and fall back to the
/// other side; candidates on pavement, water, out of bounds, on a claimed cell,
/// or within `sqrt(min_gap_sq)` of an earlier anchor are skipped.
///
/// Shared by [`place_street_lights`] and the stone-lantern placer — both want
/// "evenly spaced spots just off the kerb", differing only in spacing and what
/// they stand there. Returns spots in walk order.
pub(super) fn verge_anchors(
    editor: &Editor,
    paths: &[Path],
    spacing: f64,
    min_gap_sq: i32,
) -> Vec<(Point2D, Point2D)> {
    // Light every road tier — arterials, collectors, and alleys.
    let lit: Vec<&Path> = paths.iter().collect();
    if lit.is_empty() {
        return Vec::new();
    }

    // The full paved set across the lit roads, so a candidate can be tested for
    // "is this still road?" — built exactly like the paving widen pass.
    let paved = paved_cells(&lit);

    let mut placed: Vec<Point2D> = Vec::new();
    let mut anchors: Vec<(Point2D, Point2D)> = Vec::new();
    let mut anchor_index: u32 = 0;

    for path in &lit {
        let pts = path.points();
        if pts.len() < 2 {
            continue;
        }
        // Offset that clears the pavement: the widen pass reaches `width - 1`
        // cells out from the centreline (4-connected), so `width` is one past.
        let offset = path.width().max(1) as i32;

        let mut dist_acc = 0.0_f64;
        for i in 1..pts.len() {
            let prev = pts[i - 1].drop_y();
            let curr = pts[i].drop_y();
            dist_acc += prev.distance(&curr);

            while dist_acc >= spacing {
                dist_acc -= spacing;

                // Perpendicular to travel, reduced to a clean cardinal so the
                // spot offsets squarely off the road rather than diagonally.
                let delta = curr - prev;
                let perp = if delta.x.abs() >= delta.y.abs() {
                    Point2D::new(0, 1)
                } else {
                    Point2D::new(1, 0)
                };

                // Alternate which side we try first; fall back to the other.
                let primary = if anchor_index % 2 == 0 { 1 } else { -1 };
                anchor_index += 1;

                for side in [primary, -primary] {
                    let cell = curr + perp * (offset * side);
                    if is_valid_spot(editor, cell, &paved, &placed, min_gap_sq) {
                        // Step from the verge back toward the road.
                        let toward_road = perp * (-side);
                        anchors.push((cell, toward_road));
                        placed.push(cell);
                        break;
                    }
                }
            }
        }
    }

    anchors
}

/// The widened paved footprint of `paths`, mirroring the shoulder pass in
/// [`build_paths_merged`](super::build_paths_merged).
fn paved_cells(paths: &[&Path]) -> HashSet<Point2D> {
    let mut paved: HashSet<Point2D> = HashSet::new();
    for path in paths {
        let centre: HashSet<Point2D> = path.points().iter().map(|p| p.drop_y()).collect();
        paved.extend(get_surrounding_set(&centre, path.width().saturating_sub(1)));
        paved.extend(centre);
    }
    paved
}

/// A verge cell is a good lamp spot if it is in bounds, off the pavement, on
/// unclaimed natural ground (not water), and not crowding another lamp.
fn is_valid_spot(
    editor: &Editor,
    cell: Point2D,
    paved: &HashSet<Point2D>,
    placed: &[Point2D],
    min_gap_sq: i32,
) -> bool {
    let world = editor.world();
    if !world.is_in_bounds_2d(cell) || paved.contains(&cell) {
        return false;
    }
    // Only stand a lamp on open ground — never on a building, structure, wall,
    // gate, or another path's claim.
    if !matches!(
        world.get_claim(cell),
        Some(BuildClaim::None | BuildClaim::Nature)
    ) {
        return false;
    }
    if world.is_water(cell) {
        return false;
    }
    placed.iter().all(|p| p.distance_squared(&cell) >= min_gap_sq)
}

/// Stand a lamp on the verge: a fence post, an arm fence reaching one block
/// toward the road at the post's top, and `lantern` hung beneath that arm so the
/// light leans out over the street. `toward_road` is a unit step from the verge
/// cell back toward the centreline.
async fn place_lamp(editor: &Editor, cell: Point2D, toward_road: Point2D, lantern: &Block) {
    let Some(ground) = editor.world().add_height(cell) else { return; };
    let fence: Block = "minecraft:oak_fence".into();

    // Vertical post.
    for h in 0..POST_FENCES {
        editor.place_block_forced(&fence, ground + UP * h).await;
    }

    // Arm fence one block toward the road, level with the post's top.
    let top = ground.y + POST_FENCES - 1;
    let arm = Point3D::new(cell.x + toward_road.x, top, cell.y + toward_road.y);
    editor.place_block_forced(&fence, arm).await;

    // Lantern hung beneath the arm — force `hanging=true` regardless of the
    // caller's block state.
    let mut state = HashMap::new();
    state.insert("hanging".to_string(), "true".to_string());
    let hung = Block::new(lantern.id.clone(), Some(state), None);
    editor.place_block_forced(&hung, arm + DOWN).await;
}
