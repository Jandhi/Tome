use std::collections::{HashMap, HashSet};

use anyhow::Result;
use log::{info, warn};

use crate::{
    editor::Editor,
    generator::{
        BuildClaim,
        data::LoadedData,
        districts::{District, DistrictID, ParcelAnalysis},
        materials::{Palette, Placer},
        nbts::{Rotation, Structure, StructureID, StructureType, place_structure},
        resource_chain::{ParcelResourceAssignment, SettlementProductionResult},
        terrain::{force_height, log_trees},
    },
    geometry::{Cardinal, Point2D, Point3D, Rect2D},
    minecraft::Block,
    noise::RNG,
};

pub const NUM_CANDIDATES: usize = 16;
/// For rural placement, the candidate pool is the flattest `NUM_CANDIDATES *
/// CANDIDATE_POOL_MULTIPLE` interior cells, from which `NUM_CANDIDATES` are drawn
/// at random. A larger multiple spreads candidates out (more variety, flatter
/// bias diluted); a smaller one concentrates them on the flattest ground. This
/// lets large footprints (e.g. the 16×15 apiary) reliably find a viable pad in
/// rough forest districts where uniformly-random darts usually miss it.
pub const CANDIDATE_POOL_MULTIPLE: usize = 6;
pub const WATER_MARGIN_RADIUS: i32 = 4;
/// Fraction of the in-bounds cells in the `WATER_MARGIN_RADIUS` ring around a
/// footprint that may be water before the site is hard-rejected. A building at a
/// normal shoreline has water on roughly one side (~1/3 of the ring), which is
/// fine; a building on a small island or spit is surrounded by water on most
/// sides (well over half) and reads as "built on the water". Past this fraction
/// the candidate is dropped entirely rather than merely penalised.
pub const MAX_WATER_SURROUND_FRACTION: f32 = 0.5;
/// Width (cells) of the ring around a footprint that is graded from the
/// flattened pad height back down to natural terrain. Wider = gentler grade on
/// sloped sites, so the pad edges taper instead of dropping off as a cliff.
pub const BLEND_RADIUS: i32 = 6;
/// Maximum footprint height range (highest minus lowest natural ground cell)
/// allowed for a normal building. Footprints steeper than this are hard-rejected
/// during candidate selection — past this the per-building flatten leaves raw
/// cut/fill faces no blend ring can hide. Bypassed by `Structure::allow_steep`
/// (e.g. mines, which are meant to cut into a hillside).
pub const MAX_PLACEMENT_SLOPE: i32 = 4;
/// Pad-height percentile for `allow_steep` buildings (mines). They sit on slopes
/// far beyond `MAX_PLACEMENT_SLOPE`, where flattening to the median (0.5) would
/// perch the downhill half on a tall fill pedestal — the building looks like it's
/// floating on a plinth. A low percentile cuts the pad *into* the hillside so the
/// downhill edge meets near-natural grade and no fill pedestal is left underneath.
pub const STEEP_TARGET_PERCENTILE: f32 = 0.1;
/// Radius (cells) of the solid foundation skirt built around an `allow_steep`
/// building's footprint. On the steep, broken terrain mines land on, the regular
/// dirt blend ramp can't reach grade and the building perches on a thin pad; the
/// skirt batters a solid, ground-matched plinth down to natural grade instead.
pub const FOUNDATION_SKIRT_RADIUS: i32 = 6;
pub const YARD_RADIUS: i32 = 2;
pub const ROAD_SEARCH_RADIUS: i32 = 8;
/// When seeding urban industrial candidates, prefer interior cells within this
/// Chebyshev distance of a built road (`BuildClaim::Path`). A candidate seeded
/// here fronts a street *by construction*, rather than merely being nudged
/// toward one by `road_bonus`. Falls back to the full interior when no road is
/// near (roads not built yet, or a parcel with none routed through it).
pub const ROAD_SEED_RADIUS: i32 = 6;
/// Minimum distance (in cells) a placed building's footprint must keep from any
/// `BuildClaim::Wall` cell. Guarantees a visible gap between buildings and the city wall.
pub const WALL_BUFFER_RADIUS: i32 = 1;

pub const FLATNESS_WEIGHT: f32 = 2.0;
pub const WATER_WEIGHT: f32 = 1.5;
pub const EDGE_WEIGHT: f32 = 1.0;
pub const ROAD_WEIGHT: f32 = 1.0;

#[derive(Debug, Clone, Copy)]
pub struct Candidate {
    pub centre: Point2D,
    pub direction: Cardinal,
}

#[derive(Debug, Clone, Copy)]
pub struct CandidateScore {
    pub flatness: f32,
    /// Footprint height range (max minus min natural ground), in blocks. Used as
    /// the hard slope-reject metric; `flatness` (stddev) still feeds the score.
    pub slope: i32,
    pub water_margin: i32,
    pub edge_penalty: f32,
    pub road_bonus: f32,
    pub total: f32,
}

/// Computes the world-space footprint dimensions `(fw, fd)` for a structure
/// of size `(sx, sz)` after applying `rotation`.
pub fn footprint_dims_for_rotation(size: (i32, i32), rotation: Rotation) -> (i32, i32) {
    let (sx, sz) = size;
    match rotation {
        Rotation::None | Rotation::Twice => (sx, sz),
        Rotation::Once | Rotation::Thrice => (sz, sx),
    }
}

