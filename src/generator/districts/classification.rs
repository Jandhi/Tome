
use super::{
    Parcel,
    ParcelID,
    ParcelAnalysis,
    District,
    DistrictID,
    parcel::ParcelType,
    constants::{OFF_LIMITS_ROUGHNESS, OFF_LIMITS_GRADIENT, URBAN_WATER_LIMIT, URBAN_RELATIVE_TO_PRIME,
        URBAN_SIZE_MIN, URBAN_SIZE_MAX, URBAN_GROWTH_CUTOFF, URBAN_GROWTH_CUTOFF_HIGH,
        URBAN_OPTION_SCORE_MAX, RURAL_OPTION_SCORE_MAX},
    merge::{get_candidate_score, parcel_similarity_score},
    data::HasParcelData
};


use std::collections::{HashMap, HashSet};
use log::info;

pub fn classify_parcels<'a>(parcels: & mut HashMap<ParcelID, Parcel>, parcel_analysis_data: &HashMap<ParcelID, ParcelAnalysis>){
    
    let mut options: Vec<ParcelID> = Vec::new(); // Placeholder for options to choose from

    for (id, parcel) in parcels.iter_mut() {
        let analysis_data = parcel_analysis_data.get(id).expect("Parcel analysis data not found");

        if parcel.data.is_border() || analysis_data.roughness() > OFF_LIMITS_ROUGHNESS || analysis_data.gradient() > OFF_LIMITS_GRADIENT {
            parcel.data.parcel_type = ParcelType::OffLimits;
            info!("Parcel {:?} is off-limits due to roughness or gradient", id);
            continue
        }
        info!("Parcel {:?} has data {:?}", id, analysis_data);
        if analysis_data.water_percentage() <= URBAN_WATER_LIMIT && parcel.data.parcel_type == ParcelType::Unknown {
            options.push(*id);
        } else if analysis_data.water_percentage() > URBAN_WATER_LIMIT && parcel.data.parcel_type == ParcelType::Unknown{
            parcel.data.parcel_type = ParcelType::Rural;
        }
    }
    info!("Options for prime urban parcel: {:?}", options);

    let Some(prime_urban_parcel) = select_prime_urban_parcel(options, parcel_analysis_data) else {
        log::warn!("No prime urban candidate found"); //will mean no other parcels are classified beyond this point
        return;
    };
    info!("Prime urban parcel selected: {:?}", prime_urban_parcel);

    if let Some(parcel) = parcels.get_mut(&prime_urban_parcel) {
        parcel.data.parcel_type = ParcelType::Urban;
    } else {
        panic!("Parcel not found");
    }
    let parcel_ids = parcels.keys().map(|id| id.clone()).collect::<Vec<ParcelID>>();
    for id in parcel_ids.iter() {
        let parcel = parcels.get_mut(id).expect("Parcel not found");
        if parcel.data.parcel_type == ParcelType::Unknown {
            let score = parcel_similarity_score(parcel_analysis_data, prime_urban_parcel, *id);
            if score > URBAN_RELATIVE_TO_PRIME {
                parcel.data.parcel_type = ParcelType::Urban;
                info!("Parcel {:?} classified as Urban with score {}", id, score);
            } else {
                parcel.data.parcel_type = ParcelType::Rural;
                info!("Parcel {:?} classified as Off-Limits with score {}", id, score);
            }
        }
        
    }
    
}

