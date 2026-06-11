use std::collections::{HashMap, HashSet};

use log::info;

use crate::editor::{Editor, World};

use super::analysis::analyze_district;
use super::{constants::{TARGET_DISTRICT_AMOUNT, ADJACENCY_WEIGHT, DISTRICT_SIZE_LOWER_FACTOR, DISTRICT_SIZE_UPPER_FACTOR}, DistrictAnalysis, SuperDistrict, SuperDistrictID};
use super::{District, DistrictID, HasDistrictData};


/// Block count (surface cells) of a super-district — the metric the size band is defined in.
fn block_size(sd : &SuperDistrict) -> usize {
    sd.data().points_2d.len()
}

/// Merge the ~1-district-per-super-district soup down into balanced interior districts.
///
/// Approach B (size-band driven, see docs/plans/district_size_balancing.md): instead of merging
/// blindly until a fixed *count* remains, we target an average interior size
/// `S = interior_blocks / TARGET_DISTRICT_AMOUNT` and repeatedly merge every interior district that
/// is below the band floor `L = DISTRICT_SIZE_LOWER_FACTOR * S` up into a neighbour, never pushing a
/// parent above the ceiling `U = DISTRICT_SIZE_UPPER_FACTOR * S`. Off-limits (border) districts are
/// exempt from the ceiling — they only get coalesced up to `L` so we don't leave tiny fragments.
pub async fn merge_down(superdistricts : &mut HashMap<SuperDistrictID, SuperDistrict>, districts : &HashMap<DistrictID, District>, district_analysis_data : &mut HashMap<SuperDistrictID, DistrictAnalysis>, editor : &mut Editor) {
    // Target average size S is derived from the interior (non-border) mass only, so off-limits
    // regions don't distort the band. The band is then [L, U] around S.
    let interior_blocks : usize = superdistricts.values()
        .filter(|sd| !sd.data().is_border())
        .map(|sd| block_size(sd))
        .sum();
    let interior_count = superdistricts.values().filter(|sd| !sd.data().is_border()).count();

    if interior_blocks == 0 || interior_count == 0 {
        info!("No interior super-districts to balance, skipping merge.");
        return;
    }

    let target = (interior_blocks / TARGET_DISTRICT_AMOUNT as usize).max(1);
    let lower = ((target as f32) * DISTRICT_SIZE_LOWER_FACTOR) as usize;
    let upper = ((target as f32) * DISTRICT_SIZE_UPPER_FACTOR).max(1.0) as usize;
    info!(
        "Merge size band: interior_blocks={}, interior_count={}, target S={}, band [L={}, U={}]",
        interior_blocks, interior_count, target, lower, upper
    );

    // Districts we've given up on (genuinely isolated below-L pockets). Permanently skipped so the
    // loop always makes progress and terminates.
    let mut ignore : HashSet<SuperDistrictID> = HashSet::new();

    // Keep merging while any non-ignored district is below the floor. Every merge removes a below-L
    // child and only ever grows the parent, so the below-L set shrinks monotonically.
    loop {
        let child = superdistricts.iter()
            .filter(|(id, sd)| !ignore.contains(id) && block_size(sd) < lower)
            .min_by_key(|(_, sd)| block_size(sd))
            .map(|(id, _)| *id);

        let Some(child) = child else {
            info!("No districts below the size floor remain, merge complete.");
            break;
        };

        let child_sd = superdistricts.get(&child).expect("child super-district not found");
        let child_border = child_sd.data().is_border();
        let child_blocks = block_size(child_sd);
        let neighbours : Vec<SuperDistrictID> = child_sd.district_adjacency().keys().cloned().collect();

        // Interior children obey the ceiling; border children have no ceiling (off-limits may be any size).
        let cap = if child_border { None } else { Some(upper) };

        let parent = pick_balanced_parent(superdistricts, district_analysis_data, child, &neighbours, child_blocks, cap)
            // Starvation: no same-type neighbour fits under the ceiling. Merge into the smallest
            // same-type neighbour instead (combine two smalls), accepting a district slightly over U
            // rather than stranding one below L.
            .or_else(|| {
                neighbours.iter()
                    .filter(|n| superdistricts.get(n).map_or(false, |s| s.data().is_border() == child_border))
                    .min_by_key(|n| block_size(superdistricts.get(n).expect("neighbour not found")))
                    .cloned()
            });

        let Some(parent) = parent else {
            // Truly isolated below-L district with no same-type neighbour. Drop tiny garbage from the
            // map; otherwise leave it as-is (exempt) and stop reconsidering it.
            info!("No mergeable neighbour for child {} ({} cells), leaving it.", child.0, child_blocks);
            if child_blocks < 10 {
                remove_district(superdistricts, child, editor.world_mut());
            }
            ignore.insert(child);
            continue;
        };

        merge(superdistricts, districts, district_analysis_data, parent, child, editor).await;
    }
}


