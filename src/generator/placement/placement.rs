use std::collections::HashSet;

use anyhow::Result;
use log::{info, warn};

use crate::{
    editor::Editor,
    generator::{
        BuildClaim,
        data::LoadedData,
        parcels::District,
        nbts::{Rotation, Structure, StructureID, place_structure},
        terrain::{force_height, log_trees},
    },
    geometry::{Cardinal, Point2D, Point3D, Rect2D},
    noise::RNG,
};

pub const NUM_CANDIDATES: usize = 10;
pub const WATER_MARGIN_RADIUS: i32 = 4;
pub const BLEND_RADIUS: i32 = 4;
pub const MAX_BLEND_DELTA: i32 = 4;
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
/// places the structure, and claims the footprint. Returns `Ok(())` whether
/// a placement happened or the function bailed out due to no viable site —
/// the failure case is logged but not error-propagated.
///
/// Operates at the super-parcel level to match the resource chain's assignment
/// granularity (`SettlementProductionResult::parcel_assignments` is keyed by
/// `DistrictID`).
pub async fn place_rural_building(
    district: &District,
    structure: &Structure,
    rng: &mut RNG,
    editor: &mut Editor,
    data: &LoadedData,
) -> Result<()> {
    if structure.size_xz.0 <= 0 || structure.size_xz.1 <= 0 {
        warn!(
            "Structure '{}' has invalid size {:?}; skipping placement for super-parcel {:?}",
            structure.id.0, structure.size_xz, district.id
        );
        return Ok(());
    }

    let edge_2d: HashSet<Point2D> =
        district.data.edges.iter().map(|p| p.drop_y()).collect();

    let interior: Vec<Point2D> = district
        .data
        .points_2d
        .iter()
        .filter(|p| !edge_2d.contains(p))
        .copied()
        .collect();

    if interior.is_empty() {
        warn!(
            "Super-parcel {:?} has no interior cells for placement of '{}'",
            district.id, structure.id.0
        );
        return Ok(());
    }

    let centres: Vec<Point2D> = rng
        .choose_many(&interior, NUM_CANDIDATES)
        .into_iter()
        .copied()
        .collect();

    let best = select_best_candidate(&centres, &district.data.points_2d, structure, editor, rng);
    let Some((candidate, score, rect)) = best else {
        warn!(
            "No viable placement for '{}' in super-parcel {:?}",
            structure.id.0, district.id
        );
        return Ok(());
    };

    info!(
        "Placing '{}' in super-parcel {:?} at {:?} facing {:?} (score {:.2})",
        structure.id.0, district.id, candidate.centre, candidate.direction, score.total
    );

    if let Err(e) = execute_placement(candidate, rect, structure, editor, data).await {
        warn!(
            "place_structure failed for '{}' in super-parcel {:?}: {}",
            structure.id.0, district.id, e
        );
        return Err(e);
    }
    Ok(())
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

    let urban_points: HashSet<Point2D> = urban_districts
        .iter()
        .flat_map(|sd| sd.data.points_2d.iter().copied())
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

    if let Err(e) = execute_placement(candidate, rect, structure, editor, data).await {
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
            place_urban_building(urban_districts, &structure, rng, editor, data).await
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
) -> Result<()> {
    let footprint_cells: Vec<Point2D> = rect.iter().collect();

    // Step 4a — clear vegetation in the footprint plus a yard margin.
    let yard: HashSet<Point2D> = expanded_rect_cells(&rect, YARD_RADIUS)
        .into_iter()
        .filter(|p| editor.world().is_in_bounds_2d(*p))
        .collect();
    log_trees(editor, yard).await;

    // Step 4b — flatten the footprint and taper the blend ring.
    let target_y = median_height(&footprint_cells, editor);
    let inner_points: HashSet<Point3D> = footprint_cells
        .iter()
        .map(|p| Point3D::new(p.x, target_y, p.y))
        .collect();
    force_height(editor, &inner_points, false).await;

    let blend_points = build_blend_ring(&rect, target_y, editor);
    if !blend_points.is_empty() {
        force_height(editor, &blend_points, true).await;
    }

    // Step 5 — place the NBT.
    let anchor_y = target_y + structure.y_offset;
    let offset = Point3D::new(candidate.centre.x, anchor_y, candidate.centre.y);
    place_structure(
        editor,
        None,
        structure,
        offset,
        candidate.direction,
        Some(data),
        None,
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
/// hard-rejected (water cell inside it).
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

    let mut water_margin = 0;
    for p in expanded_rect_cells(rect, WATER_MARGIN_RADIUS) {
        if rect.contains(p) {
            continue;
        }
        if world.is_in_bounds_2d(p) && world.is_water(p) {
            water_margin += 1;
        }
    }

    let edge_penalty = edge_proximity_penalty(rect, editor);
    let road_bonus = road_proximity_bonus(rect, editor);

    let total = FLATNESS_WEIGHT * flatness
        + WATER_WEIGHT * water_margin as f32
        + EDGE_WEIGHT * edge_penalty
        + ROAD_WEIGHT * road_bonus;

    Some(CandidateScore {
        flatness,
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

fn median_height(cells: &[Point2D], editor: &Editor) -> i32 {
    let mut heights: Vec<i32> = cells
        .iter()
        .map(|p| editor.world().get_non_tree_height(*p))
        .collect();
    heights.sort_unstable();
    heights[heights.len() / 2]
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
        if (natural_y - target_y).abs() > MAX_BLEND_DELTA {
            continue;
        }
        let t = dist as f32 / BLEND_RADIUS as f32;
        let blended = (target_y as f32 * (1.0 - t) + natural_y as f32 * t).round() as i32;
        out.insert(Point3D::new(p.x, blended, p.y));
    }
    out
}