/// Computes the anchor offset `(dx, dz)` from the footprint rect's min corner
/// to the structure's origin point, for a structure of size `(sx, sz)` with
/// origin `(ox, oz)` after `rotation`.
pub fn anchor_offset_for_rotation(
    size: (i32, i32),
    origin_xz: (i32, i32),
    rotation: Rotation,
) -> (i32, i32) {
    let (sx, sz) = size;
    let (ox, oz) = origin_xz;
    match rotation {
        Rotation::None => (ox, oz),
        Rotation::Once => (sz - 1 - oz, ox),
        Rotation::Twice => (sx - 1 - ox, sz - 1 - oz),
        Rotation::Thrice => (oz, sx - 1 - ox),
    }
}

/// Computes the world-space footprint rectangle for a candidate placement.
pub fn footprint_rect(structure: &Structure, candidate: Candidate) -> Rect2D {
    let rotation =
        Rotation::from(candidate.direction) - Rotation::from(structure.facing);
    let (fw, fd) = footprint_dims_for_rotation(structure.size_xz, rotation);
    let (dx, dz) = anchor_offset_for_rotation(
        structure.size_xz,
        (structure.origin.x, structure.origin.z),
        rotation,
    );
    Rect2D {
        origin: Point2D::new(candidate.centre.x - dx, candidate.centre.y - dz),
        size: Point2D::new(fw, fd),
    }
}

/// Public entry point. Picks a spot inside `district`, prepares the ground,
/// places the structure, and claims the footprint.
///
/// Operates at the super-parcel level to match the resource chain's assignment
/// granularity (`SettlementProductionResult::parcel_assignments` is keyed by
/// `DistrictID`).
///
/// Returns `Ok(true)` only when a building was actually placed. The "couldn't
/// place" outcomes (invalid size, no interior, no viable candidate) return
/// `Ok(false)` so callers can tell a real placement from a skip — painting a
/// production area on a skip would attribute it to the previously placed
/// building (e.g. bees scattered with no apiary ever built).
pub async fn place_rural_building(
    district: &District,
    structure: &Structure,
    rng: &mut RNG,
    editor: &mut Editor,
    data: &LoadedData,
) -> Result<bool> {
    if structure.size_xz.0 <= 0 || structure.size_xz.1 <= 0 {
        warn!(
            "Structure '{}' has invalid size {:?}; skipping placement for super-parcel {:?}",
            structure.id.0, structure.size_xz, district.id
        );
        return Ok(false);
    }

    let edge_2d: HashSet<Point2D> =
        district.data.edges.iter().map(|p| p.drop_y()).collect();

    // Exclude cells inside the regularized wall footprint: a district can vote Rural
    // yet still own a few cells the closing pass pulled inside the wall. Water cells
    // are dropped here too: a building centre never belongs on water, and — since a
    // water surface is perfectly flat — leaving them in would let the flatness ranking
    // below rank them as the *best* candidates, only for `score_candidate` to reject
    // every one (water in footprint), starving placement near lakes/rivers.
    let interior: Vec<Point2D> = district
        .data
        .points_2d
        .iter()
        .filter(|p| {
            !edge_2d.contains(p) && !editor.world().is_urban(**p) && !editor.world().is_water(**p)
        })
        .copied()
        .collect();

    if interior.is_empty() {
        warn!(
            "Super-parcel {:?} has no interior cells for placement of '{}'",
            district.id, structure.id.0
        );
        return Ok(false);
    }

    let centres = flattest_candidate_centres(&interior, structure, editor, rng);

    let best = select_best_candidate(&centres, &district.data.points_2d, structure, editor, rng);
    let Some((candidate, score, rect)) = best else {
        warn!(
            "No viable placement for '{}' in super-parcel {:?}",
            structure.id.0, district.id
        );
        return Ok(false);
    };

    info!(
        "Placing '{}' in super-parcel {:?} at {:?} facing {:?} (score {:.2})",
        structure.id.0, district.id, candidate.centre, candidate.direction, score.total
    );

    if let Err(e) = execute_placement(candidate, rect, structure, editor, data, rng, None).await {
        warn!(
            "place_structure failed for '{}' in super-parcel {:?}: {}",
            structure.id.0, district.id, e
        );
        return Err(e);
    }
    Ok(true)
}

/// A rural resource building that physically placed, carrying everything needed
/// to (a) route the rural road network to it and (b) paint its production area.
///
/// Painting is **deferred**: the rural road network must be built between
/// placement and painting (so it can predict and reuse each area's `rural_road`
/// border ring), so `try_place_rural` no longer paints inline — the caller paints
/// via [`paint_production_area_for`](crate::generator::resource_chain::paint_production_area_for)
/// once the roads are down.
pub struct PlacedRural {
    pub district: DistrictID,
    pub structure: StructureID,
    /// Production painter name for this building, if any.
    pub painter: Option<String>,
    /// Resource resolved for the *placed* building (so a mine painter's ore
    /// matches an overridden building), used when painting the production area.
    pub resource: String,
    /// Whether this building's production painter will lay a `rural_road` border
    /// ring — the rural road network predicts and reuses it.
    pub has_border_ring: bool,
}

