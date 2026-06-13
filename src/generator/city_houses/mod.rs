//! Street-frontage pass for placing procedural houses in city blocks.
//!
//! For each block, identifies chains of cells adjacent to `BuildClaim::Path`,
//! then walks each chain and places rectangular houses with their short edge
//! flush against the road. The door-direction trick lives in `walk.rs` — we
//! pass a thin synthetic plot_bounds outside the house on the road side, so
//! `place_doors`'s distance-to-plot-edge score picks the road-facing wall.
//!
//! Interior fill is **not** handled here — callers are expected to run their
//! existing greedy plot fill afterward against the same `Plot` (with the
//! frontage houses already marked unusable).

pub mod frontage;
pub mod walk;
#[cfg(test)]
mod test;

use std::collections::HashSet;

use crate::generator::buildings_v2::footprint::{Footprint, Plot, SizeClass, generate_footprint_biased};
use crate::generator::buildings_v2::{BuildCtx, BuildingContext, Culture, HouseOutput, build_house};
use crate::generator::buildings_v2::roof::RoofStyle;
use crate::geometry::{Point2D, Rect2D};

pub use frontage::{Frontage, detect_frontages, detect_perimeter_frontages, frontage_from_roads};
pub use walk::{SIDE_BUFFER_CELLS, rect_from_frontage, synthetic_plot_bounds, walk_and_place};

/// Default size classes eligible for the frontage pass — small, townhouse-style.
pub fn default_frontage_size_pool() -> Vec<SizeClass> {
    vec![SizeClass::Cottage, SizeClass::House]
}

/// Default size classes for the interior fill pass — biased to larger,
/// irregular shapes since the interior is where halls and manors fit.
pub fn default_interior_size_pool() -> Vec<SizeClass> {
    vec![SizeClass::Hall, SizeClass::Manor, SizeClass::House, SizeClass::Cottage]
}

/// Cells around a placed footprint (per side) that are marked unusable so
/// adjacent buildings don't share walls. 0 = buildings may sit flush together.
pub const INTERIOR_BUFFER_CELLS: i32 = 0;

/// Greedy interior fill against whatever `plot.usable` cells remain. Rotates
/// through `size_pool`; if a class doesn't fit, falls back to smaller classes
/// before giving up.
pub async fn fill_interior(
    plot: &mut Plot,
    ctx: &mut BuildCtx<'_>,
    culture: Culture,
    roof_style: RoofStyle,
    size_pool: &[SizeClass],
    max_buildings: usize,
) -> Vec<HouseOutput> {
    let mut out = Vec::new();
    if size_pool.is_empty() {
        return out;
    }

    let mut attempt = 0usize;
    while out.len() < max_buildings {
        let size_class = size_pool[attempt % size_pool.len()];
        attempt += 1;

        let footprint = match try_generate_footprint(ctx, plot, size_class, size_pool, culture.square_bias()) {
            Some(fp) => fp,
            None => break,
        };
        let placed_class = classify_footprint(&footprint, size_pool).unwrap_or(size_class);

        mark_footprint_used(plot, &footprint, INTERIOR_BUFFER_CELLS);

        let bctx = BuildingContext::new(culture, placed_class, roof_style);
        let plot_bounds = plot.bounds;
        match build_house(ctx, footprint, &bctx, plot_bounds).await {
            Ok(house) => out.push(house),
            Err(msg) => {
                log::warn!("fill_interior: build_house failed: {}", msg);
                continue;
            }
        }
    }
    out
}

/// Try the requested size class first; if it can't find a fit, try the smaller
/// classes from `size_pool` (descending order) before giving up.
fn try_generate_footprint(
    ctx: &mut BuildCtx<'_>,
    plot: &Plot,
    primary: SizeClass,
    size_pool: &[SizeClass],
    square_bias: i32,
) -> Option<Footprint> {
    if let Some(fp) = generate_footprint_biased(ctx.rng, plot, &primary, square_bias) {
        return Some(fp);
    }
    let mut fallback: Vec<SizeClass> = size_pool.iter().copied().filter(|s| *s != primary).collect();
    fallback.sort_by_key(|s| s.min_side());
    for s in fallback {
        if let Some(fp) = generate_footprint_biased(ctx.rng, plot, &s, square_bias) {
            return Some(fp);
        }
    }
    None
}

/// Pick the size class whose target-area range best brackets this footprint.
/// Used so the BuildingContext reports the class actually generated, not the
/// one the caller originally asked for.
fn classify_footprint(footprint: &Footprint, size_pool: &[SizeClass]) -> Option<SizeClass> {
    let area = footprint.filled_points().len() as i32;
    let mut best = None;
    let mut best_diff = i32::MAX;
    for &s in size_pool {
        let mid = (s.target_area_min() + s.target_area_max()) / 2;
        let diff = (mid - area).abs();
        if diff < best_diff {
            best_diff = diff;
            best = Some(s);
        }
    }
    best
}

fn mark_footprint_used(plot: &mut Plot, footprint: &Footprint, buffer: i32) {
    let plot_min = plot.bounds.min();
    for point in footprint.filled_points() {
        for dx in -buffer..=buffer {
            for dz in -buffer..=buffer {
                let p = Point2D::new(point.x + dx, point.y + dz);
                let lx = p.x - plot_min.x;
                let lz = p.y - plot_min.y;
                if lx < 0 || lz < 0 { continue; }
                let lx = lx as usize;
                let lz = lz as usize;
                if lx < plot.usable.len() && lz < plot.usable[0].len() {
                    plot.usable[lx][lz] = false;
                }
            }
        }
    }
}

/// Run the frontage pass for a single block. If the block has no
/// `BuildClaim::Path` neighbours, falls back to using the block's outer
/// perimeter as frontage so interior blocks still get houses lining their
/// edges. Returns the placed houses; `plot` is updated so subsequent passes
/// (e.g. interior fill) skip cells already occupied.
pub async fn place_block_frontage(
    block: &HashSet<Point2D>,
    plot: &mut Plot,
    ctx: &mut BuildCtx<'_>,
    culture: Culture,
    roof_style: RoofStyle,
    size_pool: &[SizeClass],
) -> Vec<HouseOutput> {
    let mut frontages = detect_frontages(block, ctx.editor);
    if frontages.is_empty() {
        frontages = detect_perimeter_frontages(block);
    }
    let mut out = Vec::new();
    for frontage in frontages {
        let placed = walk_and_place(&frontage, plot, ctx, culture, roof_style, size_pool).await;
        out.extend(placed);
    }
    out
}

/// Convenience: build a `Plot` whose usable cells are exactly the inner cells
/// of a city block, intersected with a bounding rectangle.
pub fn plot_from_block(block: &HashSet<Point2D>) -> Option<Plot> {
    if block.is_empty() {
        return None;
    }
    let min_x = block.iter().map(|p| p.x).min().unwrap();
    let min_z = block.iter().map(|p| p.y).min().unwrap();
    let max_x = block.iter().map(|p| p.x).max().unwrap();
    let max_z = block.iter().map(|p| p.y).max().unwrap();
    let bounds = Rect2D::from_points(Point2D::new(min_x, min_z), Point2D::new(max_x, max_z));

    let w = (max_x - min_x + 1) as usize;
    let h = (max_z - min_z + 1) as usize;
    let mut usable = vec![vec![false; h]; w];
    for p in block {
        let lx = (p.x - min_x) as usize;
        let lz = (p.y - min_z) as usize;
        usable[lx][lz] = true;
    }
    Some(Plot::new(bounds, usable))
}
