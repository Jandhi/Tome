use std::collections::{HashMap, HashSet};

use crate::{
    geometry::{voronoi_fill_with_recenter, Point2D, CARDINALS_2D},
    noise::RNG,
};

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
