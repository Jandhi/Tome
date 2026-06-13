//! Regularization of the urban footprint.
//!
//! The raw urban region is the union of every `Urban`-classified district's cells.
//! Because districts grow from organic, flood-filled parcels, that union has a noisy,
//! concave perimeter — bays, notches and thin tendrils — which the wall tracer
//! ([`super::wall::trace_wall_loops`]) follows faithfully, producing a blobby outline.
//!
//! [`regularize_urban_footprint`] morphologically smooths that cell set into a compact,
//! mostly-convex footprint that reads as a realistic walled city, and
//! [`reconcile_districts_to_footprint`] re-votes each district's `Urban`/`Rural`
//! classification against the new footprint so every downstream consumer stays
//! consistent with "inside the wall".
//!
//! Every cell *added* by the morphology (dilation and hole-filling) is **terrain-clipped**:
//! it is never an `OffLimits` (steep/rough) or water cell. This keeps walls hugging real
//! cliffs and rivers — only soft organic noise gets smoothed.

use std::collections::{HashMap, HashSet};

use log::info;

use crate::{editor::World, geometry::{Point2D, CARDINALS_2D}};

use super::{
    constants::{CLOSE_RADIUS, OPEN_RADIUS, FOOTPRINT_RECLASSIFY_THRESHOLD},
    wall::{connected_components, trace_wall_loops},
    District, DistrictID, ParcelType,
};

/// True if `point` may be absorbed into the footprint: in-bounds, not water, and not
/// an off-limits (steep/rough) cell. This is the terrain clip that stops dilation and
/// hole-filling from spilling onto cliffs or open water.
fn is_buildable(world: &World, point: Point2D) -> bool {
    world.is_in_bounds_2d(point)
        && !world.is_water(point)
        && world.get_parcel_type(point) != Some(ParcelType::OffLimits)
}

/// Grow `set` outward by `radius` cells (4-connectivity), adding only cells for which
/// `allowed` holds. One ring per iteration, so `radius` is a Manhattan distance.
fn dilate(set: &HashSet<Point2D>, radius: i32, allowed: &impl Fn(Point2D) -> bool) -> HashSet<Point2D> {
    let mut current = set.clone();
    for _ in 0..radius {
        let mut next = current.clone();
        for &point in &current {
            for dir in CARDINALS_2D {
                let neighbour = point + dir;
                if !current.contains(&neighbour) && allowed(neighbour) {
                    next.insert(neighbour);
                }
            }
        }
        current = next;
    }
    current
}

/// Shrink `set` inward by `radius` cells: a cell survives only if its whole
/// 4-neighbourhood (out to `radius`) stays inside the set. Out-of-bounds neighbours are
/// ignored so the map border is never peeled. Erosion only removes, so it needs no
/// terrain clip.
fn erode(set: &HashSet<Point2D>, radius: i32, world: &World) -> HashSet<Point2D> {
    let mut current = set.clone();
    for _ in 0..radius {
        let next: HashSet<Point2D> = current
            .iter()
            .filter(|&&point| {
                CARDINALS_2D.iter().all(|&dir| {
                    let neighbour = point + dir;
                    // Ignore out-of-bounds neighbours so we don't shrink at the map edge.
                    !world.is_in_bounds_2d(neighbour) || current.contains(&neighbour)
                })
            })
            .copied()
            .collect();
        current = next;
    }
    current
}

