use crate::generator::buildings_v2::footprint::{Footprint, Plot, SizeClass};
use crate::generator::buildings_v2::{BuildCtx, BuildingContext, Culture, HouseOutput, build_house};
use crate::generator::buildings_v2::roof::RoofStyle;
use crate::geometry::{Cardinal, Point2D, Rect2D};

use super::frontage::Frontage;

/// Gap, in cells, between adjacent front-row houses along a single chain.
pub const SIDE_BUFFER_CELLS: i32 = 1;

/// Walk a frontage chain and place rectangular houses along it. Houses extend
/// away from the road (in `-outward`). Cells of placed houses (plus a
/// `SIDE_BUFFER_CELLS` buffer) are marked unusable on `plot` so the interior
/// fill pass won't overlap.
pub async fn walk_and_place(
    frontage: &Frontage,
    plot: &mut Plot,
    ctx: &mut BuildCtx<'_>,
    culture: Culture,
    roof_style: RoofStyle,
    size_pool: &[SizeClass],
) -> Vec<HouseOutput> {
    let mut out = Vec::new();

    if size_pool.is_empty() || frontage.cells.is_empty() {
        return out;
    }

    let min_front_width = size_pool
        .iter()
        .map(|s| *s.front_width_range().start())
        .min()
        .unwrap_or(0);
    if (frontage.cells.len() as i32) < min_front_width {
        return out;
    }

    // Random starting offset so adjacent blocks don't all line up at x=0.
    let mut cursor: i32 = if min_front_width > 1 {
        ctx.rng.rand_i32_range(0, min_front_width)
    } else {
        0
    };
    let chain_len = frontage.cells.len() as i32;

    while cursor + min_front_width <= chain_len {
        // Pick a size class for this slot.
        let pick = ctx.rng.rand_i32_range(0, size_pool.len() as i32) as usize;
        let size_class = size_pool[pick];

        let width_range = size_class.front_width_range();
        let depth_range = size_class.depth_range();
        let front_width = ctx.rng.rand_i32_range(*width_range.start(), *width_range.end() + 1);
        let depth = ctx.rng.rand_i32_range(*depth_range.start(), *depth_range.end() + 1);

        if cursor + front_width > chain_len {
            // This size won't fit; advance and try a smaller one next iteration.
            cursor += 1;
            continue;
        }

        let chain_slice = &frontage.cells[cursor as usize..(cursor + front_width) as usize];
        let rect = rect_from_frontage(chain_slice, frontage.outward, depth);

        if !rect_fits_in_plot(plot, &rect) {
            cursor += 1;
            continue;
        }

        let footprint = Footprint::from_rect(rect);
        let plot_bounds = synthetic_plot_bounds(chain_slice, frontage.outward);
        let bctx = BuildingContext::new(culture, size_class, roof_style);

        match build_house(ctx, footprint, &bctx, plot_bounds).await {
            Ok(house) => {
                mark_used(plot, &rect, SIDE_BUFFER_CELLS);
                out.push(house);
                cursor += front_width + SIDE_BUFFER_CELLS;
            }
            Err(msg) => {
                log::warn!("walk_and_place: build_house failed at cursor {}: {}", cursor, msg);
                cursor += 1;
            }
        }
    }

    out
}

/// Anchor a `front_width × depth` rect with one short edge flush along
/// `chain_slice`, extending in `-outward` (into the block).
pub fn rect_from_frontage(chain_slice: &[Point2D], outward: Cardinal, depth: i32) -> Rect2D {
    assert!(!chain_slice.is_empty(), "chain_slice must be non-empty");
    let first = chain_slice[0];
    let last = *chain_slice.last().unwrap();
    match outward {
        Cardinal::North => {
            // Chain runs along x at fixed z. Inside the block is +z.
            Rect2D::from_points(first, Point2D::new(last.x, first.y + depth - 1))
        }
        Cardinal::South => {
            // Chain runs along x at fixed z. Inside the block is -z.
            Rect2D::from_points(Point2D::new(first.x, first.y - depth + 1), last)
        }
        Cardinal::East => {
            // Chain runs along z at fixed x. Inside the block is -x.
            Rect2D::from_points(Point2D::new(first.x - depth + 1, first.y), last)
        }
        Cardinal::West => {
            // Chain runs along z at fixed x. Inside the block is +x.
            Rect2D::from_points(first, Point2D::new(first.x + depth - 1, last.y))
        }
    }
}

