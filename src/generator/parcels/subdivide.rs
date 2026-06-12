use std::collections::{HashMap, HashSet, VecDeque};

use crate::{
    geometry::{voronoi_fill_with_recenter, Point2D, CARDINALS_2D},
    noise::RNG,
};

/// Peel a frontage *ribbon* — the band of block cells within `depth` cardinal
/// steps of a main road — off a block before its interior is subdivided.
///
/// Subdividing a block directly fragments the edge that faces a main road into
/// short stubs, so the long arterial/collector frontage we actually want goes
/// unused. Reserving the ribbon first keeps that edge whole: the ribbon faces
/// the road along its full length and houses placed on it get long continuous
/// frontage chains (the whole point of the road hierarchy). The leftover
/// `interior` is what gets subdivided into back lots served by alleys.
///
/// Returns `(ribbon_lots, interior)`. `ribbon_lots` is the ribbon split
/// into connected components (each a buildable lot); `interior` is the block
/// minus the ribbon. If no block cell touches a `main_road` cell, the ribbon is
/// empty and the whole block comes back as `interior`.
///
/// `depth` is measured in cells: the cells touching the road are depth 1, and
/// the BFS stops once it has taken `depth` cells inward. Pick it to match the
/// deepest house the road tier should host so the deepest footprint still fits.
pub fn reserve_road_ribbon(
    block: &HashSet<Point2D>,
    main_roads: &HashSet<Point2D>,
    depth: i32,
) -> (Vec<HashSet<Point2D>>, HashSet<Point2D>) {
    // Multi-source BFS inward from every cell fronting a main road.
    let mut ribbon: HashSet<Point2D> = HashSet::new();
    let mut frontier: VecDeque<(Point2D, i32)> = VecDeque::new();
    for &cell in block {
        let fronts_road = CARDINALS_2D.iter().any(|&d| {
            let n = cell + d;
            !block.contains(&n) && main_roads.contains(&n)
        });
        if fronts_road && ribbon.insert(cell) {
            frontier.push_back((cell, 1));
        }
    }
    while let Some((cell, dist)) = frontier.pop_front() {
        if dist >= depth {
            continue;
        }
        for d in CARDINALS_2D {
            let n = cell + d;
            if block.contains(&n) && ribbon.insert(n) {
                frontier.push_back((n, dist + 1));
            }
        }
    }

    let interior: HashSet<Point2D> = block.difference(&ribbon).copied().collect();
    (connected_components(&ribbon), interior)
}

/// Carve connectors so the interior alley network reaches the main roads through
/// a reserved frontage [ribbon](reserve_road_ribbon). Without this the alleys
/// dead-end behind the ribbon, never touching the big roads.
///
/// From each alley cell that is the roadward *tip* of a perpendicular run (the
/// alley continues ≥2 cells away from the road), walk straight through the ribbon
/// in the road direction; if the walk reaches a `main_roads` cell, the ribbon
/// cells it crossed become a connector. The ≥2-cell back-check is what keeps a
/// parallel alley flanking the ribbon from carving its whole side into road.
///
/// Returns the union of connector cells (a subset of `ribbon`). Callers convert
/// these from frontage ribbon to alley.
pub fn carve_ribbon_connectors(
    ribbon: &HashSet<Point2D>,
    alleys: &HashSet<Point2D>,
    main_roads: &HashSet<Point2D>,
) -> HashSet<Point2D> {
    let mut connectors = HashSet::new();
    for &a in alleys {
        for dir in CARDINALS_2D {
            let out = a + dir;
            if !ribbon.contains(&out) {
                continue;
            }
            let nd = Point2D::new(-dir.x, -dir.y);
            if !alleys.contains(&(a + nd)) || !alleys.contains(&(a + nd + nd)) {
                continue;
            }
            let mut p = out;
            let mut seg = Vec::new();
            let mut reached = false;
            while ribbon.contains(&p) {
                seg.push(p);
                let next = p + dir;
                if main_roads.contains(&next) {
                    reached = true;
                    break;
                }
                p = next;
            }
            if reached {
                connectors.extend(seg);
            }
        }
    }
    connectors
}

/// Recursively subdivide a block of cells until every sub-block fits within
/// `max_dim` along both axes. Each cut lays a 1-cell alley on the split line
/// and recurses on the connected components of each side (so concave blocks
/// produced by voronoi partitioning fragment naturally).
///
/// Returns `(sub_blocks, alleys)`. Alleys are the cells consumed by new cut
/// lines; sub_blocks are the remaining buildable cells, partitioned.
pub fn subdivide_block(
    cells: &HashSet<Point2D>,
    rng: &mut RNG,
    max_dim: i32,
) -> (Vec<HashSet<Point2D>>, HashSet<Point2D>) {
    let mut alleys = HashSet::new();
    let mut sub_blocks = Vec::new();
    recurse(cells.clone(), rng, max_dim, &mut alleys, &mut sub_blocks);
    (sub_blocks, alleys)
}