/// Add every cell enclosed by `set` (not reachable from the world border through
/// non-footprint cells) that is also buildable. Genuine interior water/cliffs stay as
/// holes — they fail the terrain clip — so the wall hugs them; small morphology
/// artifacts on normal terrain get filled.
fn fill_holes(set: &HashSet<Point2D>, world: &World) -> HashSet<Point2D> {
    let rect = world.world_rect_2d();

    // Flood-fill the "outside": every in-bounds non-footprint cell reachable from the
    // border. Anything in-bounds and not in the footprint but not reached is enclosed.
    let mut outside: HashSet<Point2D> = HashSet::new();
    let mut stack: Vec<Point2D> = Vec::new();
    for x in 0..rect.size.x {
        for &point in &[Point2D::new(x, 0), Point2D::new(x, rect.size.y - 1)] {
            if !set.contains(&point) && outside.insert(point) {
                stack.push(point);
            }
        }
    }
    for y in 0..rect.size.y {
        for &point in &[Point2D::new(0, y), Point2D::new(rect.size.x - 1, y)] {
            if !set.contains(&point) && outside.insert(point) {
                stack.push(point);
            }
        }
    }

    while let Some(point) = stack.pop() {
        for dir in CARDINALS_2D {
            let neighbour = point + dir;
            if world.is_in_bounds_2d(neighbour)
                && !set.contains(&neighbour)
                && outside.insert(neighbour)
            {
                stack.push(neighbour);
            }
        }
    }

    let mut filled = set.clone();
    for point in world.iter_points_2d() {
        if !set.contains(&point) && !outside.contains(&point) && is_buildable(world, point) {
            filled.insert(point);
        }
    }
    filled
}

/// Number of direction changes ("corners") around a closed 4-connected ring — the most
/// direct measure of outline blobbiness. A perfect rectangle has 4; a wiggly organic
/// boundary has many more.
fn count_turns(loop_: &[Point2D]) -> usize {
    let n = loop_.len();
    if n < 3 {
        return 0;
    }
    (0..n)
        .filter(|&i| {
            let prev = loop_[(i + n - 1) % n];
            let cur = loop_[i];
            let next = loop_[(i + 1) % n];
            (cur - prev) != (next - cur)
        })
        .count()
}

/// Outline complexity of a filled region: `(loops, total perimeter, total turns)`,
/// computed with the same tracer the wall uses ([`trace_wall_loops`]). Used to log how
/// much the regularization straightened the boundary.
fn outline_metrics(region: &HashSet<Point2D>) -> (usize, usize, usize) {
    let loops = trace_wall_loops(region);
    let perimeter = loops.iter().map(|l| l.len()).sum();
    let turns = loops.iter().map(|l| count_turns(l)).sum();
    (loops.len(), perimeter, turns)
}

/// Regularize the raw urban cell set into a compact, mostly-convex footprint:
/// closing (fill concave bays) → opening (trim thin tendrils) → keep the largest
/// connected component → fill enclosed holes. All additive steps are terrain-clipped.
pub fn regularize_urban_footprint(world: &World, raw_urban: &HashSet<Point2D>) -> HashSet<Point2D> {
    if raw_urban.is_empty() {
        return HashSet::new();
    }

    let allowed = |p: Point2D| is_buildable(world, p);

    // Closing: dilate then erode — fills bays/notches and bridges hairline gaps.
    let closed = erode(&dilate(raw_urban, CLOSE_RADIUS, &allowed), CLOSE_RADIUS, world);
    // Opening: erode then dilate — amputates tendrils/peninsulas thinner than the radius.
    let opened = dilate(&erode(&closed, OPEN_RADIUS, world), OPEN_RADIUS, &allowed);

    // Keep only the largest connected component; drop detached specks.
    let largest = connected_components(&opened)
        .into_iter()
        .max_by_key(|component| component.len())
        .unwrap_or_default();

    let footprint = fill_holes(&largest, world);

    info!(
        "[Footprint] Regularized urban footprint: {} raw cells -> {} cells",
        raw_urban.len(),
        footprint.len()
    );

    // Straightness readout: how much simpler the traced outline got. Fewer loops and,
    // above all, fewer turns mean a straighter, less blobby wall.
    let (raw_loops, raw_perimeter, raw_turns) = outline_metrics(raw_urban);
    let (fp_loops, fp_perimeter, fp_turns) = outline_metrics(&footprint);
    info!(
        "[Footprint] Outline: loops {}->{}, perimeter {}->{}, turns {}->{}",
        raw_loops, fp_loops, raw_perimeter, fp_perimeter, raw_turns, fp_turns
    );

    footprint
}