/// Places a rural resource building for `sd_id` and returns its [`PlacedRural`]
/// record on success (or `None` if the building couldn't be seated). Does **not**
/// paint the production area — see [`PlacedRural`].
pub async fn try_place_rural(
    sd_id: DistrictID,
    assignment: &ParcelResourceAssignment,
    data: &LoadedData,
    editor: &mut Editor,
    rng: &mut RNG,
) -> Option<PlacedRural> {
    let building = assignment.building.clone();
    let Some(district) = editor.world().districts.get(&sd_id).cloned() else { return None };
    let Some(structure) = data.structures.get(&StructureType(building.clone())).cloned() else {
        warn!("No structure for building '{}' (parcel {:?})", building, sd_id);
        return None;
    };
    match place_rural_building(&district, &structure, rng, editor, data).await {
        Ok(true) => {
            let Some(structure_id) = editor.world().structures.last().cloned() else {
                warn!("try_place_rural: placement reported success but pushed no structure for '{}'", building);
                return None;
            };
            // Resource for the *placed* building (so the mine painter's ore
            // matches an overridden building), falling back to the parcel's.
            let resource = data.resource_registry.recipes().values()
                .find(|r| r.inputs.is_empty() && r.building == building)
                .and_then(|r| r.outputs.keys().next().cloned())
                .unwrap_or_else(|| assignment.primary_resource.clone());
            let has_border_ring = assignment.production_painter.as_deref().map_or(false, |name| {
                data.resource_registry.production_painters.get(name).map_or(false, |p| p.paints_border())
            });
            Some(PlacedRural {
                district: sd_id,
                structure: structure_id,
                painter: assignment.production_painter.clone(),
                resource,
                has_border_ring,
            })
        }
        Ok(false) => None,
        Err(e) => {
            warn!("Rural placement failed for '{}' (parcel {:?}): {}", building, sd_id, e);
            None
        }
    }
}

/// Picks placement-candidate centres biased toward the flattest interior ground.
///
/// Uniform random sampling places `NUM_CANDIDATES` darts across the whole district;
/// in a large, rough forest district (where the apiary and other big production
/// buildings land) the few flat pockets that can actually hold the footprint are
/// rarely hit, and placement fails outright. Instead we rank every interior cell by
/// the flatness of the ground around it (over a window sized to the footprint),
/// keep the flattest `NUM_CANDIDATES * CANDIDATE_POOL_MULTIPLE`, and draw the
/// candidates from that pool — so we reliably consider the viable pads while
/// keeping randomness for spatial variety. `select_best_candidate` still fully
/// scores and slope-checks each, so this only changes *where we look*, not the
/// acceptance criteria.
fn flattest_candidate_centres(
    interior: &[Point2D],
    structure: &Structure,
    editor: &Editor,
    rng: &mut RNG,
) -> Vec<Point2D> {
    if interior.len() <= NUM_CANDIDATES {
        return interior.to_vec();
    }

    // Probe a window roughly half the footprint's larger side: a small height range
    // here means the full footprint has a real chance of fitting under the slope cap.
    let (sx, sz) = structure.size_xz;
    let probe_radius = (sx.max(sz) / 2).max(1);

    // Precompute non-tree surface heights once so the windowed range below doesn't
    // re-walk tree columns for every overlapping window.
    let height_at: HashMap<Point2D, i32> = interior
        .iter()
        .map(|&p| (p, editor.world().get_non_tree_height(p)))
        .collect();

    let mut ranked: Vec<Point2D> = interior.to_vec();
    ranked.sort_by_cached_key(|&c| local_height_range(c, probe_radius, &height_at));

    let pool_size = (NUM_CANDIDATES * CANDIDATE_POOL_MULTIPLE).min(ranked.len());
    rng.choose_many(&ranked[..pool_size], NUM_CANDIDATES)
        .into_iter()
        .copied()
        .collect()
}

/// Local surface-height range (max − min) over the `radius` window around `cell`,
/// read from the precomputed `height_at` map. Cells outside the map (district
/// edge / non-interior) are skipped; a cell whose window is mostly outside the
/// district is ranked worst (`i32::MAX`), since a large footprint can't fit there
/// anyway. Lower is flatter.
fn local_height_range(cell: Point2D, radius: i32, height_at: &HashMap<Point2D, i32>) -> i32 {
    let mut min = i32::MAX;
    let mut max = i32::MIN;
    let mut count = 0;
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            if let Some(&h) = height_at.get(&Point2D::new(cell.x + dx, cell.y + dz)) {
                min = min.min(h);
                max = max.max(h);
                count += 1;
            }
        }
    }
    let window = (2 * radius + 1) * (2 * radius + 1);
    if (count * 2) < window {
        return i32::MAX; // too close to the district edge to seat the footprint
    }
    max - min
}