/// A 1×1 sentinel rect at the chain's midpoint, offset one cell outward (onto
/// the road). Used as the `plot_bounds` argument to `place_doors` so the
/// road-facing wall has the smallest distance-to-plot-edge and wins door
/// placement.
///
/// We use a 1×1 rect (not a strip along the chain) so the building's side
/// walls don't share an axis with the strip's east/west edges — which would
/// give those walls distance 0 and steal the door from the road-facing wall.
pub fn synthetic_plot_bounds(chain_slice: &[Point2D], outward: Cardinal) -> Rect2D {
    let mid = chain_slice[chain_slice.len() / 2];
    let sentinel = mid + Point2D::from(outward);
    Rect2D::from_points(sentinel, sentinel)
}

fn rect_fits_in_plot(plot: &Plot, rect: &Rect2D) -> bool {
    for p in rect.iter() {
        if !plot.bounds.contains(p) {
            return false;
        }
        if !plot.is_usable(p) {
            return false;
        }
    }
    true
}

fn mark_used(plot: &mut Plot, rect: &Rect2D, buffer: i32) {
    let plot_min = plot.bounds.min();
    let min = rect.min();
    let max = rect.max();
    for x in (min.x - buffer)..=(max.x + buffer) {
        for z in (min.y - buffer)..=(max.y + buffer) {
            let lx = x - plot_min.x;
            let lz = z - plot_min.y;
            if lx < 0 || lz < 0 {
                continue;
            }
            let lx = lx as usize;
            let lz = lz as usize;
            if lx < plot.usable.len() && lz < plot.usable[0].len() {
                plot.usable[lx][lz] = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_from_frontage_north_extends_south() {
        let chain = vec![Point2D::new(5, 10), Point2D::new(6, 10), Point2D::new(7, 10)];
        let rect = rect_from_frontage(&chain, Cardinal::North, 6);
        assert_eq!(rect.min(), Point2D::new(5, 10));
        assert_eq!(rect.max(), Point2D::new(7, 15));
    }

    #[test]
    fn rect_from_frontage_south_extends_north() {
        let chain = vec![Point2D::new(5, 20), Point2D::new(6, 20), Point2D::new(7, 20)];
        let rect = rect_from_frontage(&chain, Cardinal::South, 6);
        assert_eq!(rect.min(), Point2D::new(5, 15));
        assert_eq!(rect.max(), Point2D::new(7, 20));
    }

    #[test]
    fn rect_from_frontage_east_extends_west() {
        let chain = vec![Point2D::new(20, 5), Point2D::new(20, 6), Point2D::new(20, 7)];
        let rect = rect_from_frontage(&chain, Cardinal::East, 6);
        assert_eq!(rect.min(), Point2D::new(15, 5));
        assert_eq!(rect.max(), Point2D::new(20, 7));
    }

    #[test]
    fn rect_from_frontage_west_extends_east() {
        let chain = vec![Point2D::new(10, 5), Point2D::new(10, 6), Point2D::new(10, 7)];
        let rect = rect_from_frontage(&chain, Cardinal::West, 6);
        assert_eq!(rect.min(), Point2D::new(10, 5));
        assert_eq!(rect.max(), Point2D::new(15, 7));
    }

    #[test]
    fn synthetic_plot_bounds_sits_one_cell_outside_block() {
        let chain = vec![Point2D::new(5, 10), Point2D::new(6, 10), Point2D::new(7, 10)];
        let pb = synthetic_plot_bounds(&chain, Cardinal::North);
        // 1×1 rect at chain midpoint (6, 10) offset 1 cell north to (6, 9).
        assert_eq!(pb.min(), Point2D::new(6, 9));
        assert_eq!(pb.max(), Point2D::new(6, 9));
    }

    #[test]
    fn mark_used_writes_buffer() {
        let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(9, 9));
        let mut plot = Plot::fully_usable(bounds);
        let rect = Rect2D::from_points(Point2D::new(2, 2), Point2D::new(3, 3));
        mark_used(&mut plot, &rect, 1);
        // Rect plus 1-cell buffer = (1, 1) to (4, 4).
        for x in 1..=4 {
            for z in 1..=4 {
                assert!(!plot.is_usable(Point2D::new(x, z)), "cell ({}, {}) should be marked used", x, z);
            }
        }
        // Edges just outside the buffer remain usable.
        assert!(plot.is_usable(Point2D::new(0, 0)));
        assert!(plot.is_usable(Point2D::new(5, 5)));
    }
}