fn remove_district(districts : &mut HashMap<SuperDistrictID, SuperDistrict>, district_id : SuperDistrictID, world : &mut World) {
    for point in districts.get(&district_id).expect(&format!("Superdistrict with id {} not found", district_id.0)).points_2d().iter() {
        world.super_district_map[point.x as usize][point.y as usize] = None;
    }
    
    districts.remove(&district_id);
}


async fn merge(superdistricts : &mut HashMap<SuperDistrictID, SuperDistrict>, districts : &HashMap<DistrictID, District>, district_analysis_data : &mut HashMap<SuperDistrictID, DistrictAnalysis>, parent : SuperDistrictID, child : SuperDistrictID, editor : &mut Editor) {
    let child = superdistricts.remove(&child).expect(&format!("Superdistrict with id {} not found", child.0));

    let district_ids = superdistricts.keys().map(|id| id.clone()).collect::<Vec<SuperDistrictID>>();
    for id in district_ids.into_iter().filter(|id| *id != parent) {
        // Replace the child district with the parent district in the adjacency map
        let district = superdistricts.get_mut(&id).expect(&format!("Superdistrict with id {} not found", id.0));
        let amount = district.data.district_adjacency.remove(&child.id()).unwrap_or(0);
        if amount > 0 as u32 {
            *district.data.district_adjacency.entry(parent).or_insert(0) += amount;
        }
    }
    let parent = superdistricts.get_mut(&parent).expect(&format!("Superdistrict with id {} not found", parent.0));
    parent.add_superdistrict(&child, districts, editor.world_mut());
    let new_analysis = analyze_district(parent.data(), editor).await;
    district_analysis_data.insert(parent.id(), new_analysis);
}

/// Choose the best parent to merge `child` into, respecting the size band.
///
/// Candidates are restricted to same-border-type neighbours (interior↔interior, border↔border) that
/// still in the map. When `cap` is `Some(U)`, any neighbour whose size plus `child_blocks` would
/// exceed `U` is rejected — this is what prevents the rich-get-richer blobs. Among the surviving
/// (in-band) candidates the highest `get_candidate_score` wins, so terrain/adjacency similarity is
/// now a *tiebreaker for where to send the child*, not a hard gate that could strand it.
/// Returns `None` if no in-cap same-type neighbour exists (caller then handles starvation).
fn pick_balanced_parent(
    superdistricts : &HashMap<SuperDistrictID, SuperDistrict>,
    district_analysis_data : &HashMap<SuperDistrictID, DistrictAnalysis>,
    child : SuperDistrictID,
    neighbours : &[SuperDistrictID],
    child_blocks : usize,
    cap : Option<usize>,
) -> Option<SuperDistrictID> {
    let child_sd = superdistricts.get(&child).expect("child super-district not found");
    let child_border = child_sd.data().is_border();
    let child_perimeter = child_sd.adjacencies_count();

    neighbours.iter()
        .filter_map(|other| {
            let other_sd = superdistricts.get(other)?;
            // Only merge like with like (keeps off-limits border regions out of interior districts).
            if other_sd.data().is_border() != child_border {
                return None;
            }
            // Respect the ceiling for capped (interior) merges.
            if let Some(cap) = cap {
                if block_size(other_sd) + child_blocks > cap {
                    return None;
                }
            }
            // Merge keeps the original child-centric adjacency: fraction of the child's perimeter facing this parent.
            let adjacency_ratio = if child_perimeter == 0 {
                0.0
            } else {
                *child_sd.district_adjacency().get(other).unwrap_or(&0) as f32 / child_perimeter as f32
            };
            let score = get_candidate_score(district_analysis_data, child, *other, Some(adjacency_ratio));
            Some((*other, score))
        })
        .max_by(|(_, score1), (_, score2)| score1.partial_cmp(score2).expect("We should be able to compare scores"))
        .map(|(other, _score)| other)
}

