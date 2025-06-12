
use super::{
    District,
    DistrictID,
    DistrictAnalysis,
    SuperDistrict,
    SuperDistrictID,
    district::DistrictType,
    constants::{OFF_LIMITS_ROUGHNESS, OFF_LIMITS_GRADIENT, URBAN_WATER_LIMIT, URBAN_SIZE, URBAN_RELATIVE_TO_PRIME},
    merge::{get_candidate_score, district_similarity_score},
    data::HasDistrictData
};


use std::collections::HashMap;
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
        if analysis_data.water_percentage() <= URBAN_WATER_LIMIT && district.data.district_type == DistrictType::Unknown {
            options.push(*id);
        }
    }
    info!("Options for prime urban district: {:?}", options);
    let prime_urban_district: DistrictID = select_prime_urban_district(options, district_analysis_data).expect("No prime urban candidate found"); // Placeholder for prime urban district ID

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
        if score <= 0.5 {
            info!("Superdistrict {:?} classified as Urban option with score {}", id, score);
            options.push(*id);
        } else if score > 0.5 && score <= 1.5 {
            superdistrict.data.district_type = DistrictType::Rural;
            info!("Superdistrict {:?} classified as Rural with score {}", id, score);
        } else {
            superdistrict.data.district_type = DistrictType::OffLimits;
            info!("Superdistrict {:?} classified as Off-Limits with score {}", id, score);
        }
    }

    info!("Options for prime urban district: {:?}", options);
    let prime_urban_district: SuperDistrictID = select_prime_urban_superdistrict(options, district_analysis_data).expect("No prime urban candidate found"); // Placeholder for prime urban district ID
    superdistricts.get_mut(&prime_urban_district).expect("SuperDistrict not found").data.district_type = DistrictType::Urban;
    classify_urban_districts(prime_urban_district, superdistricts, districts, district_analysis_data);

    // classify remaining superdistricts as rural if they are unknown
    for district in superdistricts.values_mut() {
        if district.data.district_type == DistrictType::Unknown {
            district.data.district_type = DistrictType::Rural;
        }
    }


}

fn classify_urban_districts(prime_urban_district: SuperDistrictID, superdistricts: &mut HashMap<SuperDistrictID, SuperDistrict>, _districts: &mut HashMap<DistrictID, District>, district_analysis_data: &HashMap<SuperDistrictID, DistrictAnalysis>) {
    let mut urban_districts: Vec<SuperDistrictID> = vec![prime_urban_district];
    let mut urban_count : u32 = 1;
    while urban_count < URBAN_SIZE {
        let mut options: Vec<SuperDistrictID> = Vec::new();
        //let district_ids = superdistricts.keys().map(|id| id.clone()).collect::<Vec<SuperDistrictID>>();
        for id in urban_districts.clone().into_iter() {
            let neighbours = superdistricts.get(&id).expect("SuperDistrict not found").data().district_adjacency.keys()
                .filter(|&&neighbour_id| superdistricts.get(&neighbour_id).expect("SuperDistrict not found").data().district_type == DistrictType::Unknown)
                .cloned()
                .collect::<Vec<SuperDistrictID>>();
            options.extend(neighbours);
        }
        println!("Options for urban district classification: {:?}", options);
        let best_candidate = options.iter()
            .map(|&id| {
                let score = get_candidate_score(&superdistricts, district_analysis_data, prime_urban_district, id, true);
                println!("Candidate {:?} has score {}", id, score);
                (id, score)
            })
            .max_by(|(_, score1), (_, score2)| score1.partial_cmp(score2).expect("We should be able to compare scores"))
            .map(|(other, _score)| {
                println!("Best candidate is {:?}", other);
                other
            });
        
        if best_candidate.is_none() {
            println!("No more candidates found, stopping urban district classification.");
            break;
        } else {
            superdistricts.get_mut(&best_candidate.expect("No best candidate found")).expect("SuperDistrict not found").data.district_type = DistrictType::Urban;
        }
        urban_count += 1;
        urban_districts.push(best_candidate.expect("No best candidate found"));
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

fn select_prime_urban_superdistrict(options: Vec<SuperDistrictID>, district_analysis_data: &HashMap<SuperDistrictID, DistrictAnalysis>) -> Option<SuperDistrictID> {
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
        .sum()
}