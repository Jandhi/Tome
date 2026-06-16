use crate::editor::Editor;
use crate::generator::buildings_v2::footprint::{Footprint, Plot, SizeClass};
use crate::generator::buildings_v2::walls::{DoorStyle, OpeningKind, WallSegments, segment_cells};
use crate::generator::buildings_v2::{BuildCtx, BuildingContext, Culture, HouseOutput, build_house};
use crate::generator::buildings_v2::roof::RoofStyle;
use crate::geometry::{Cardinal, Point2D, Rect2D};
use crate::minecraft::Block;
use crate::noise::RNG;

use super::frontage::Frontage;

/// How far outward (toward the road) to probe from a frontage cell when sampling
/// the road surface. On a straight edge the road sits one cell past the frontage;
/// a couple more covers stepped/diagonal frontage.
const ROAD_PROBE_DEPTH: i32 = 3;

/// Gap, in cells, between adjacent front-row houses along a single chain.
/// 0 = houses may share a wall (terraced row).
pub const SIDE_BUFFER_CELLS: i32 = 0;

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
        // Find the largest house that actually fits at this cell, shrinking the
        // size class and depth before giving up (see `find_fitting_house`).
        let Some((rect, size_class, front_width)) =
            find_fitting_house(frontage, plot, cursor, chain_len, size_pool, ctx.rng)
        else {
            // Nothing in the pool fits here — block too shallow at this point,
            // or the cells were already claimed by a perpendicular frontage.
            // Skip a single cell and probe again.
            cursor += 1;
            continue;
        };

        let plot_bounds = synthetic_plot_bounds(&rect, frontage.outward);
        let footprint = Footprint::from_rect(rect);
        let mut bctx = BuildingContext::new(culture, size_class, roof_style);

        // Align the primary (road-facing) door with the street: pin the floor to
        // the road surface in front of this frontage. A road slab sits one block
        // above the adjacent full-block surface, so this raises the floor a block
        // when the door lands on a slab (step up over the slab into the doorway).
        let chain_slice = &frontage.cells[cursor as usize..(cursor + front_width) as usize];
        bctx.base_y_override = road_floor_level(chain_slice, frontage.outward, ctx.editor);

        match build_house(ctx, footprint, &bctx, plot_bounds).await {
            Ok(house) => {
                // Safety net: clear any road-slab lip a taller neighbouring road
                // cell pokes into the two cells just outside each ground-floor door.
                clear_door_thresholds(ctx.editor, &house.wall_segs).await;
                mark_used(plot, &rect, SIDE_BUFFER_CELLS);
                out.push(house);
                cursor += front_width + SIDE_BUFFER_CELLS;
            }
            Err(msg) => {
                log::warn!(
                    "walk_and_place: build_house failed at cursor {} ({:?} {:?}..={:?} facing {:?}): {}",
                    cursor, size_class, rect.min(), rect.max(), frontage.outward, msg
                );
                cursor += 1;
            }
        }
    }

    out
}

/// Sample the floor level a house should sit at so its primary (road-facing)
/// door lines up with the street. Probes outward (toward the road) from each cell
/// of `chain_slice`, reads the topmost solid block of the nearest road column,
/// and returns `topmost_solid_y + 1` — the level a player stands on. A road slab
/// sits one block higher than the adjacent full-block surface, so a door landing
/// on a slab naturally raises the floor by one. Returns the *nearest* road hit
/// across the slice, or `None` when no road column is found (the caller then
/// falls back to the terrain percentile in `place_foundation`).
fn road_floor_level(chain_slice: &[Point2D], outward: Cardinal, editor: &Editor) -> Option<i32> {
    let dir: Point2D = outward.into();
    let mut best: Option<(i32, i32)> = None; // (distance, floor_level)
    for &cell in chain_slice {
        for step in 1..=ROAD_PROBE_DEPTH {
            let probe = cell + Point2D::new(dir.x * step, dir.y * step);
            if let Some(level) = road_surface_top(editor, probe) {
                if best.map_or(true, |(bd, _)| step < bd) {
                    best = Some((step, level));
                }
                break;
            }
        }
    }
    best.map(|(_, level)| level)
}

/// Floor level to stand on at the road column `cell` (= topmost solid block's
/// `y + 1`). Scans a tight window around the cell's terrain height so it ignores
/// the debug road markers that float high above the surface. `None` if the
/// window holds only air (no road here).
fn road_surface_top(editor: &Editor, cell: Point2D) -> Option<i32> {
    let ref_y = editor.world().get_ocean_floor_height_at(cell);
    for y in (ref_y - 5..=ref_y + 5).rev() {
        if let Some(block) = editor.try_get_block(cell.add_y(y)) {
            if !block.id.is_air() {
                return Some(y + 1);
            }
        }
    }
    None
}