/// Cheap feasibility probe used by the resource chain *before* it locks the
/// settlement's economy: for a rural `district`, which of the given building
/// `footprints` can actually be seated under `MAX_PLACEMENT_SLOPE`?
///
/// Each footprint is `(size_x, size_z, allow_steep)`. The check mirrors the
/// interior + slope gating in `place_rural_building`/`select_best_candidate`:
/// a footprint is seatable if some interior cell's footprint-sized window stays
/// within the slope cap (the same `local_height_range` window the candidate
/// ranker uses). `allow_steep` footprints (mines) bypass the cap and only need a
/// non-empty interior. Returns one bool per input footprint, in order.
///
/// This is intentionally permissive: it answers "could this ever seat here?" so a
/// resource is only excluded from a parcel when *no* viable pad exists — placement
/// still does the exact per-candidate slope check. Sharing one interior height
/// scan across all footprints keeps it to a single pass per district.
pub fn district_seatable_footprints(
    district: &District,
    editor: &Editor,
    footprints: &[(i32, i32, bool)],
) -> Vec<bool> {
    let edge_2d: HashSet<Point2D> =
        district.data.edges.iter().map(|p| p.drop_y()).collect();

    // Same interior as `place_rural_building`: drop edge, urban and water cells.
    let interior: Vec<Point2D> = district
        .data
        .points_2d
        .iter()
        .filter(|p| {
            !edge_2d.contains(p) && !editor.world().is_urban(**p) && !editor.world().is_water(**p)
        })
        .copied()
        .collect();

    if interior.is_empty() {
        return vec![false; footprints.len()];
    }

    // Precompute non-tree surface heights once, reused for every footprint's window.
    let height_at: HashMap<Point2D, i32> = interior
        .iter()
        .map(|&p| (p, editor.world().get_non_tree_height(p)))
        .collect();

    footprints
        .iter()
        .map(|&(sx, sz, allow_steep)| {
            if sx <= 0 || sz <= 0 {
                return false;
            }
            if allow_steep {
                return true; // slope cap bypassed; interior is non-empty here
            }
            let probe_radius = (sx.max(sz) / 2).max(1);
            interior
                .iter()
                .any(|&c| local_height_range(c, probe_radius, &height_at) <= MAX_PLACEMENT_SLOPE)
        })
        .collect()
}

/// Resolve the settlement's rural economy with placement feasibility folded in.
///
/// Wraps [`ResourceRegistry::resolve_for_parcels_seated`]: it derives each gather
/// resource's building footprint from `data`, asks
/// [`district_seatable_footprints`] which footprints every rural district can
/// actually seat under the slope cap, and feeds that constraint into the resolver
/// so the plan never assigns a building a parcel can't physically hold. Consumers
/// (and tests) with a live `Editor` call this instead of `resolve_for_parcels` and
/// get the seatability handling for free.
pub fn resolve_rural_production(
    data: &LoadedData,
    editor: &Editor,
    rural_analysis: &HashMap<DistrictID, ParcelAnalysis>,
    rng: &mut RNG,
) -> SettlementProductionResult {
    // Map each gather resource to its building footprint (size + steep tolerance).
    let mut gather_footprints: HashMap<String, (i32, i32, bool)> = HashMap::new();
    for recipe in data.resource_registry.recipes().values() {
        if !recipe.inputs.is_empty() {
            continue; // only gather (no-input) recipes seat a rural building
        }
        let Some(structure) = data.structures.get(&StructureType(recipe.building.clone())) else {
            continue;
        };
        let fp = (structure.size_xz.0, structure.size_xz.1, structure.allow_steep);
        for resource in recipe.outputs.keys() {
            gather_footprints.insert(resource.clone(), fp);
        }
    }

    // Per rural district, the subset of gather resources whose footprint fits.
    let resources: Vec<String> = gather_footprints.keys().cloned().collect();
    let footprints: Vec<(i32, i32, bool)> = resources.iter().map(|r| gather_footprints[r]).collect();
    let seatable: HashMap<DistrictID, HashSet<String>> = rural_analysis
        .keys()
        .filter_map(|id| {
            let district = editor.world().districts.get(id)?.clone();
            let fits = district_seatable_footprints(&district, editor, &footprints);
            let set: HashSet<String> = resources
                .iter()
                .zip(fits)
                .filter_map(|(r, ok)| ok.then(|| r.clone()))
                .collect();
            Some((*id, set))
        })
        .collect();

    data.resource_registry.resolve_for_parcels_seated(rural_analysis, Some(&seatable), rng)
}

/// Places a single processing/secondary building somewhere in the urban region.
///
/// The urban region is the union of all urban super-parcels' footprints — there is
/// no fixed mapping of building type to a specific urban super-parcel, so we treat
/// the whole area as one candidate pool. Picks 10 random interior centres × 4 cardinals,
/// scores by flatness + water/edge proximity + road proximity (same scorer as
/// `place_rural_building`), then places the best.
pub async fn place_urban_building(
    urban_districts: &[&District],
    structure: &Structure,
    rng: &mut RNG,
    editor: &mut Editor,
    data: &LoadedData,
    palette: Option<&Palette>,
) -> Result<()> {
    if structure.size_xz.0 <= 0 || structure.size_xz.1 <= 0 {
        warn!(
            "Structure '{}' has invalid size {:?}; skipping urban placement",
            structure.id.0, structure.size_xz
        );
        return Ok(());
    }
    if urban_districts.is_empty() {
        warn!("No urban super-parcels available; skipping placement of '{}'", structure.id.0);
        return Ok(());
    }

    // Restrict to cells actually inside the regularized wall footprint: a district can
    // vote Urban yet still own a few cells the opening pass trimmed outside the wall.
    let urban_points: HashSet<Point2D> = urban_districts
        .iter()
        .flat_map(|sd| sd.data.points_2d.iter().copied())
        .filter(|p| editor.world().is_urban(*p))
        .collect();
    let urban_edges: HashSet<Point2D> = urban_districts
        .iter()
        .flat_map(|sd| sd.data.edges.iter().map(|p| p.drop_y()))
        .collect();

    let interior: Vec<Point2D> = urban_points
        .iter()
        .filter(|p| !urban_edges.contains(p))
        .copied()
        .collect();

    if interior.is_empty() {
        warn!("No urban interior cells for placement of '{}'", structure.id.0);
        return Ok(());
    }

    // Seed candidates from road-adjacent cells *and* the general interior. The
    // road-adjacent ones win when viable (their `road_bonus` discount dominates a
    // flattened site's score), so industrial buildings front the streets; the
    // interior ones are a fallback so placement never starves when every
    // road-adjacent footprint collides with the pavement, wall buffer, or edge.
    let road_adjacent = road_adjacent_centres(&interior, editor, ROAD_SEED_RADIUS);
    let mut centres: Vec<Point2D> = rng
        .choose_many(&road_adjacent, NUM_CANDIDATES)
        .into_iter()
        .copied()
        .collect();
    centres.extend(
        rng.choose_many(&interior, NUM_CANDIDATES)
            .into_iter()
            .copied(),
    );

    let best = select_best_candidate(&centres, &urban_points, structure, editor, rng);
    let Some((candidate, score, rect)) = best else {
        warn!("No viable urban placement for '{}'", structure.id.0);
        return Ok(());
    };

    info!(
        "Placing urban '{}' at {:?} facing {:?} (score {:.2})",
        structure.id.0, candidate.centre, candidate.direction, score.total
    );

    if let Err(e) = execute_placement(candidate, rect, structure, editor, data, rng, palette).await {
        warn!("place_structure failed for urban '{}': {}", structure.id.0, e);
        return Err(e);
    }
    Ok(())
}

