//! Torii (鳥居) gates straddling the rural roads — the threshold where a road
//! leaves the town wall into the countryside.
//!
//! One torii per gate, set a short way *out* along the rural road (not flush
//! against the wall) so it frames the countryside approach. It's built across
//! the road from the road's own direction — two vermilion uprights either side
//! of the carriageway, a black `kasagi` lintel sweeping up at both ends — so a
//! traveller walks straight through it heading in or out. Japanese-only; a
//! no-op for every other culture, so the caller can ring every settlement
//! unconditionally.

use std::collections::{HashMap, HashSet};

use crate::editor::Editor;
use crate::generator::buildings_v2::Culture;
use crate::geometry::{Cardinal, Point2D, Point3D};
use crate::minecraft::Block;

use super::path::Path;

/// How far along the rural road — as a fraction of its length, measured out from
/// the gate — the torii stands. Deep toward the countryside end so it reads as a
/// gateway out in the fields, not a gate-hugger, while still leaving road beyond
/// it. Clamped to keep the torii (and its tangent neighbours) on the route.
const TORII_ROAD_FRACTION: f32 = 0.66;

/// Half the gap between the two uprights: posts sit at ±this on the cross-road
/// axis, leaving a `2*OFFSET - 1` cell opening for the carriageway to pass.
const POST_OFFSET: i32 = 2;
/// Upright height in blocks above the road surface (lintel rests on top).
const POST_HEIGHT: i32 = 5;

/// Place a torii over each rural road, a short way out from its gate. `paths`
/// are the routed rural roads (each runs building→gate); the gate end of each is
/// matched to a town gate so every gate gets at most one torii. Returns how many
/// landed. Japanese-only — a no-op otherwise.
pub async fn place_rural_torii(editor: &Editor, paths: &[Path], culture: Culture) -> usize {
    if culture != Culture::Japanese {
        return 0;
    }
    let gates: Vec<Point2D> = editor
        .world()
        .gate_locations
        .iter()
        .map(|(g, _)| g.drop_y())
        .collect();
    if gates.is_empty() {
        return 0;
    }

    let mut placed = 0usize;
    let mut used_gates: HashSet<Point2D> = HashSet::new();

    for path in paths {
        let pts = path.points();
        if pts.len() < 4 {
            continue;
        }

        // The route runs building→gate, but don't rely on the order: the gate end
        // is whichever endpoint sits nearest a town gate.
        let first = pts[0].drop_y();
        let last = pts[pts.len() - 1].drop_y();
        let gate_at_start = nearest_dist(first, &gates) <= nearest_dist(last, &gates);
        let gate_cell = nearest_cell(if gate_at_start { first } else { last }, &gates);

        // One torii per gate (several rural roads can merge onto the same gate).
        if !used_gates.insert(gate_cell) {
            continue;
        }

        // Step TORII_ROAD_FRACTION of the way out from the gate end, along the
        // road, kept off both endpoints so the tangent neighbours below exist.
        let n = pts.len();
        let steps = (((n - 1) as f32) * TORII_ROAD_FRACTION).round() as usize;
        let from_gate = steps.clamp(1, n - 2);
        let idx = if gate_at_start { from_gate } else { n - 1 - from_gate };
        let center = pts[idx];

        // Road tangent at `idx`, pointing *away* from the gate (the travel
        // direction into the countryside) — this is what the torii faces along.
        let ahead = if gate_at_start { pts[idx + 1] } else { pts[idx - 1] };
        let behind = if gate_at_start { pts[idx - 1] } else { pts[idx + 1] };
        let tangent = ahead.drop_y() - behind.drop_y();
        let road_dir = dominant_cardinal(tangent);

        build_torii(editor, center, road_dir).await;
        placed += 1;
    }
    placed
}

/// Build one torii standing on `center` (a road cell at its surface height),
/// straddling the road that runs along `road_dir`. The uprights and tie-beam are
/// vermilion; the lintel is black with upturned stair tips at each end.
async fn build_torii(editor: &Editor, center: Point3D, road_dir: Cardinal) {
    // The gate spans *across* the road, so its long axis is perpendicular to the
    // travel direction. `perp` is the unit step along that cross-road axis.
    let perp = road_dir.rotate_right();
    let p: Point2D = perp.into();
    let c = center.drop_y();
    let base_y = center.y;

    // Cross-road offset `t`, vertical offset `y` → a world point.
    let at = |t: i32, y: i32| {
        let q = c + p * t;
        Point3D::new(q.x, y, q.y)
    };

    let red: Block = "minecraft:red_concrete".into();
    let black: Block = "minecraft:polished_blackstone".into();

    // Two uprights either side of the carriageway. Each runs from its OWN ground
    // (sampled at that post's cell — `add_height` gives where a base block sits)
    // up to the shared post-top, so a leg on lower ground simply grows taller and
    // the lintel stays level even when the two sides aren't at the same height.
    let post_top = base_y + POST_HEIGHT - 1;
    for side in [-POST_OFFSET, POST_OFFSET] {
        let foot = c + p * side;
        let Some(ground) = editor.world().add_height(foot) else { continue; };
        let ground_y = ground.y;
        for y in ground_y.min(post_top)..=post_top {
            editor.place_block_forced(&red, at(side, y)).await;
        }
    }

    // Nuki: the lower tie-beam joining the posts a block below their tops.
    let nuki_y = base_y + POST_HEIGHT - 2;
    for t in -POST_OFFSET..=POST_OFFSET {
        editor.place_block_forced(&red, at(t, nuki_y)).await;
    }

    // Gakuzuka: the short central strut between the tie-beam and the lintel.
    let kasagi_y = base_y + POST_HEIGHT;
    for y in (nuki_y + 1)..kasagi_y {
        editor.place_block_forced(&red, at(0, y)).await;
    }

    // Kasagi: the lintel, overhanging the posts by one cell on each end.
    let end = POST_OFFSET + 1;
    for t in -end..=end {
        editor.place_block_forced(&black, at(t, kasagi_y)).await;
    }

    // Upturned tips: a stair one block above each lintel end, its tall side
    // facing outward so the corner sweeps up and away — the torii's flared roof.
    editor
        .place_block_forced(&upturn_stair(perp.opposite()), at(end, kasagi_y + 1))
        .await;
    editor
        .place_block_forced(&upturn_stair(perp), at(-end, kasagi_y + 1))
        .await;
}

/// A bottom blackstone stair whose tall (full-height) side faces away from
/// `facing` — pass the inward cardinal so the high side points outward.
fn upturn_stair(facing: Cardinal) -> Block {
    Block::new(
        "minecraft:polished_blackstone_stairs".into(),
        Some(HashMap::from([
            ("facing".to_string(), facing.to_string()),
            ("half".to_string(), "bottom".to_string()),
        ])),
        None,
    )
}

/// Reduce a free vector to the cardinal of its dominant axis (X ties to E/W).
fn dominant_cardinal(d: Point2D) -> Cardinal {
    if d.x.abs() >= d.y.abs() {
        if d.x >= 0 { Cardinal::East } else { Cardinal::West }
    } else if d.y >= 0 {
        Cardinal::South
    } else {
        Cardinal::North
    }
}

/// The gate cell nearest `c`.
fn nearest_cell(c: Point2D, gates: &[Point2D]) -> Point2D {
    *gates
        .iter()
        .min_by_key(|g| g.distance_squared(&c))
        .expect("gates is non-empty")
}

/// Squared distance from `c` to its nearest gate.
fn nearest_dist(c: Point2D, gates: &[Point2D]) -> i32 {
    nearest_cell(c, gates).distance_squared(&c)
}