fn recurse(
    cells: HashSet<Point2D>,
    rng: &mut RNG,
    max_dim: i32,
    alleys: &mut HashSet<Point2D>,
    out: &mut Vec<HashSet<Point2D>>,
) {
    if cells.is_empty() {
        return;
    }

    let (min_x, max_x, min_y, max_y) = bounds(&cells);
    let width = max_x - min_x + 1;
    let height = max_y - min_y + 1;

    if width <= max_dim && height <= max_dim {
        out.push(cells);
        return;
    }

    // Pick which axis to bisect. Prefer not to split an axis that's already
    // smaller than 2*max_dim (cutting it would just produce unnecessarily
    // small pieces). When both axes are >= 2*max_dim, pick randomly to vary
    // road orientation. If neither qualifies, cut the longer axis as a
    // fallback so we still make progress.
    let x_eligible = width >= 2 * max_dim;
    let y_eligible = height >= 2 * max_dim;
    let cut_along_x = if x_eligible && y_eligible {
        rng.rand_i32_range(0, 2) == 0
    } else if x_eligible {
        true
    } else if y_eligible {
        false
    } else {
        width >= height
    };
    let (axis_min, axis_max) = if cut_along_x { (min_x, max_x) } else { (min_y, max_y) };
    // Cut anywhere along the axis such that each side keeps at least roughly
    // half-a-block on that axis. Picking uniformly within this range (rather
    // than near the midpoint) yields highly varied sub-block sizes and avoids
    // a stacked-grid look. Margin is intentionally small so sub-pieces are
    // free to be lopsided. `cut` is the first of two alley rows.
    let margin = (max_dim / 4).max(2);
    let min_cut = axis_min + margin;
    let max_cut = axis_max - margin - 1; // reserve one extra row for the 2-wide alley
    let cut = if min_cut >= max_cut {
        (axis_min + axis_max) / 2
    } else {
        rng.rand_i32_range(min_cut, max_cut + 1)
    };

    let mut side_a = HashSet::new();
    let mut side_b = HashSet::new();
    for p in cells {
        let v = if cut_along_x { p.x } else { p.y };
        if v == cut || v == cut + 1 {
            alleys.insert(p);
        } else if v < cut {
            side_a.insert(p);
        } else {
            side_b.insert(p);
        }
    }

    for component in connected_components(&side_a) {
        recurse(component, rng, max_dim, alleys, out);
    }
    for component in connected_components(&side_b) {
        recurse(component, rng, max_dim, alleys, out);
    }
}

fn bounds(cells: &HashSet<Point2D>) -> (i32, i32, i32, i32) {
    let mut min_x = i32::MAX;
    let mut max_x = i32::MIN;
    let mut min_y = i32::MAX;
    let mut max_y = i32::MIN;
    for p in cells {
        if p.x < min_x { min_x = p.x; }
        if p.x > max_x { max_x = p.x; }
        if p.y < min_y { min_y = p.y; }
        if p.y > max_y { max_y = p.y; }
    }
    (min_x, max_x, min_y, max_y)
}

/// Voronoi-style partition of `cells` into roughly `sections` sub-blocks, with
/// boundary cells extracted as alleys. Mirrors the same `(sub_blocks, alleys)`
/// shape as `subdivide_block` so callers can swap strategies. A cell is an
/// alley if any cardinal neighbour belongs to a different section — gives
/// 2-wide alleys naturally (one cell from each side of every voronoi edge).
pub fn voronoi_subdivide_block(
    cells: &HashSet<Point2D>,
    rng: &mut RNG,
    sections: usize,
) -> (Vec<HashSet<Point2D>>, HashSet<Point2D>) {
    if cells.is_empty() || sections == 0 {
        return (Vec::new(), HashSet::new());
    }

    let raw_sections = voronoi_fill_with_recenter(
        cells,
        &|p: Point2D| CARDINALS_2D.iter().map(|d| p + *d).collect(),
        &|set: &HashSet<Point2D>| {
            let avg = set.iter().fold(Point2D::ZERO, |a, p| a + *p) / set.len() as i32;
            if set.contains(&avg) {
                avg
            } else {
                set.iter().min_by_key(|p| p.distance_manhattan(&avg)).copied().unwrap()
            }
        },
        rng,
        sections,
        3,
    );

    let mut cell_to_section: HashMap<Point2D, usize> = HashMap::new();
    for (idx, section) in raw_sections.iter().enumerate() {
        for p in section {
            cell_to_section.insert(*p, idx);
        }
    }

    let mut alleys = HashSet::new();
    for (&p, &my_idx) in &cell_to_section {
        for d in CARDINALS_2D {
            if let Some(&their_idx) = cell_to_section.get(&(p + d)) {
                if their_idx != my_idx {
                    alleys.insert(p);
                    break;
                }
            }
        }
    }

    let sub_blocks: Vec<HashSet<Point2D>> = raw_sections.into_iter()
        .map(|s| s.difference(&alleys).copied().collect())
        .filter(|s: &HashSet<Point2D>| !s.is_empty())
        .collect();

    (sub_blocks, alleys)
}

