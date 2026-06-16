use std::collections::{HashMap, HashSet};

use log::info;

use crate::editor::{Editor, World};

use super::analysis::analyze_parcel;
use super::{constants::{TARGET_PARCEL_AMOUNT, ADJACENCY_WEIGHT, PARCEL_SIZE_LOWER_FACTOR, PARCEL_SIZE_UPPER_FACTOR}, ParcelAnalysis, District, DistrictID};
use super::{Parcel, ParcelID, HasParcelData};


/// Block count (surface cells) of a super-parcel — the metric the size band is defined in.
fn block_size(sd : &District) -> usize {
    sd.data().points_2d.len()
}

/// Merge the ~1-parcel-per-super-parcel soup down into balanced interior parcels.
///
/// Approach B (size-band driven, see docs/plans/parcel_size_balancing.md): instead of merging
/// blindly until a fixed *count* remains, we target an average interior size
/// `S = interior_blocks / TARGET_PARCEL_AMOUNT` and repeatedly merge every interior parcel that
/// is below the band floor `L = PARCEL_SIZE_LOWER_FACTOR * S` up into a neighbour, never pushing a
/// parent above the ceiling `U = PARCEL_SIZE_UPPER_FACTOR * S`. Off-limits (border) parcels are
/// exempt from the ceiling — they only get coalesced up to `L` so we don't leave tiny fragments.
pub async fn merge_down(districts : &mut HashMap<DistrictID, District>, parcels : &HashMap<ParcelID, Parcel>, parcel_analysis_data : &mut HashMap<DistrictID, ParcelAnalysis>, editor : &mut Editor) {
    // Target average size S is derived from the interior (non-border) mass only, so off-limits
    // regions don't distort the band. The band is then [L, U] around S.
    let interior_blocks : usize = districts.values()
        .filter(|sd| !sd.data().is_border())
        .map(|sd| block_size(sd))
        .sum();
    let interior_count = districts.values().filter(|sd| !sd.data().is_border()).count();

    if interior_blocks == 0 || interior_count == 0 {
        info!("No interior super-parcels to balance, skipping merge.");
        return;
    }

    let target = (interior_blocks / TARGET_PARCEL_AMOUNT as usize).max(1);
    let lower = ((target as f32) * PARCEL_SIZE_LOWER_FACTOR) as usize;
    let upper = ((target as f32) * PARCEL_SIZE_UPPER_FACTOR).max(1.0) as usize;
    info!(
        "Merge size band: interior_blocks={}, interior_count={}, target S={}, band [L={}, U={}]",
        interior_blocks, interior_count, target, lower, upper
    );

    // Parcels we've given up on (genuinely isolated below-L pockets). Permanently skipped so the
    // loop always makes progress and terminates.
    let mut ignore : HashSet<DistrictID> = HashSet::new();

    // Keep merging while any non-ignored parcel is below the floor. Every merge removes a below-L
    // child and only ever grows the parent, so the below-L set shrinks monotonically.
    loop {
        let child = districts.iter()
            .filter(|(id, sd)| !ignore.contains(id) && block_size(sd) < lower)
            .min_by_key(|(_, sd)| block_size(sd))
            .map(|(id, _)| *id);

        let Some(child) = child else {
            info!("No parcels below the size floor remain, merge complete.");
            break;
        };

        let child_sd = districts.get(&child).expect("child super-parcel not found");
        let child_border = child_sd.data().is_border();
        let child_blocks = block_size(child_sd);
        let neighbours : Vec<DistrictID> = child_sd.parcel_adjacency().keys().cloned().collect();

        // Interior children obey the ceiling; border children have no ceiling (off-limits may be any size).
        let cap = if child_border { None } else { Some(upper) };

        let parent = pick_balanced_parent(districts, parcel_analysis_data, child, &neighbours, child_blocks, cap)
            // Starvation: no same-type neighbour fits under the ceiling. Merge into the smallest
            // same-type neighbour instead (combine two smalls), accepting a parcel slightly over U
            // rather than stranding one below L.
            .or_else(|| {
                neighbours.iter()
                    .filter(|n| districts.get(n).map_or(false, |s| s.data().is_border() == child_border))
                    .min_by_key(|n| block_size(districts.get(n).expect("neighbour not found")))
                    .cloned()
            });

        let Some(parent) = parent else {
            // Truly isolated below-L parcel with no same-type neighbour. Drop tiny garbage from the
            // map; otherwise leave it as-is (exempt) and stop reconsidering it.
            info!("No mergeable neighbour for child {} ({} cells), leaving it.", child.0, child_blocks);
            if child_blocks < 10 {
                remove_parcel(districts, child, editor.world_mut());
            }
            ignore.insert(child);
            continue;
        };

        merge(districts, parcels, parcel_analysis_data, parent, child, editor).await;
    }
}


fn remove_parcel(parcels : &mut HashMap<DistrictID, District>, parcel_id : DistrictID, world : &mut World) {
    for point in parcels.get(&parcel_id).expect(&format!("District with id {} not found", parcel_id.0)).points_2d().iter() {
        world.district_map[point.x as usize][point.y as usize] = None;
    }
    
    parcels.remove(&parcel_id);
}