/// Is this a road-paving slab? Mirrors the placement integration test's filter:
/// only clear stone/cobble/brick-family slabs, never a house's own (wooden)
/// door-ramp slab.
fn is_road_slab(block: &Block) -> bool {
    let id = block.id.as_str();
    id.contains("slab")
        && (id.contains("cobble") || id.contains("stone") || id.contains("brick")
            || id.contains("andesite") || id.contains("granite") || id.contains("diorite")
            || id.contains("gravel"))
}

/// Safety pass after a house is placed: clear any road-slab lip sitting in the
/// two cells directly outside each ground-floor door, at the sill and one above.
/// The floor is already aligned to (and raised over) the road *directly* in front
/// of the door, but a taller neighbouring road cell can still poke a half-block
/// lip into the doorway — force air there so the threshold stays walkable.
async fn clear_door_thresholds(editor: &Editor, wall_segs: &WallSegments) {
    let air: Block = "air".into();
    for (seg, opening) in wall_segs.doors() {
        if seg.floor != 0 {
            continue;
        }
        let width = match opening.kind {
            OpeningKind::Door(DoorStyle::Double) => 2,
            OpeningKind::Door(_) => 1,
            _ => continue,
        };
        let cells = segment_cells(seg);
        let door_idx = opening.offset as usize;
        let outward: Point2D = (-seg.facing).into();
        let sill = seg.base_y;
        for w in 0..width {
            let Some(&door_cell) = cells.get(door_idx + w) else { continue };
            for step in 1..=2 {
                let c = door_cell + Point2D::new(outward.x * step, outward.y * step);
                for y in sill..=sill + 1 {
                    let p = c.add_y(y);
                    if editor.try_get_block(p).is_some_and(|b| is_road_slab(&b)) {
                        editor.place_block_forced(&air, p).await;
                    }
                }
            }
        }
    }
}

/// Find the largest house that fits at `cursor` on the chain, falling back to
/// smaller size classes and then shallower depths before giving up. Returns the
/// anchored rect, the class that fit, and its front width (street-axis span,
/// used to advance the cursor). `None` means nothing in the pool fits here.
///
/// This mirrors `fill_interior`'s `try_generate_footprint`: a random *primary*
/// class keeps the street varied, but on a tight spot we retry smaller classes
/// and depths rather than skipping the cell outright. Depth is the dimension
/// that overruns shallow blocks (and that perpendicular frontages eat into at
/// corners), so it's shrunk last, cell by cell, down to the class minimum.
fn find_fitting_house(
    frontage: &Frontage,
    plot: &Plot,
    cursor: i32,
    chain_len: i32,
    size_pool: &[SizeClass],
    rng: &mut RNG,
) -> Option<(Rect2D, SizeClass, i32)> {
    let primary = size_pool[rng.rand_i32_range(0, size_pool.len() as i32) as usize];
    let mut classes: Vec<SizeClass> = vec![primary];
    let mut fallback: Vec<SizeClass> = size_pool.iter().copied().filter(|s| *s != primary).collect();
    fallback.sort_by_key(|s| s.min_side()); // smallest first
    classes.extend(fallback);

    for size_class in classes {
        let width_range = size_class.front_width_range();
        let depth_range = size_class.depth_range();

        // Largest front width that still fits the remaining chain.
        let max_width = (chain_len - cursor).min(*width_range.end());
        if max_width < *width_range.start() {
            continue; // not enough chain left for even this class's minimum
        }
        let front_width = rng.rand_i32_range(*width_range.start(), max_width + 1);
        let chain_slice = &frontage.cells[cursor as usize..(cursor + front_width) as usize];

        // Roll a target depth, then shrink toward the class minimum until the
        // footprint fits the plot.
        let start_depth = rng.rand_i32_range(*depth_range.start(), *depth_range.end() + 1);
        for depth in (*depth_range.start()..=start_depth).rev() {
            let rect = rect_from_frontage(chain_slice, frontage.outward, depth);
            if rect_fits_in_plot(plot, &rect) {
                return Some((rect, size_class, front_width));
            }
        }
    }
    None
}