fn connected_components(cells: &HashSet<Point2D>) -> Vec<HashSet<Point2D>> {
    let mut remaining = cells.clone();
    let mut comps = Vec::new();
    while let Some(&seed) = remaining.iter().next() {
        let mut comp = HashSet::new();
        let mut stack = vec![seed];
        while let Some(p) = stack.pop() {
            if !remaining.remove(&p) {
                continue;
            }
            comp.insert(p);
            for d in CARDINALS_2D {
                let np = p + d;
                if remaining.contains(&np) {
                    stack.push(np);
                }
            }
        }
        comps.push(comp);
    }
    comps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_block(x0: i32, x1: i32, z0: i32, z1: i32) -> HashSet<Point2D> {
        (x0..=x1).flat_map(|x| (z0..=z1).map(move |z| Point2D::new(x, z))).collect()
    }

    #[test]
    fn ribbon_peels_depth_band_along_one_road() {
        // 10×10 block; a road runs along its north edge (z = 4).
        let block = rect_block(0, 9, 5, 14);
        let road: HashSet<Point2D> = (0..=9).map(|x| Point2D::new(x, 4)).collect();

        let (lots, interior) = reserve_road_ribbon(&block, &road, 3);

        // One contiguous ribbon lot, 3 rows deep (z = 5,6,7) × 10 wide.
        assert_eq!(lots.len(), 1);
        assert_eq!(lots[0].len(), 30);
        assert!(lots[0].iter().all(|p| (5..=7).contains(&p.y)));
        // Interior is the remaining 7 rows.
        assert_eq!(interior.len(), 70);
        assert!(interior.iter().all(|p| (8..=14).contains(&p.y)));
    }

    #[test]
    fn ribbon_empty_when_no_road_touches() {
        let block = rect_block(0, 9, 0, 9);
        let road: HashSet<Point2D> = HashSet::new();

        let (lots, interior) = reserve_road_ribbon(&block, &road, 5);

        assert!(lots.is_empty());
        assert_eq!(interior, block);
    }

    #[test]
    fn connector_punches_perpendicular_alley_to_road() {
        // Road along z=0. Ribbon is z=1..=3 (3 deep). A perpendicular alley runs
        // up x=5 at z=4..=7 (in the interior, just past the ribbon).
        let road: HashSet<Point2D> = (0..=9).map(|x| Point2D::new(x, 0)).collect();
        let ribbon: HashSet<Point2D> = (0..=9)
            .flat_map(|x| (1..=3).map(move |z| Point2D::new(x, z)))
            .collect();
        let alleys: HashSet<Point2D> = (4..=7).map(|z| Point2D::new(5, z)).collect();

        let connectors = carve_ribbon_connectors(&ribbon, &alleys, &road);
        // Carves x=5, z=1..=3 (the column from the alley tip through the ribbon).
        assert_eq!(connectors, (1..=3).map(|z| Point2D::new(5, z)).collect());
    }

    #[test]
    fn connector_ignores_alley_running_parallel_to_road() {
        // Road along z=0, ribbon z=1..=3, and a 2-wide alley running parallel to
        // the road at z=4,5 (no perpendicular approach) — nothing should carve.
        let road: HashSet<Point2D> = (0..=9).map(|x| Point2D::new(x, 0)).collect();
        let ribbon: HashSet<Point2D> = (0..=9)
            .flat_map(|x| (1..=3).map(move |z| Point2D::new(x, z)))
            .collect();
        let alleys: HashSet<Point2D> = (0..=9)
            .flat_map(|x| [Point2D::new(x, 4), Point2D::new(x, 5)])
            .collect();

        let connectors = carve_ribbon_connectors(&ribbon, &alleys, &road);
        assert!(connectors.is_empty(), "parallel alley must not carve the ribbon flank");
    }

    #[test]
    fn ribbon_wraps_a_corner_as_one_component() {
        // Roads on the north (z=4) and west (x=-1) edges meet at a corner, so
        // the ribbon is an L — a single connected component.
        let block = rect_block(0, 9, 5, 14);
        let mut road: HashSet<Point2D> = (0..=9).map(|x| Point2D::new(x, 4)).collect();
        road.extend((5..=14).map(|z| Point2D::new(-1, z)));

        let (lots, _interior) = reserve_road_ribbon(&block, &road, 2);
        assert_eq!(lots.len(), 1, "L-shaped ribbon should be one component");
    }
}