async fn merge(districts : &mut HashMap<DistrictID, District>, parcels : &HashMap<ParcelID, Parcel>, parcel_analysis_data : &mut HashMap<DistrictID, ParcelAnalysis>, parent : DistrictID, child : DistrictID, editor : &mut Editor) {
    let child = districts.remove(&child).expect(&format!("District with id {} not found", child.0));

    let parcel_ids = districts.keys().map(|id| id.clone()).collect::<Vec<DistrictID>>();
    for id in parcel_ids.into_iter().filter(|id| *id != parent) {
        // Replace the child parcel with the parent parcel in the adjacency map
        let parcel = districts.get_mut(&id).expect(&format!("District with id {} not found", id.0));
        let amount = parcel.data.parcel_adjacency.remove(&child.id()).unwrap_or(0);
        if amount > 0 as u32 {
            *parcel.data.parcel_adjacency.entry(parent).or_insert(0) += amount;
        }
    }
    let parent = districts.get_mut(&parent).expect(&format!("District with id {} not found", parent.0));
    parent.add_district(&child, parcels, editor.world_mut());
    let new_analysis = analyze_parcel(parent.data(), editor).await;
    parcel_analysis_data.insert(parent.id(), new_analysis);
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
    districts : &HashMap<DistrictID, District>,
    parcel_analysis_data : &HashMap<DistrictID, ParcelAnalysis>,
    child : DistrictID,
    neighbours : &[DistrictID],
    child_blocks : usize,
    cap : Option<usize>,
) -> Option<DistrictID> {
    let child_sd = districts.get(&child).expect("child super-parcel not found");
    let child_border = child_sd.data().is_border();
    let child_perimeter = child_sd.adjacencies_count();

    neighbours.iter()
        .filter_map(|other| {
            let other_sd = districts.get(other)?;
            // Only merge like with like (keeps off-limits border regions out of interior parcels).
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
                *child_sd.parcel_adjacency().get(other).unwrap_or(&0) as f32 / child_perimeter as f32
            };
            let score = get_candidate_score(parcel_analysis_data, child, *other, Some(adjacency_ratio), ADJACENCY_WEIGHT);
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
/// `weight` is how heavily the adjacency term counts relative to the five terrain terms — district
/// merge passes `ADJACENCY_WEIGHT`, city growth passes the larger `CITY_GROWTH_ADJACENCY_WEIGHT` so
/// compactness dominates. Ignored when `adjacency_ratio` is `None`.
pub fn get_candidate_score(parcel_analysis_data : &HashMap<DistrictID, ParcelAnalysis>, target : DistrictID, candidate : DistrictID, adjacency_ratio : Option<f32>, weight : f32) -> f32 {
    candidate_score_terms(parcel_analysis_data, target, candidate, adjacency_ratio, weight).total()
}

/// Per-term decomposition of [`get_candidate_score`], for diagnostics. `total()` reproduces the
/// score exactly, so logging this keeps the breakdown in lock-step with the scoring formula.
pub struct ScoreTerms {
    /// Raw adjacency ratio in `[0,1]` (0 when the adjacency term is disabled).
    pub adjacency: f32,
    pub biome: f32,
    pub water: f32,
    pub forest: f32,
    pub gradient: f32,
    pub roughness: f32,
    /// Adjacency weight applied in the total: `ADJACENCY_WEIGHT` when enabled, else `0`.
    pub weight: f32,
}

impl ScoreTerms {
    /// The five terrain-similarity terms (each in `[0,1]`), summed.
    pub fn terrain_sum(&self) -> f32 {
        self.biome + self.water + self.forest + self.gradient + self.roughness
    }

    /// The final normalized candidate score — identical to [`get_candidate_score`].
    pub fn total(&self) -> f32 {
        (self.adjacency * self.weight + self.terrain_sum()) / (5.0 + self.weight)
    }
}

/// Compute the [`ScoreTerms`] breakdown for `candidate` joining/merging next to `target`.
/// See [`get_candidate_score`] for the meaning of `adjacency_ratio` and `weight`.
pub fn candidate_score_terms(parcel_analysis_data : &HashMap<DistrictID, ParcelAnalysis>, target : DistrictID, candidate : DistrictID, adjacency_ratio : Option<f32>, weight : f32) -> ScoreTerms {
    let target_analysis = parcel_analysis_data.get(&target).expect("Could not find parcel analysis data for target");
    let candidate_analysis = parcel_analysis_data.get(&candidate).expect("Could not find parcel analysis data for candidate");

    let use_adjacency = adjacency_ratio.is_some();

    let biome_score : f32 = 1.0 - target_analysis.biome_count().iter()
        .map(|(biome, _)| {
            (target_analysis.biome_percentage(biome) - candidate_analysis.biome_percentage(biome)).abs()
        })
        .sum::<f32>() / target_analysis.biome_count().len() as f32;

    let water_score = 1.0 - (target_analysis.water_percentage() - candidate_analysis.water_percentage()).abs();
    let forest_score = 1.0 - (target_analysis.forested_percentage() - candidate_analysis.forested_percentage()).abs();
    let gradient_score = 1.0 - (target_analysis.gradient() - candidate_analysis.gradient()).abs();
    let roughness_score = 1.0 - (target_analysis.roughness() - candidate_analysis.roughness()).abs();

    ScoreTerms {
        adjacency: adjacency_ratio.unwrap_or(0.0),
        biome: biome_score,
        water: water_score,
        forest: forest_score,
        gradient: gradient_score,
        roughness: roughness_score,
        weight: if use_adjacency { weight } else { 0.0 },
    }
}

/// Calculates the similarity score between two parcels based on their analysis data, same as above but for parcels instead of super parcel and adjacency removed
pub fn parcel_similarity_score(parcel_analysis_data : &HashMap<ParcelID, ParcelAnalysis>, target : ParcelID, candidate : ParcelID) -> f32 {
    let target_analysis = parcel_analysis_data.get(&target).expect("Could not find parcel analysis data for target");
    let candidate_analysis = parcel_analysis_data.get(&candidate).expect("Could not find parcel analysis data for candidate");

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