pub fn classify_districts<'a>(districts: &mut HashMap<DistrictID, District>, parcels: &mut HashMap<ParcelID, Parcel>, parcel_analysis_data: &HashMap<DistrictID, ParcelAnalysis>) {
    // This function will classify districts based on their parcels

    let mut options: Vec<DistrictID> = Vec::new(); // Placeholder for options to choose from

    for (id, district) in districts.iter_mut() {

        // Size of this district: blocks = surface cells it covers, parcels = constituent parcels merged into it.
        let blocks = district.data.points_2d.len();
        let n_parcels = district.parcels.len();

        if district.data.is_border() {
            district.data.parcel_type = ParcelType::OffLimits;
            info!("District {:?} is off-limits due to being border ({} blocks, {} parcels)", id, blocks, n_parcels);
            continue
        }
        let score = district_score(district, parcels);
        if score <= URBAN_OPTION_SCORE_MAX {
            info!("District {:?} classified as Urban option with score {} ({} blocks, {} parcels)", id, score, blocks, n_parcels);
            options.push(*id);
        } else if score <= RURAL_OPTION_SCORE_MAX {
            district.data.parcel_type = ParcelType::Rural;
            info!("District {:?} classified as Rural with score {} ({} blocks, {} parcels)", id, score, blocks, n_parcels);
        } else {
            district.data.parcel_type = ParcelType::OffLimits;
            info!("District {:?} classified as Off-Limits with score {} ({} blocks, {} parcels)", id, score, blocks, n_parcels);
        }
    }

    info!("Options for prime urban parcel: {:?}", options);

    // Primes ordered best-first. Try each in turn: if a prime cannot grow a city of at least
    // URBAN_SIZE_MIN from qualifying neighbours, fall back to the next-best prime.
    let primes = rank_prime_urban_districts(options, parcel_analysis_data);

    let mut city: Option<Vec<DistrictID>> = None;
    for prime in primes.iter() {
        if let Some(grown) = try_grow_city(*prime, districts, parcel_analysis_data) {
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

    // Tally the committed city's total footprint for visibility into how large the urban core ended up.
    let mut city_blocks = 0usize;
    let mut city_parcels = 0usize;
    for id in &city {
        let sd = districts.get_mut(id).expect("District not found");
        sd.data.parcel_type = ParcelType::Urban;
        city_blocks += sd.data.points_2d.len();
        city_parcels += sd.parcels.len();
        info!(
            "Urban district {:?} committed to city ({} blocks, {} parcels)",
            id, sd.data.points_2d.len(), sd.parcels.len()
        );
    }
    info!(
        "City committed: {} districts, {} parcels, {} blocks total",
        city.len(), city_parcels, city_blocks
    );

    // classify remaining districts as rural if they are unknown
    for parcel in districts.values_mut() {
        if parcel.data.parcel_type == ParcelType::Unknown {
            parcel.data.parcel_type = ParcelType::Rural;
        }
    }


}

/// Attempt to grow a city anchored at `prime` without mutating any classification.
///
/// Greedily adds the best adjacent unclassified district (by `get_candidate_score`)
/// as long as it clears the relevant cutoff: the normal cutoff while below URBAN_SIZE_MIN,
/// a higher cutoff to keep growing up to URBAN_SIZE_MAX. Returns the chosen set only if it
/// reaches URBAN_SIZE_MIN, otherwise `None` so the caller can fall back to another prime.
fn try_grow_city(
    prime: DistrictID,
    districts: &HashMap<DistrictID, District>,
    parcel_analysis_data: &HashMap<DistrictID, ParcelAnalysis>,
) -> Option<Vec<DistrictID>> {
    let mut urban: Vec<DistrictID> = vec![prime];
    let mut urban_set: HashSet<DistrictID> = HashSet::from([prime]);

    while (urban.len() as u32) < URBAN_SIZE_MAX {
        // Gather candidate neighbours of the current (tentative) urban set that are still unclassified.
        let mut candidates: Vec<DistrictID> = Vec::new();
        for id in urban.iter() {
            let neighbours = districts.get(id).expect("District not found").data().parcel_adjacency.keys()
                .filter(|&&neighbour_id| {
                    !urban_set.contains(&neighbour_id)
                        && districts.get(&neighbour_id).expect("District not found").data().parcel_type == ParcelType::Unknown
                })
                .cloned()
                .collect::<Vec<DistrictID>>();
            candidates.extend(neighbours);
        }

        let best = candidates.iter()
            .map(|&id| {
                // Adjacency is measured against the whole current urban set (not just the prime), so
                // candidates nestled into the city are preferred over lone tendrils — keeps the city compact.
                let adjacency_ratio = set_adjacency_ratio(districts, &urban_set, id);
                let score = get_candidate_score(parcel_analysis_data, prime, id, Some(adjacency_ratio));
                info!("Candidate {:?} has score {} (set-adjacency {:.3})", id, score, adjacency_ratio);
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

/// Fraction of `candidate`'s perimeter that touches the current urban set — the compactness signal for
/// city growth. High when the candidate is enveloped by the city (concave infill), low for a parcel
/// hanging off a thin edge, so growth fills in around the city instead of trailing off into whispy
/// tendrils. Measured candidate-centric so it stays in `[0,1]` regardless of how large the city is.
fn set_adjacency_ratio(
    districts: &HashMap<DistrictID, District>,
    urban_set: &HashSet<DistrictID>,
    candidate: DistrictID,
) -> f32 {
    let sd = districts.get(&candidate).expect("District not found");
    let perimeter = sd.adjacencies_count();
    if perimeter == 0 {
        return 0.0;
    }
    let shared: u32 = urban_set.iter()
        .filter_map(|member| sd.parcel_adjacency().get(member))
        .sum();
    shared as f32 / perimeter as f32
}

fn select_prime_urban_parcel(options: Vec<ParcelID>, parcel_analysis_data: &HashMap<ParcelID, ParcelAnalysis>) -> Option<ParcelID> {
    options.iter()
        .map(|&id| {
            let analysis_data = parcel_analysis_data.get(&id).expect("Parcel analysis data not found");
            let score = urban_parcel_score(analysis_data);
            (id, score)
        })
        .min_by(|(_, score1), (_, score2)| score1.partial_cmp(score2).expect("We should be able to compare scores"))
        .map(|(other, _score)| {
            info!("Best candidate is {:?}", other);
            other
        })
}

/// Rank urban candidates best-first (lower `urban_parcel_score` is better) so the caller
/// can fall back from the prime to the next-best candidate when growth fails.
fn rank_prime_urban_districts(options: Vec<DistrictID>, parcel_analysis_data: &HashMap<DistrictID, ParcelAnalysis>) -> Vec<DistrictID> {
    let mut scored: Vec<(DistrictID, f32)> = options.iter()
        .map(|&id| {
            let analysis_data = parcel_analysis_data.get(&id).expect("Parcel analysis data not found");
            (id, urban_parcel_score(analysis_data))
        })
        .collect();
    scored.sort_by(|(_, score1), (_, score2)| score1.partial_cmp(score2).expect("We should be able to compare scores"));
    info!("Prime urban candidates ranked best-first: {:?}", scored);
    scored.into_iter().map(|(id, _)| id).collect()
}

fn urban_parcel_score(analysis_data: &ParcelAnalysis) -> f32 {
    // Calculate a score based on the analysis data for urban parcels
    let water_score = analysis_data.water_percentage();
    let forest_score = analysis_data.forested_percentage();
    let gradient_score = analysis_data.gradient() / 3.0;
    let roughness_score = analysis_data.roughness();

    water_score + forest_score + gradient_score + roughness_score
}

fn district_score(district: &District, parcels: &mut HashMap<ParcelID, Parcel>) -> f32 {
    let sub_types = district.get_subtypes(parcels);
    return sub_types.iter()
        .map(|(parcel_type, count)| {
            match parcel_type {
                ParcelType::Urban => *count as f32 * 0.0,
                ParcelType::Rural => *count as f32 * 1.0,
                ParcelType::OffLimits => *count as f32 * 2.0,
                _ => 2.0,
            }
        })
        .sum::<f32>() / district.parcels().len() as f32
}