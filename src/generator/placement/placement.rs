use std::collections::HashSet;

use anyhow::Result;
use log::{info, warn};
use strum::IntoEnumIterator;

use crate::{
    editor::Editor,
    generator::{
        BuildClaim,
        data::LoadedData,
        districts::District,
        nbts::{Rotation, Structure, place_structure},
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
pub async fn place_resource_building(
    district: &District,
    structure: &Structure,
    rng: &mut RNG,
    editor: &mut Editor,
    data: &LoadedData,
) -> Result<()> {
    if structure.size_xz.0 <= 0 || structure.size_xz.1 <= 0 {
        warn!(
            "Structure '{}' has invalid size {:?}; skipping placement for district {:?}",
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
            "District {:?} has no interior cells for placement of '{}'",
            district.id, structure.id.0
        );
        return Ok(());
    }

    let centres: Vec<Point2D> = rng
        .choose_many(&interior, NUM_CANDIDATES)
        .into_iter()
        .copied()
        .collect();

    let mut best: Option<(Candidate, CandidateScore, Rect2D)> = None;
    for centre in centres {
        for direction in Cardinal::iter() {
            let candidate = Candidate { centre, direction };
            let rect = footprint_rect(structure, candidate);

            if !rect_inside_points(&rect, &district.data.points_2d) {
                continue;
            }
            if rect_overlaps_claim(&rect, editor) {
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
        }
    }

    let Some((candidate, score, rect)) = best else {
        warn!(
            "No viable placement for '{}' in district {:?}",
            structure.id.0, district.id
        );
        return Ok(());
    };

    info!(
        "Placing '{}' in district {:?} at {:?} facing {:?} (score {:.2})",
        structure.id.0, district.id, candidate.centre, candidate.direction, score.total
    );

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
    let mut anchor_y = target_y + 1;
    if !structure.has_subgrade {
        anchor_y -= 1;
    }
    let offset = Point3D::new(candidate.centre.x, anchor_y, candidate.centre.y);
    if let Err(e) = place_structure(
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
    .await
    {
        warn!(
            "place_structure failed for '{}' in district {:?}: {}",
            structure.id.0, district.id, e
        );
        return Err(e);
    }

    // Step 6 — claim the footprint cells (blend ring is intentionally not claimed).
    let claim = BuildClaim::Structure(structure.id.clone());
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
    // The cheapest proxy for "near a district edge" is "near the world edge or
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
