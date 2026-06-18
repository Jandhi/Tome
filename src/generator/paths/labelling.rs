//! Geometric road labelling.
//!
//! Assigns a road number to each road *cell* based on the realised pavement,
//! not the abstract node graph. The node-graph stroke grouping (`name_roads`)
//! can't see where two A*-routed roads physically cross or merge — there's no
//! graph node there — so different roads end up overlapping on the map. This
//! pass works on the routed centrelines directly:
//!
//! 1. Densify every path into a 1-cell centreline set.
//! 2. Mark **junction** cells (8-neighbour degree ≥ 3) — every real crossing,
//!    T, and merge, including ones with no graph node.
//! 3. Split the non-junction cells into **segments** (connected chains).
//! 4. At each junction (cluster of junction cells = one blob): if exactly two
//!    roads meet it's an L/corner — merge them at any angle. If three or more
//!    (a T or crossing), only merge near-straight through-pairs, so a T's
//!    perpendicular stem keeps its own number.
//! 5. Drop strokes shorter than [`MIN_ROAD_CELLS`], densely renumber the rest,
//!    and return a `cell -> road number` map (junction cells included).

use std::collections::{HashMap, HashSet, VecDeque};

use crate::geometry::Point2D;

use super::path::Path;

/// Minimum total cells for a stroke to keep a number — short spurs stay
/// unlabelled (grey on the map, no sign), mirroring the old length cutoff.
const MIN_ROAD_CELLS: usize = 12;

/// Cells walked into a segment from a junction to get a stable heading for the
/// straightness pairing (a single adjacent cell is too noisy on diagonals).
const DIR_LOOKAHEAD: usize = 4;

/// How straight a through-pair must be to merge into one road. With headings
/// measured leaving the junction, `straightness = -dot(hA, hB)`: 1.0 is dead
/// straight (opposite headings), 0.0 a right-angle turn, negative a hairpin.
/// At a multi-way junction the straightest *available* pair is taken first, so a
/// 4-way crossing still resolves into two through-roads; this threshold only
/// governs the leftover pairs. -0.2 lets a corner up to ~100° (a right angle and
/// a bit past) read as one bending road — e.g. a side street that turns into the
/// road to a gate — while a near-hairpin still starts a new road. (Exactly-two-
/// road junctions are L-corners and always merge regardless, handled above.)
const STRAIGHTNESS_MIN: f64 = -0.2;

/// Eight-neighbour offsets.
const NB8: [(i32, i32); 8] = [
    (-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1),
];