/// Score `candidate` for joining/merging next to `target`.
///
/// Terrain similarity (biome/water/forest/gradient/roughness) is always measured against `target`.
/// The adjacency term is supplied by the caller via `adjacency_ratio` rather than computed here, so
/// the *adjacency reference* can differ from the *terrain reference*:
/// - the merge phase passes the child→candidate perimeter fraction (target == the merging child);
/// - city growth passes the fraction of the candidate's perimeter touching the whole urban set, so
///   compactness is rewarded relative to the growing city, not just the prime anchor.
///
/// `adjacency_ratio` is an already-normalized `[0,1]` fraction; `None` disables the adjacency term.
pub fn get_candidate_score(district_analysis_data : &HashMap<SuperDistrictID, DistrictAnalysis>, target : SuperDistrictID, candidate : SuperDistrictID, adjacency_ratio : Option<f32>) -> f32 {
    let target_analysis = district_analysis_data.get(&target).expect("Could not find district analysis data for target");
    let candidate_analysis = district_analysis_data.get(&candidate).expect("Could not find district analysis data for candidate");

    let use_adjacency = adjacency_ratio.is_some();
    let adjacency_score = adjacency_ratio.unwrap_or(0.0);

    let biome_score : f32 = 1.0 - target_analysis.biome_count().iter()
        .map(|(biome, _)| {
            (target_analysis.biome_percentage(biome) - candidate_analysis.biome_percentage(biome)).abs()
        })
        .sum::<f32>() / target_analysis.biome_count().len() as f32;
    
    let water_score = 1.0 - (target_analysis.water_percentage() - candidate_analysis.water_percentage()).abs();
    let forest_score = 1.0 - (target_analysis.forested_percentage() - candidate_analysis.forested_percentage()).abs();
    let gradient_score = 1.0 - (target_analysis.gradient() - candidate_analysis.gradient()).abs();
    let roughness_score = 1.0 - (target_analysis.roughness() - candidate_analysis.roughness()).abs();

    return (adjacency_score * ADJACENCY_WEIGHT
        + biome_score
        + water_score
        + forest_score
        + gradient_score
        + roughness_score) / (5.0 + if use_adjacency { ADJACENCY_WEIGHT } else { 0.0 })
}

/// Calculates the similarity score between two districts based on their analysis data, same as above but for districts instead of super district and adjacency removed
pub fn district_similarity_score(district_analysis_data : &HashMap<DistrictID, DistrictAnalysis>, target : DistrictID, candidate : DistrictID) -> f32 {
    let target_analysis = district_analysis_data.get(&target).expect("Could not find district analysis data for target");
    let candidate_analysis = district_analysis_data.get(&candidate).expect("Could not find district analysis data for candidate");

    let biome_score : f32 = 1.0 - target_analysis.biome_count().iter()
        .map(|(biome, _)| {
            (target_analysis.biome_percentage(biome) - candidate_analysis.biome_percentage(biome)).abs()
        })
        .sum::<f32>() / target_analysis.biome_count().len() as f32;
    
    let water_score = 1.0 - (target_analysis.water_percentage() - candidate_analysis.water_percentage()).abs();
    let forest_score = 1.0 - (target_analysis.forested_percentage() - candidate_analysis.forested_percentage()).abs();
    let gradient_score = 1.0 - (target_analysis.gradient() - candidate_analysis.gradient()).abs();
    let roughness_score = 1.0 - (target_analysis.roughness() - candidate_analysis.roughness()).abs();

    return ( biome_score
        + water_score
        + forest_score
        + gradient_score
        + roughness_score) / 5.0
}