/// Re-vote each district's `Urban`/`Rural` classification against `footprint`: a
/// district is `Urban` iff at least `FOOTPRINT_RECLASSIFY_THRESHOLD` of its cells lie
/// inside the footprint. `OffLimits` districts (including borders) are left untouched.
///
/// This keeps the district-type consumers (rural placement, urban special-structure
/// gathering, path network) aligned with the regularized outline — a district swallowed
/// by closing flips `Urban`, a tendril trimmed by opening flips `Rural`.
pub fn reconcile_districts_to_footprint(
    districts: &mut HashMap<DistrictID, District>,
    footprint: &HashSet<Point2D>,
) {
    let tally = |districts: &HashMap<DistrictID, District>| {
        let mut urban = 0usize;
        let mut rural = 0usize;
        for d in districts.values() {
            match d.data.parcel_type {
                ParcelType::Urban => urban += 1,
                ParcelType::Rural => rural += 1,
                _ => {}
            }
        }
        (urban, rural)
    };

    let (urban_before, rural_before) = tally(districts);
    let mut flipped = 0usize;

    for district in districts.values_mut() {
        if district.data.parcel_type == ParcelType::OffLimits {
            continue;
        }

        let total = district.data.points_2d.len();
        if total == 0 {
            continue;
        }

        let inside = district
            .data
            .points_2d
            .iter()
            .filter(|p| footprint.contains(p))
            .count();

        let fraction = inside as f32 / total as f32;
        let new_type = if fraction >= FOOTPRINT_RECLASSIFY_THRESHOLD {
            ParcelType::Urban
        } else {
            ParcelType::Rural
        };

        // Per-district trace (debug level) so borderline cases near the threshold are
        // visible when tuning, without flooding the default Info log.
        log::debug!(
            "[Footprint] District {:?} {:.0}% inside footprint: {:?} -> {:?}",
            district.id, fraction * 100.0, district.data.parcel_type, new_type
        );

        if new_type != district.data.parcel_type {
            district.data.parcel_type = new_type;
            flipped += 1;
        }
    }

    let (urban_after, rural_after) = tally(districts);
    info!(
        "[Footprint] Reconcile: urban {}->{}, rural {}->{}, {} districts flipped",
        urban_before, urban_after, rural_before, rural_after, flipped
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point3D, Rect3D};

    /// A synthetic flat world large enough to hold the test shapes.
    fn world(size: i32) -> World {
        let build_area =
            Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(size, 320, size));
        World::synthetic(build_area, 64)
    }

    /// A solid axis-aligned rectangle of cells [x0,x1) × [z0,z1).
    fn filled_rect(x0: i32, z0: i32, x1: i32, z1: i32) -> HashSet<Point2D> {
        let mut set = HashSet::new();
        for x in x0..x1 {
            for z in z0..z1 {
                set.insert(Point2D::new(x, z));
            }
        }
        set
    }

    #[test]
    fn closing_fills_a_concave_bay() {
        let world = world(40);
        // A rectangle with a narrow (2-wide) bay cutting 5 cells deep into the top
        // edge. Closing (radius 4 > half-width) bridges the mouth and fills the bay;
        // the deep cells, surrounded on three sides, are reliably restored. (The single
        // cell flush with the silhouette edge is not — that is correct morphology and
        // visually irrelevant.)
        let mut region = filled_rect(5, 5, 25, 20);
        for z in 5..10 {
            region.remove(&Point2D::new(14, z));
            region.remove(&Point2D::new(15, z));
        }
        let fp = regularize_urban_footprint(&world, &region);
        assert!(fp.contains(&Point2D::new(14, 8)), "deep bay cell should be filled by closing");
        assert!(fp.contains(&Point2D::new(15, 8)), "deep bay cell should be filled by closing");
        assert!(fp.contains(&Point2D::new(14, 9)), "bay mouth should be bridged by closing");
    }

    #[test]
    fn opening_trims_a_thin_tendril() {
        let world = world(60);
        let mut region = filled_rect(5, 5, 30, 25);
        // A 1-cell-wide spur sticking far out of the blob — thinner than OPEN_RADIUS.
        for x in 30..45 {
            region.insert(Point2D::new(x, 15));
        }
        let fp = regularize_urban_footprint(&world, &region);
        assert!(!fp.contains(&Point2D::new(44, 15)), "tip of thin tendril should be trimmed");
        assert!(!fp.contains(&Point2D::new(40, 15)), "thin tendril should be trimmed");
        // The main body survives.
        assert!(fp.contains(&Point2D::new(17, 15)), "main body should remain");
    }

    #[test]
    fn footprint_never_includes_offlimits_or_water() {
        let mut world = world(40);
        // Mark a strip as water inside what would otherwise be filled.
        let water = Point2D::new(15, 6);
        world.set_water_for_test(water);

        // A bay adjacent to the water cell so closing would try to absorb it.
        let mut region = filled_rect(5, 5, 25, 20);
        region.remove(&water);
        let fp = regularize_urban_footprint(&world, &region);
        assert!(!fp.contains(&water), "terrain clip must keep water out of the footprint");
    }

    #[test]
    fn enclosed_hole_is_filled_but_speck_is_dropped() {
        let world = world(60);
        // Main blob with a 1-cell interior hole.
        let mut region = filled_rect(5, 5, 30, 25);
        let hole = Point2D::new(15, 15);
        region.remove(&hole);
        // A detached speck far away.
        region.extend(filled_rect(50, 50, 52, 52));

        let fp = regularize_urban_footprint(&world, &region);
        assert!(fp.contains(&hole), "interior hole on normal terrain should be filled");
        assert!(!fp.contains(&Point2D::new(50, 50)), "detached speck should be dropped");
        assert!(!fp.contains(&Point2D::new(51, 51)), "detached speck should be dropped");
    }

    /// Build a district occupying `cells` with the given starting classification.
    fn district(id: usize, cells: HashSet<Point2D>, parcel_type: ParcelType) -> District {
        let mut d = District::new(DistrictID(id));
        d.data.points_2d = cells;
        d.data.parcel_type = parcel_type;
        d
    }

    #[test]
    fn reconcile_revotes_against_footprint() {
        let footprint = filled_rect(0, 0, 20, 20);

        let mut districts = HashMap::new();
        // Fully inside the footprint but currently Rural -> should flip Urban.
        districts.insert(DistrictID(0), district(0, filled_rect(2, 2, 8, 8), ParcelType::Rural));
        // Fully outside the footprint but currently Urban -> should flip Rural.
        districts.insert(DistrictID(1), district(1, filled_rect(30, 30, 36, 36), ParcelType::Urban));
        // Off-limits and fully inside -> must stay Off-limits (untouched).
        districts.insert(DistrictID(2), district(2, filled_rect(10, 10, 14, 14), ParcelType::OffLimits));

        reconcile_districts_to_footprint(&mut districts, &footprint);

        assert_eq!(districts[&DistrictID(0)].data.parcel_type, ParcelType::Urban);
        assert_eq!(districts[&DistrictID(1)].data.parcel_type, ParcelType::Rural);
        assert_eq!(districts[&DistrictID(2)].data.parcel_type, ParcelType::OffLimits);
    }

    #[test]
    fn regularization_is_idempotent() {
        let world = world(60);
        let mut region = filled_rect(5, 5, 30, 25);
        region.remove(&Point2D::new(14, 5));
        for x in 30..40 {
            region.insert(Point2D::new(x, 15));
        }
        let once = regularize_urban_footprint(&world, &region);
        let twice = regularize_urban_footprint(&world, &once);
        assert_eq!(once, twice, "regularize should be a fixed point on its own output");
    }
}
