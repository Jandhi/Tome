
use super::{
    District,
    DistrictID,
    DistrictAnalysis,
    SuperDistrict,
    SuperDistrictID,
    district::DistrictType,
    constants::{OFF_LIMITS_ROUGHNESS, OFF_LIMITS_GRADIENT, URBAN_WATER_LIMIT, URBAN_RELATIVE_TO_PRIME,
        URBAN_SIZE_MIN, URBAN_SIZE_MAX, URBAN_GROWTH_CUTOFF, URBAN_GROWTH_CUTOFF_HIGH,
        URBAN_OPTION_SCORE_MAX, RURAL_OPTION_SCORE_MAX},
    merge::{get_candidate_score, district_similarity_score},
    data::HasDistrictData
};


use std::collections::{HashMap, HashSet};
use log::info;

pub fn classify_districts<'a>(districts: & mut HashMap<DistrictID, District>, district_analysis_data: &HashMap<DistrictID, DistrictAnalysis>){
    
    let mut options: Vec<DistrictID> = Vec::new(); // Placeholder for options to choose from

    for (id, district) in districts.iter_mut() {
        let analysis_data = district_analysis_data.get(id).expect("District analysis data not found");

        if district.data.is_border() || analysis_data.roughness() > OFF_LIMITS_ROUGHNESS || analysis_data.gradient() > OFF_LIMITS_GRADIENT {
            district.data.district_type = DistrictType::OffLimits;
            info!("District {:?} is off-limits due to roughness or gradient", id);
            continue
        }
        info!("District {:?} has data {:?}", id, analysis_data);
        if analysis_data.water_percentage() <= URBAN_WATER_LIMIT && district.data.district_type == DistrictType::Unknown {
            options.push(*id);
        } else if analysis_data.water_percentage() > URBAN_WATER_LIMIT && district.data.district_type == DistrictType::Unknown{
            district.data.district_type = DistrictType::Rural;
        }
    }
    info!("Options for prime urban district: {:?}", options);

    let Some(prime_urban_district) = select_prime_urban_district(options, district_analysis_data) else {
        log::warn!("No prime urban candidate found"); //will mean no other districts are classified beyond this point
        return;
    };
    info!("Prime urban district selected: {:?}", prime_urban_district);

    if let Some(district) = districts.get_mut(&prime_urban_district) {
        district.data.district_type = DistrictType::Urban;
    } else {
        panic!("District not found");
    }
    let district_ids = districts.keys().map(|id| id.clone()).collect::<Vec<DistrictID>>();
    for id in district_ids.iter() {
        let district = districts.get_mut(id).expect("District not found");
        if district.data.district_type == DistrictType::Unknown {
            let score = district_similarity_score(district_analysis_data, prime_urban_district, *id);
            if score > URBAN_RELATIVE_TO_PRIME {
                district.data.district_type = DistrictType::Urban;
                info!("District {:?} classified as Urban with score {}", id, score);
            } else {
                district.data.district_type = DistrictType::Rural;
                info!("District {:?} classified as Off-Limits with score {}", id, score);
            }
        }
        
    }
    
}

pub fn classify_superdistricts<'a>(superdistricts: &mut HashMap<SuperDistrictID, SuperDistrict>, districts: &mut HashMap<DistrictID, District>, district_analysis_data: &HashMap<SuperDistrictID, DistrictAnalysis>) {
    // This function will classify superdistricts based on their districts

    let mut options: Vec<SuperDistrictID> = Vec::new(); // Placeholder for options to choose from

    for (id, superdistrict) in superdistricts.iter_mut() {

        if superdistrict.data.is_border() {
            superdistrict.data.district_type = DistrictType::OffLimits;
            info!("Superdistrict {:?} is off-limits due to being border", id);
            continue
        }
        let score = superdistrict_score(superdistrict, districts);
        if score <= URBAN_OPTION_SCORE_MAX {
            info!("Superdistrict {:?} classified as Urban option with score {}", id, score);
            options.push(*id);
        } else if score <= RURAL_OPTION_SCORE_MAX {
            superdistrict.data.district_type = DistrictType::Rural;
            info!("Superdistrict {:?} classified as Rural with score {}", id, score);
        } else {
            superdistrict.data.district_type = DistrictType::OffLimits;
            info!("Superdistrict {:?} classified as Off-Limits with score {}", id, score);
        }
    }

    info!("Options for prime urban district: {:?}", options);

    // Primes ordered best-first. Try each in turn: if a prime cannot grow a city of at least
    // URBAN_SIZE_MIN from qualifying neighbours, fall back to the next-best prime.
    let primes = rank_prime_urban_superdistricts(options, district_analysis_data);

    let mut city: Option<Vec<SuperDistrictID>> = None;
    for prime in primes.iter() {
        if let Some(grown) = try_grow_city(*prime, superdistricts, district_analysis_data) {
            info!("Prime {:?} grew a city of size {}", prime, grown.len());
            city = Some(grown);
            break;
        }
        info!("Prime {:?} could not reach minimum city size, trying next-best prime", prime);
    }

    // Final fallback: no prime could reach the minimum, so commit the best prime alone
    // (whatever it could anchor) rather than producing no city at all.
    let city = city.or_else(|| {
        primes.first().map(|p| {
            info!("No prime reached URBAN_SIZE_MIN; committing best prime {:?} alone", p);
            vec![*p]
        })
    });

    let Some(city) = city else {
        log::warn!("No prime urban candidate found");
        return;
    };

    for id in city {
        superdistricts.get_mut(&id).expect("SuperDistrict not found").data.district_type = DistrictType::Urban;
    }

    // classify remaining superdistricts as rural if they are unknown
    for district in superdistricts.values_mut() {
        if district.data.district_type == DistrictType::Unknown {
            district.data.district_type = DistrictType::Rural;
        }
    }


}