/// Places every processing building in `building_counts` (building id -> count) into
/// the urban region. Buildings are visited one-by-one in a random order — each placement
/// claims its footprint, so subsequent placements steer around what's already been built.
pub async fn place_urban_buildings(
    urban_districts: &[&District],
    building_counts: &std::collections::HashMap<String, u32>,
    rng: &mut RNG,
    editor: &mut Editor,
    data: &LoadedData,
    // Output palette to re-skin each structure into (e.g. desert sandstone). The
    // structure's own `palette` field is the input; blocks are swapped role-for-
    // role into this one. `None` places the structures with their baked blocks.
    palette: Option<&Palette>,
) -> Result<()> {
    // Flatten counts into a single Vec<String> with multiplicity, then pop randomly
    // until empty. Sort for deterministic ordering before randomising.
    let mut queue: Vec<String> = Vec::new();
    let mut sorted: Vec<(&String, &u32)> = building_counts.iter().collect();
    sorted.sort_by_key(|(name, _)| name.as_str());
    for (name, count) in sorted {
        for _ in 0..*count {
            queue.push(name.clone());
        }
    }

    while let Some(building) = rng.pop(&mut queue) {
        let structure_type = crate::generator::nbts::StructureType(building.clone());
        let Some(structure) = data.structures.get(&structure_type).cloned() else {
            warn!("No structure found for processing building '{}'", building);
            continue;
        };

        if let Err(e) =
            place_urban_building(urban_districts, &structure, rng, editor, data, palette).await
        {
            warn!("Urban placement failed for '{}': {}", building, e);
        }
    }

    Ok(())
}

/// Scans candidate `(centre, direction)` pairs against `points_2d` (the bounds the
/// footprint must lie within), rejecting overlapping claims and water, and returns
/// the lowest-scoring candidate with its rect.
///
/// For each centre the preferred facing direction is the one pointing toward the
/// nearest road cell within `ROAD_SEARCH_RADIUS`. When no road is found a random
/// cardinal is chosen instead. The three remaining directions act as fallbacks in
/// case the preferred one produces an invalid footprint.
fn select_best_candidate(
    centres: &[Point2D],
    points_2d: &HashSet<Point2D>,
    structure: &Structure,
    editor: &Editor,
    rng: &mut RNG,
) -> Option<(Candidate, CandidateScore, Rect2D)> {
    const ALL_CARDINALS: [Cardinal; 4] =
        [Cardinal::North, Cardinal::East, Cardinal::South, Cardinal::West];

    let mut best: Option<(Candidate, CandidateScore, Rect2D)> = None;
    for centre in centres {
        let preferred = nearest_road_direction(*centre, ROAD_SEARCH_RADIUS, editor)
            .unwrap_or_else(|| *rng.choose(&ALL_CARDINALS));

        // Try preferred direction first, then the three fallbacks in clockwise order.
        let directions = [
            preferred,
            preferred.rotate_right(),
            preferred.opposite(),
            preferred.rotate_left(),
        ];

        for direction in directions {
            let candidate = Candidate { centre: *centre, direction };
            let rect = footprint_rect(structure, candidate);

            if !rect_inside_points(&rect, points_2d) {
                continue;
            }
            if rect_overlaps_claim(&rect, editor) {
                continue;
            }
            if rect_too_close_to_wall(&rect, editor, WALL_BUFFER_RADIUS) {
                continue;
            }
            let Some(score) = score_candidate(&rect, editor) else {
                continue; // hard reject (water inside footprint)
            };
            // Hard reject footprints too steep to flatten cleanly, unless the
            // structure opts into steep ground (e.g. mines).
            if !structure.allow_steep && score.slope > MAX_PLACEMENT_SLOPE {
                continue;
            }

            match &best {
                None => best = Some((candidate, score, rect)),
                Some((_, prev, _)) if score.total < prev.total => {
                    best = Some((candidate, score, rect))
                }
                _ => {}
            }
            // Use the first valid direction for this centre (preferred direction wins).
            break;
        }
    }
    best
}