/// Label road cells geometrically. See module docs. Returns centreline cell →
/// dense road number; unlabelled (too-short) cells are absent.
pub fn label_roads_geometric(paths: &[Path]) -> HashMap<Point2D, u32> {
    // 1. Densified centreline cell set (paths may step on a coarse lattice).
    let mut cells: HashSet<Point2D> = HashSet::new();
    for path in paths {
        let pts: Vec<Point2D> = path.points().iter().map(|p| p.drop_y()).collect();
        match pts.split_first() {
            None => {}
            Some((&first, rest)) => {
                cells.insert(first);
                let mut prev = first;
                for &p in rest {
                    for c in line_cells(prev, p) {
                        cells.insert(c);
                    }
                    prev = p;
                }
            }
        }
    }
    if cells.is_empty() {
        return HashMap::new();
    }

    let nbrs = |c: Point2D| -> Vec<Point2D> {
        NB8.iter()
            .map(|&(dx, dz)| Point2D::new(c.x + dx, c.y + dz))
            .filter(|n| cells.contains(n))
            .collect()
    };

    // 2. Junction cells: 8-degree ≥ 3.
    let junction: HashSet<Point2D> = cells
        .iter()
        .copied()
        .filter(|&c| nbrs(c).len() >= 3)
        .collect();

    // 3. Segments: connected components of (cells − junctions).
    let mut seg_of: HashMap<Point2D, usize> = HashMap::new();
    let mut segments: Vec<Vec<Point2D>> = Vec::new();
    for &start in &cells {
        if junction.contains(&start) || seg_of.contains_key(&start) {
            continue;
        }
        let id = segments.len();
        let mut comp = Vec::new();
        let mut q = VecDeque::new();
        q.push_back(start);
        seg_of.insert(start, id);
        while let Some(c) = q.pop_front() {
            comp.push(c);
            for n in nbrs(c) {
                if !junction.contains(&n) && !seg_of.contains_key(&n) {
                    seg_of.insert(n, id);
                    q.push_back(n);
                }
            }
        }
        segments.push(comp);
    }

    // 4a. Cluster junction cells into blobs, so a fat multi-road crossing counts
    // as ONE junction. (Pairing per single cell left collinear roads on opposite
    // sides of a wide junction unmerged — the cause of the over-fragmentation.)
    let mut blob_of: HashMap<Point2D, usize> = HashMap::new();
    let mut blobs: Vec<Vec<Point2D>> = Vec::new();
    for &jc in &junction {
        if blob_of.contains_key(&jc) {
            continue;
        }
        let id = blobs.len();
        let mut comp = Vec::new();
        let mut q = VecDeque::new();
        q.push_back(jc);
        blob_of.insert(jc, id);
        while let Some(c) = q.pop_front() {
            comp.push(c);
            for n in nbrs(c) {
                if junction.contains(&n) && !blob_of.contains_key(&n) {
                    blob_of.insert(n, id);
                    q.push_back(n);
                }
            }
        }
        blobs.push(comp);
    }

    // 4b. At each blob, pair the straightest-through segments into one road. A
    // segment touching two blobs links them, so a road chains across crossings.
    let mut parent: Vec<usize> = (0..segments.len()).collect();
    for blob in &blobs {
        let n = blob.len() as f64;
        let bc = (
            blob.iter().map(|c| c.x as f64).sum::<f64>() / n,
            blob.iter().map(|c| c.y as f64).sum::<f64>() / n,
        );
        // Incident segments (deduped) with their heading leaving the blob.
        let mut incident: Vec<(usize, (f64, f64))> = Vec::new();
        let mut seen: HashSet<usize> = HashSet::new();
        for &bcell in blob {
            for nb in nbrs(bcell) {
                if let Some(&sid) = seg_of.get(&nb) {
                    if seen.insert(sid) {
                        incident.push((sid, heading_from_blob(bc, nb, &seg_of, sid, &nbrs)));
                    }
                }
            }
        }
        if incident.len() == 2 {
            // Exactly two roads meet here and nothing else — an L/corner (or a
            // straight join), NOT a T. They must be the same road bending, so
            // merge regardless of the angle between them.
            union(&mut parent, incident[0].0, incident[1].0);
            continue;
        }

        // T (3) or crossing (4+): only continue a road through near-straight
        // pairs, so a T's perpendicular stem stays its own road. Greedily pair
        // the straightest opposite headings (through ≈ -1·-1 = 1).
        let mut cands: Vec<(f64, usize, usize)> = Vec::new();
        for a in 0..incident.len() {
            for b in (a + 1)..incident.len() {
                let (da, db) = (incident[a].1, incident[b].1);
                let straightness = -(da.0 * db.0 + da.1 * db.1);
                cands.push((straightness, incident[a].0, incident[b].0));
            }
        }
        cands.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut used: HashSet<usize> = HashSet::new();
        for (s, sa, sb) in cands {
            if s < STRAIGHTNESS_MIN {
                break;
            }
            if used.contains(&sa) || used.contains(&sb) {
                continue;
            }
            union(&mut parent, sa, sb);
            used.insert(sa);
            used.insert(sb);
        }
    }

    // 5. Stroke size, length cutoff, dense renumbering.
    let mut stroke_cells: HashMap<usize, usize> = HashMap::new();
    for (sid, seg) in segments.iter().enumerate() {
        *stroke_cells.entry(find(&mut parent, sid)).or_insert(0) += seg.len();
    }
    let mut remap: HashMap<usize, u32> = HashMap::new();
    let mut next = 0u32;
    for (&root, &count) in &stroke_cells {
        if count >= MIN_ROAD_CELLS {
            remap.insert(root, next);
            next += 1;
        }
    }

    // Cell -> road number for every segment cell of a surviving stroke.
    let mut labels: HashMap<Point2D, u32> = HashMap::new();
    for (sid, seg) in segments.iter().enumerate() {
        if let Some(&rid) = remap.get(&find(&mut parent, sid)) {
            for &c in seg {
                labels.insert(c, rid);
            }
        }
    }
    // Junction cells: adopt the most common road number among their segment
    // neighbours, so a through-road's colour carries across the crossing.
    for &j in &junction {
        let mut votes: HashMap<u32, usize> = HashMap::new();
        for n in nbrs(j) {
            if let Some(&rid) = labels.get(&n) {
                *votes.entry(rid).or_insert(0) += 1;
            }
        }
        if let Some((&rid, _)) = votes.iter().max_by_key(|(_, &c)| c) {
            labels.insert(j, rid);
        }
    }

    labels
}

/// Unit XZ heading a segment leaves a junction blob with: walk up to
/// [`DIR_LOOKAHEAD`] cells into the segment from `entry`, then take the direction
/// from the blob centroid `bc` to that cell (stable across diagonals).
fn heading_from_blob(
    bc: (f64, f64),
    entry: Point2D,
    seg_of: &HashMap<Point2D, usize>,
    sid: usize,
    nbrs: &impl Fn(Point2D) -> Vec<Point2D>,
) -> (f64, f64) {
    let mut cur = entry;
    let mut visited: HashSet<Point2D> = HashSet::new();
    visited.insert(cur);
    for _ in 0..DIR_LOOKAHEAD {
        let next = nbrs(cur)
            .into_iter()
            .find(|n| seg_of.get(n) == Some(&sid) && !visited.contains(n));
        match next {
            Some(n) => {
                visited.insert(n);
                cur = n;
            }
            None => break,
        }
    }
    let (dx, dz) = (cur.x as f64 - bc.0, cur.y as f64 - bc.1);
    let len = (dx * dx + dz * dz).sqrt().max(1e-9);
    (dx / len, dz / len)
}

/// Integer line rasterisation (Bresenham) between two cells, inclusive of `b`,
/// exclusive of `a` (the caller already inserted the previous point).
fn line_cells(a: Point2D, b: Point2D) -> Vec<Point2D> {
    let mut out = Vec::new();
    let (mut x, mut y) = (a.x, a.y);
    let (dx, dy) = ((b.x - x).abs(), (b.y - y).abs());
    let (sx, sy) = (if a.x < b.x { 1 } else { -1 }, if a.y < b.y { 1 } else { -1 });
    let mut err = dx - dy;
    loop {
        if (x, y) != (a.x, a.y) {
            out.push(Point2D::new(x, y));
        }
        if x == b.x && y == b.y {
            break;
        }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
    out
}

fn find(parent: &mut [usize], x: usize) -> usize {
    let mut r = x;
    while parent[r] != r {
        r = parent[r];
    }
    let mut cur = x;
    while parent[cur] != r {
        let next = parent[cur];
        parent[cur] = r;
        cur = next;
    }
    r
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent[ra] = rb;
    }
}