/// Anchor a `front_width × depth` rect against `chain_slice`, extending `depth`
/// cells in `-outward` (into the block).
///
/// The rect spans the slice's full extent along the street, and its front edge
/// sits at the slice's **block-interior extreme** on the perpendicular axis —
/// the cell farthest from the road. For a straight (collinear) slice every cell
/// shares that coordinate, so this is the classic flush-against-the-road rect.
/// For a stepped (diagonal) slice it guarantees the footprint stays entirely on
/// the block side of the staircase — never poking onto the road — at the cost of
/// a small triangular front verge, giving a stepped terrace along the street.
pub fn rect_from_frontage(chain_slice: &[Point2D], outward: Cardinal, depth: i32) -> Rect2D {
    assert!(!chain_slice.is_empty(), "chain_slice must be non-empty");
    let min_x = chain_slice.iter().map(|p| p.x).min().unwrap();
    let max_x = chain_slice.iter().map(|p| p.x).max().unwrap();
    let min_z = chain_slice.iter().map(|p| p.y).min().unwrap();
    let max_z = chain_slice.iter().map(|p| p.y).max().unwrap();
    match outward {
        Cardinal::North => {
            // Road is -z, inside is +z. Front line = deepest (max z) cell.
            Rect2D::from_points(Point2D::new(min_x, max_z), Point2D::new(max_x, max_z + depth - 1))
        }
        Cardinal::South => {
            // Road is +z, inside is -z. Front line = min z.
            Rect2D::from_points(Point2D::new(min_x, min_z - depth + 1), Point2D::new(max_x, min_z))
        }
        Cardinal::East => {
            // Road is +x, inside is -x. Front line = min x.
            Rect2D::from_points(Point2D::new(min_x - depth + 1, min_z), Point2D::new(min_x, max_z))
        }
        Cardinal::West => {
            // Road is -x, inside is +x. Front line = max x.
            Rect2D::from_points(Point2D::new(max_x, min_z), Point2D::new(max_x + depth - 1, max_z))
        }
    }
}

/// Plot bounds that steer `place_doors` to the road-facing wall: a rectangle
/// **flush with the building's road-facing edge** but extended `MARGIN` cells on
/// the other three sides. `distance_to_plot_edge` then reads exactly 0 for the
/// road-facing wall (it sits on a plot edge) and a positive margin for the back
/// and side walls, so the primary door reliably lands facing the road.
///
/// A 1×1 sentinel does *not* work: `distance_to_plot_edge` takes the *min* of the
/// per-axis distances, so the front and back walls — both centred on the
/// sentinel's axis — tie at 0 and the door can land on the back wall.
pub fn synthetic_plot_bounds(rect: &Rect2D, outward: Cardinal) -> Rect2D {
    const MARGIN: i32 = 8;
    let (min, max) = (rect.min(), rect.max());
    match outward {
        // Road on -z: flush along the north (min.y) edge.
        Cardinal::North => Rect2D::from_points(
            Point2D::new(min.x - MARGIN, min.y),
            Point2D::new(max.x + MARGIN, max.y + MARGIN),
        ),
        // Road on +z: flush along the south (max.y) edge.
        Cardinal::South => Rect2D::from_points(
            Point2D::new(min.x - MARGIN, min.y - MARGIN),
            Point2D::new(max.x + MARGIN, max.y),
        ),
        // Road on +x: flush along the east (max.x) edge.
        Cardinal::East => Rect2D::from_points(
            Point2D::new(min.x - MARGIN, min.y - MARGIN),
            Point2D::new(max.x, max.y + MARGIN),
        ),
        // Road on -x: flush along the west (min.x) edge.
        Cardinal::West => Rect2D::from_points(
            Point2D::new(min.x, min.y - MARGIN),
            Point2D::new(max.x + MARGIN, max.y + MARGIN),
        ),
    }
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
    fn synthetic_plot_bounds_is_flush_on_road_side_extended_elsewhere() {
        // House rect (5,10)-(8,18), road to the north (-z).
        let rect = Rect2D::from_points(Point2D::new(5, 10), Point2D::new(8, 18));
        let pb = synthetic_plot_bounds(&rect, Cardinal::North);
        // Flush along the north edge (min.y unchanged), extended 8 on the others.
        assert_eq!(pb.min(), Point2D::new(5 - 8, 10));
        assert_eq!(pb.max(), Point2D::new(8 + 8, 18 + 8));
        // The north (road-facing) wall midpoint sits on a plot edge → distance 0.
        let front_mid = Point2D::new((5 + 8) / 2, 10);
        assert_eq!(distance_to_plot_edge_test(front_mid, &pb), 0);
        // The south (back) wall is a full margin away.
        let back_mid = Point2D::new((5 + 8) / 2, 18);
        assert!(distance_to_plot_edge_test(back_mid, &pb) > 0);
    }

    // Mirror of walls::openings::distance_to_plot_edge (private there).
    fn distance_to_plot_edge_test(point: Point2D, b: &Rect2D) -> i32 {
        let (min, max) = (b.min(), b.max());
        (point.x - min.x).abs()
            .min((point.x - max.x).abs())
            .min((point.y - min.y).abs())
            .min((point.y - max.y).abs())
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