/// Interior cells lying within `radius` (Chebyshev) of a claimed road
/// (`BuildClaim::Path`) — the prime sites for urban industrial buildings, which
/// want to front a street. Returns empty when no road is near (e.g. roads not
/// built yet), so callers can fall back to the full interior.
fn road_adjacent_centres(interior: &[Point2D], editor: &Editor, radius: i32) -> Vec<Point2D> {
    let world = editor.world();
    interior
        .iter()
        .filter(|&&p| {
            for dx in -radius..=radius {
                for dz in -radius..=radius {
                    let q = Point2D::new(p.x + dx, p.y + dz);
                    if world.is_in_bounds_2d(q)
                        && matches!(world.get_claim(q), Some(BuildClaim::Path(_)))
                    {
                        return true;
                    }
                }
            }
            false
        })
        .copied()
        .collect()
}

/// Returns the `Cardinal` direction from `centre` toward the nearest road cell
/// within `radius` (Manhattan distance). Returns `None` when no road is found.
fn nearest_road_direction(centre: Point2D, radius: i32, editor: &Editor) -> Option<Cardinal> {
    let world = editor.world();
    let mut nearest_dist = i32::MAX;
    let mut nearest_road: Option<Point2D> = None;

    for x in (centre.x - radius)..=(centre.x + radius) {
        for z in (centre.y - radius)..=(centre.y + radius) {
            let p = Point2D::new(x, z);
            if !world.is_in_bounds_2d(p) {
                continue;
            }
            if matches!(world.get_claim(p), Some(BuildClaim::Path(_))) {
                let dist = (x - centre.x).abs() + (z - centre.y).abs();
                if dist < nearest_dist {
                    nearest_dist = dist;
                    nearest_road = Some(p);
                }
            }
        }
    }

    let road = nearest_road?;
    let dx = road.x - centre.x;
    let dz = road.y - centre.y;

    // Snap the direction vector to the dominant cardinal axis.
    if dx.abs() >= dz.abs() {
        if dx >= 0 { Some(Cardinal::East) } else { Some(Cardinal::West) }
    } else {
        if dz >= 0 { Some(Cardinal::South) } else { Some(Cardinal::North) }
    }
}

/// Clears vegetation, flattens the footprint with a tapered blend ring, places the NBT,
/// and claims the footprint cells. Shared by rural and urban placement.
async fn execute_placement(
    candidate: Candidate,
    rect: Rect2D,
    structure: &Structure,
    editor: &mut Editor,
    data: &LoadedData,
    rng: &mut RNG,
    palette: Option<&Palette>,
) -> Result<()> {
    let footprint_cells: Vec<Point2D> = rect.iter().collect();

    // Step 4a — clear vegetation in the footprint plus a yard margin.
    let yard: HashSet<Point2D> = expanded_rect_cells(&rect, YARD_RADIUS)
        .into_iter()
        .filter(|p| editor.world().is_in_bounds_2d(*p))
        .collect();
    log_trees(editor, yard).await;

    // Step 4b — flatten the footprint and taper the blend ring. Steep-tolerant
    // buildings (mines) cut into the hillside instead of perching on fill.
    let target_y = footprint_target_height(&footprint_cells, editor, structure.allow_steep);
    let inner_points: HashSet<Point3D> = footprint_cells
        .iter()
        .map(|p| Point3D::new(p.x, target_y, p.y))
        .collect();
    force_height(editor, &inner_points, false).await;

    if structure.allow_steep {
        // Steep, broken sites (mines): the dirt blend ramp can't reach grade across
        // a badlands drop, leaving the building perched on a pad. A solid,
        // ground-matched skirt batters down to natural grade so it reads as a plinth
        // cut into the slope.
        build_foundation_skirt(editor, &rect, target_y).await;
    } else {
        let blend_points = build_blend_ring(&rect, target_y, editor);
        if !blend_points.is_empty() {
            force_height(editor, &blend_points, true).await;
        }
    }

    // Step 5 — place the NBT. With an output palette, build a placer so the
    // structure's blocks are swapped role-for-role from its baked input palette
    // into the requested one (e.g. desert sandstone); without one, place as-is.
    let anchor_y = target_y + structure.y_offset;
    let offset = Point3D::new(candidate.centre.x, anchor_y, candidate.centre.y);
    let mut placer_rng = rng.derive();
    let mut placer = palette.map(|_| Placer::new(&data.materials, &mut placer_rng));
    place_structure(
        editor,
        placer.as_mut(),
        structure,
        offset,
        candidate.direction,
        Some(data),
        palette,
        false,
        false,
    )
    .await?;

    // Step 6 — mint a unique instance id, record it on the world, and claim
    // the footprint cells. The blend ring is intentionally not claimed.
    let instance_id = StructureID {
        id: editor.world().structures.len() as u32,
        structure_type: structure.id.clone(),
    };
    editor.world_mut().structures.push(instance_id.clone());

    let claim = BuildClaim::Structure(instance_id);
    for cell in &footprint_cells {
        editor.world_mut().claim(*cell, claim.clone());
    }

    Ok(())
}

fn rect_inside_points(rect: &Rect2D, points: &HashSet<Point2D>) -> bool {
    rect.iter().all(|p| points.contains(&p))
}

fn rect_overlaps_claim(rect: &Rect2D, editor: &Editor) -> bool {
    rect.iter().any(|p| {
        editor.world().is_in_bounds_2d(p) && editor.world().is_claimed(p)
    })
}