/// Attempt to grow a city anchored at `prime` without mutating any classification.
///
/// Greedily adds the best adjacent unclassified superdistrict (by `get_candidate_score`)
/// as long as it clears the relevant cutoff: the normal cutoff while below URBAN_SIZE_MIN,
/// a higher cutoff to keep growing up to URBAN_SIZE_MAX. Returns the chosen set only if it
/// reaches URBAN_SIZE_MIN, otherwise `None` so the caller can fall back to another prime.
fn try_grow_city(
    prime: SuperDistrictID,
    superdistricts: &HashMap<SuperDistrictID, SuperDistrict>,
    district_analysis_data: &HashMap<SuperDistrictID, DistrictAnalysis>,
) -> Option<Vec<SuperDistrictID>> {
    let mut urban: Vec<SuperDistrictID> = vec![prime];
    let mut urban_set: HashSet<SuperDistrictID> = HashSet::from([prime]);

    while (urban.len() as u32) < URBAN_SIZE_MAX {
        // Gather candidate neighbours of the current (tentative) urban set that are still unclassified.
        let mut candidates: Vec<SuperDistrictID> = Vec::new();
        for id in urban.iter() {
            let neighbours = superdistricts.get(id).expect("SuperDistrict not found").data().district_adjacency.keys()
                .filter(|&&neighbour_id| {
                    !urban_set.contains(&neighbour_id)
                        && superdistricts.get(&neighbour_id).expect("SuperDistrict not found").data().district_type == DistrictType::Unknown
                })
                .cloned()
                .collect::<Vec<SuperDistrictID>>();
            candidates.extend(neighbours);
        }

        let best = candidates.iter()
            .map(|&id| {
                let score = get_candidate_score(superdistricts, district_analysis_data, prime, id, true);
                info!("Candidate {:?} has score {}", id, score);
                (id, score)
            })
            .max_by(|(_, score1), (_, score2)| score1.partial_cmp(score2).expect("We should be able to compare scores"));

        let Some((candidate, score)) = best else {
            info!("No more candidates reachable from prime {:?}, stopping growth", prime);
            break;
        };

        // Below the minimum we use the normal cutoff; beyond it a higher bar is required to keep growing.
        let cutoff = if (urban.len() as u32) < URBAN_SIZE_MIN {
            URBAN_GROWTH_CUTOFF
        } else {
            URBAN_GROWTH_CUTOFF_HIGH
        };

        if score < cutoff {
            info!("Best candidate {:?} (score {}) below cutoff {}, stopping growth", candidate, score, cutoff);
            break;
        }

        urban.push(candidate);
        urban_set.insert(candidate);
    }

    if (urban.len() as u32) >= URBAN_SIZE_MIN {
        Some(urban)
    } else {
        None
    }
}

fn select_prime_urban_district(options: Vec<DistrictID>, district_analysis_data: &HashMap<DistrictID, DistrictAnalysis>) -> Option<DistrictID> {
    options.iter()
        .map(|&id| {
            let analysis_data = district_analysis_data.get(&id).expect("District analysis data not found");
            let score = urban_district_score(analysis_data);
            (id, score)
        })
        .min_by(|(_, score1), (_, score2)| score1.partial_cmp(score2).expect("We should be able to compare scores"))
        .map(|(other, _score)| {
            info!("Best candidate is {:?}", other);
            other
        })
}

/// Rank urban candidates best-first (lower `urban_district_score` is better) so the caller
/// can fall back from the prime to the next-best candidate when growth fails.
fn rank_prime_urban_superdistricts(options: Vec<SuperDistrictID>, district_analysis_data: &HashMap<SuperDistrictID, DistrictAnalysis>) -> Vec<SuperDistrictID> {
    let mut scored: Vec<(SuperDistrictID, f32)> = options.iter()
        .map(|&id| {
            let analysis_data = district_analysis_data.get(&id).expect("District analysis data not found");
            (id, urban_district_score(analysis_data))
        })
        .collect();
    scored.sort_by(|(_, score1), (_, score2)| score1.partial_cmp(score2).expect("We should be able to compare scores"));
    info!("Prime urban candidates ranked best-first: {:?}", scored);
    scored.into_iter().map(|(id, _)| id).collect()
}

fn urban_district_score(analysis_data: &DistrictAnalysis) -> f32 {
    // Calculate a score based on the analysis data for urban districts
    let water_score = analysis_data.water_percentage();
    let forest_score = analysis_data.forested_percentage();
    let gradient_score = analysis_data.gradient() / 3.0;
    let roughness_score = analysis_data.roughness();

    water_score + forest_score + gradient_score + roughness_score
}

fn superdistrict_score(superdistrict: &SuperDistrict, districts: &mut HashMap<DistrictID, District>) -> f32 {
    let sub_types = superdistrict.get_subtypes(districts);
    return sub_types.iter()
        .map(|(district_type, count)| {
            match district_type {
                DistrictType::Urban => *count as f32 * 0.0,
                DistrictType::Rural => *count as f32 * 1.0,
                DistrictType::OffLimits => *count as f32 * 2.0,
                _ => 2.0,
            }
        })
        .sum::<f32>() / superdistrict.districts().len() as f32
}