/// Rejects candidates whose footprint sits within `buffer` cells of any
/// `BuildClaim::Wall` cell. Guarantees breathing room between the city wall
/// (and its walkway / inner-wall / tower bases, which are also claimed as `Wall`)
/// and any placed building.
fn rect_too_close_to_wall(rect: &Rect2D, editor: &Editor, buffer: i32) -> bool {
    if buffer < 0 {
        return false;
    }
    let world = editor.world();
    for p in expanded_rect_cells(rect, buffer) {
        if !world.is_in_bounds_2d(p) {
            continue;
        }
        if matches!(world.get_claim(p), Some(BuildClaim::Wall)) {
            return true;
        }
    }
    false
}

/// Score a candidate footprint. Returns `None` when the footprint should be
/// hard-rejected: a water cell inside it, or a footprint mostly surrounded by
/// water (a small island/spit — see `MAX_WATER_SURROUND_FRACTION`).
pub fn score_candidate(rect: &Rect2D, editor: &Editor) -> Option<CandidateScore> {
    let world = editor.world();

    let mut heights: Vec<i32> = Vec::with_capacity(rect.area() as usize);
    for p in rect.iter() {
        if world.is_water(p) {
            return None;
        }
        heights.push(world.get_non_tree_height(p));
    }

    let mean = heights.iter().sum::<i32>() as f32 / heights.len() as f32;
    let variance =
        heights.iter().map(|h| (*h as f32 - mean).powi(2)).sum::<f32>() / heights.len() as f32;
    let flatness = variance.sqrt();
    let slope = heights.iter().copied().max().unwrap_or(0)
        - heights.iter().copied().min().unwrap_or(0);

    let mut water_margin = 0;
    let mut ring_cells = 0;
    for p in expanded_rect_cells(rect, WATER_MARGIN_RADIUS) {
        if rect.contains(p) || !world.is_in_bounds_2d(p) {
            continue;
        }
        ring_cells += 1;
        if world.is_water(p) {
            water_margin += 1;
        }
    }

    // Hard reject sites that are mostly surrounded by water (small islands/spits):
    // the footprint itself is dry land, but the building reads as sitting on the
    // water. A normal shoreline (water on ~one side) stays under the threshold.
    if ring_cells > 0 && (water_margin as f32 / ring_cells as f32) > MAX_WATER_SURROUND_FRACTION {
        return None;
    }

    let edge_penalty = edge_proximity_penalty(rect, editor);
    let road_bonus = road_proximity_bonus(rect, editor);

    let total = FLATNESS_WEIGHT * flatness
        + WATER_WEIGHT * water_margin as f32
        + EDGE_WEIGHT * edge_penalty
        + ROAD_WEIGHT * road_bonus;

    Some(CandidateScore {
        flatness,
        slope,
        water_margin,
        edge_penalty,
        road_bonus,
        total,
    })
}

fn edge_proximity_penalty(rect: &Rect2D, editor: &Editor) -> f32 {
    // The cheapest proxy for "near a parcel edge" is "near the world edge or
    // near a non-claimable cell". We approximate by scanning outward up to
    // ROAD_SEARCH_RADIUS+BLEND_RADIUS for an out-of-bounds cell.
    let world = editor.world();
    let max_search = (BLEND_RADIUS + WATER_MARGIN_RADIUS) as i32;
    let mut min_dist = i32::MAX;
    for p in expanded_rect_cells(rect, max_search) {
        if !world.is_in_bounds_2d(p) {
            let dist = manhattan_distance_to_rect(rect, p);
            if dist < min_dist {
                min_dist = dist;
            }
        }
    }
    if min_dist == i32::MAX {
        0.0
    } else {
        1.0 / (1.0 + min_dist as f32)
    }
}

fn road_proximity_bonus(rect: &Rect2D, editor: &Editor) -> f32 {
    let world = editor.world();
    let mut nearest: Option<i32> = None;
    for p in expanded_rect_cells(rect, ROAD_SEARCH_RADIUS) {
        if rect.contains(p) {
            continue;
        }
        if !world.is_in_bounds_2d(p) {
            continue;
        }
        if matches!(world.get_claim(p), Some(BuildClaim::Path(_))) {
            let dist = manhattan_distance_to_rect(rect, p);
            nearest = Some(nearest.map_or(dist, |d| d.min(dist)));
        }
    }
    match nearest {
        Some(d) if d <= ROAD_SEARCH_RADIUS => -((ROAD_SEARCH_RADIUS - d) as f32),
        _ => 0.0,
    }
}

fn manhattan_distance_to_rect(rect: &Rect2D, p: Point2D) -> i32 {
    let min = rect.min();
    let max = rect.max();
    let dx = if p.x < min.x {
        min.x - p.x
    } else if p.x > max.x {
        p.x - max.x
    } else {
        0
    };
    let dy = if p.y < min.y {
        min.y - p.y
    } else if p.y > max.y {
        p.y - max.y
    } else {
        0
    };
    dx + dy
}

fn expanded_rect_cells(rect: &Rect2D, radius: i32) -> Vec<Point2D> {
    let min = rect.min();
    let max = rect.max();
    let mut out = Vec::new();
    for x in (min.x - radius)..=(max.x + radius) {
        for z in (min.y - radius)..=(max.y + radius) {
            out.push(Point2D::new(x, z));
        }
    }
    out
}

/// Height the footprint is flattened to. Normal buildings use the median
/// (balanced cut/fill — fine within `MAX_PLACEMENT_SLOPE`). `allow_steep`
/// buildings sit on much larger slopes, where the median would bury the uphill
/// side and perch the downhill side on a tall fill pedestal (the "floating mine"
/// look); they instead target `STEEP_TARGET_PERCENTILE` so the pad is cut into the
/// hill and the downhill edge meets near-natural grade. The low percentile (not a
/// strict min) ignores the odd outlier-low cell — e.g. a ravine or cave mouth in
/// the footprint — so one deep cell can't drag the whole pad down.
fn footprint_target_height(cells: &[Point2D], editor: &Editor, allow_steep: bool) -> i32 {
    let mut heights: Vec<i32> = cells
        .iter()
        .map(|p| editor.world().get_non_tree_height(*p))
        .collect();
    heights.sort_unstable();
    let percentile = if allow_steep { STEEP_TARGET_PERCENTILE } else { 0.5 };
    let idx = (((heights.len() - 1) as f32) * percentile).round() as usize;
    heights[idx]
}

fn build_blend_ring(rect: &Rect2D, target_y: i32, editor: &Editor) -> HashSet<Point3D> {
    let mut out: HashSet<Point3D> = HashSet::new();
    let world = editor.world();
    for p in expanded_rect_cells(rect, BLEND_RADIUS) {
        if rect.contains(p) {
            continue;
        }
        if !world.is_in_bounds_2d(p) {
            continue;
        }
        let dist = manhattan_distance_to_rect(rect, p);
        if dist == 0 || dist > BLEND_RADIUS {
            continue;
        }
        let natural_y = world.get_non_tree_height(p);
        // Always grade toward natural terrain — no early bail on steep deltas, so
        // the pad edge ramps down instead of leaving a cliff. The footprint slope
        // is already bounded by MAX_PLACEMENT_SLOPE (except allow_steep buildings,
        // which accept the larger earthworks), so the ramp stays reasonable.
        let t = dist as f32 / BLEND_RADIUS as f32;
        let blended = (target_y as f32 * (1.0 - t) + natural_y as f32 * t).round() as i32;
        out.insert(Point3D::new(p.x, blended, p.y));
    }
    out
}

/// Picks the foundation/skirt material: the most common *natural* surface block
/// over the footprint (`ground_block_map`, untouched by our terraforming), so the
/// skirt reads as the local rock — terracotta in badlands, sand in desert, stone on
/// a rocky hill — rather than a dirt scar. Falls back to stone.
fn sample_foundation_material(rect: &Rect2D, editor: &Editor) -> Block {
    let world = editor.world();
    let mut counts: HashMap<String, u32> = HashMap::new();
    for p in rect.iter() {
        if !world.is_in_bounds_2d(p) {
            continue;
        }
        *counts.entry(world.get_ground_block(p).id.as_str().to_string()).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(id, _)| Block::from_id(id.as_str().into()))
        .unwrap_or_else(|| Block::from_id("minecraft:stone".into()))
}

/// Builds a solid foundation skirt around an `allow_steep` footprint: a battered
/// plinth of the local rock that descends from the pad edge to natural grade,
/// replacing the dirt blend ramp. On the steep, broken terrain mines sit on, this
/// is what keeps the building grounded instead of perched on a thin pad.
///
/// Per apron cell (Manhattan ring `1..=FOUNDATION_SKIRT_RADIUS` around the
/// footprint) the skirt top is interpolated from the pad height down to that cell's
/// natural grade. Where the ground is *below* the skirt top (downhill) we fill a
/// solid column up to it; where it's *above* (uphill) we cut down to it, so the pad
/// is neither perched nor walled in. The heightmap is updated to match.
async fn build_foundation_skirt(editor: &mut Editor, rect: &Rect2D, target_y: i32) {
    let material = sample_foundation_material(rect, editor);

    // Snapshot (cell, natural_y, skirt_top) before mutating any heights.
    let mut plan: Vec<(Point2D, i32, i32)> = Vec::new();
    {
        let world = editor.world();
        for p in expanded_rect_cells(rect, FOUNDATION_SKIRT_RADIUS) {
            if rect.contains(p) || !world.is_in_bounds_2d(p) {
                continue;
            }
            let dist = manhattan_distance_to_rect(rect, p);
            if dist == 0 || dist > FOUNDATION_SKIRT_RADIUS {
                continue;
            }
            let natural_y = world.get_non_tree_height(p);
            // Taper from the pad height (just outside the wall) to natural grade at
            // the skirt's outer edge.
            let t = dist as f32 / (FOUNDATION_SKIRT_RADIUS as f32 + 1.0);
            let skirt_top = (target_y as f32 * (1.0 - t) + natural_y as f32 * t).round() as i32;
            plan.push((p, natural_y, skirt_top));
        }
    }

    let mut new_heights: HashSet<Point3D> = HashSet::new();
    for (p, natural_y, skirt_top) in plan {
        if skirt_top > natural_y {
            // Downhill: raise a solid material plinth from grade up to the skirt top.
            for y in natural_y..skirt_top {
                editor.place_block_forced(&material, Point3D::new(p.x, y, p.y)).await;
            }
        } else if skirt_top < natural_y {
            // Uphill: cut the ground down to the skirt top so the pad isn't walled in.
            for y in skirt_top..natural_y {
                editor.place_block_forced(&"air".into(), Point3D::new(p.x, y, p.y)).await;
            }
        }
        // Cap the new surface (top solid sits at skirt_top - 1) with the material.
        editor.place_block_forced(&material, Point3D::new(p.x, skirt_top - 1, p.y)).await;
        new_heights.insert(Point3D::new(p.x, skirt_top, p.y));
    }
    editor.world_mut().set_heights(&new_heights);
}
