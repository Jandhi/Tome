use std::collections::{HashMap, HashSet};

use log::info;

use crate::editor::World;

use super::analysis::analyze_district;
use super::{constants::TARGET_DISTRICT_AMOUNT, DistrictAnalysis, SuperDistrict, SuperDistrictID};
use super::{District, DistrictID, HasDistrictData};


pub async fn merge_down(superdistricts : &mut HashMap<SuperDistrictID, SuperDistrict>, districts : &HashMap<DistrictID, District>, district_analysis_data : &mut HashMap<SuperDistrictID, DistrictAnalysis>, world : &mut World) {
    let mut district_count = superdistricts.len();
    let mut ignore : HashSet<SuperDistrictID>= HashSet::new();

    while district_count > TARGET_DISTRICT_AMOUNT as usize {
        let child = superdistricts.iter()
            .filter(|(id, _)| !ignore.contains(&id))
            .min_by_key(|(_, district)| district.size())
            .map(|(id, _)| *id);

        let Some(child) = child else {
            info!("Out of districts to merge, stopping.");
            break;
        };

        let neighbours : Vec<SuperDistrictID> = superdistricts.get(&child).expect(&format!("Superdistrict with id {} not found", child.0)).district_adjacency().keys().cloned().collect();

        let parent = get_best_merge_candidate(superdistricts, district_analysis_data, child, neighbours);

        let Some(parent) = parent else {
            ignore.insert(child);

            // Remove garbage districts
            if superdistricts.get(&child).expect(&format!("Superdistrict with id {} not found", child.0)).size() < 10 {
                remove_district(superdistricts, child, world);
                district_count -= 1;
            }

            continue;
        };

        merge(superdistricts, districts, district_analysis_data, parent, child, world).await;
    }
}


fn remove_district(districts : &mut HashMap<SuperDistrictID, SuperDistrict>, district_id : SuperDistrictID, world : &mut World) {
    for point in districts.get(&district_id).expect(&format!("Superdistrict with id {} not found", district_id.0)).points_2d().iter() {
        world.super_district_map[point.x as usize][point.y as usize] = None;
    }
    
    districts.remove(&district_id);
}


async fn merge(superdistricts : &mut HashMap<SuperDistrictID, SuperDistrict>, districts : &HashMap<DistrictID, District>, district_analysis_data : &mut HashMap<SuperDistrictID, DistrictAnalysis>, parent : SuperDistrictID, child : SuperDistrictID, world : &mut World) {
    let child = superdistricts.remove(&child).expect(&format!("Superdistrict with id {} not found", child.0));
    let parent = superdistricts.get_mut(&parent).expect(&format!("Superdistrict with id {} not found", parent.0));
    parent.add_superdistrict(&child, districts, world);
    let new_analysis = analyze_district(parent.data(), world).await;
    district_analysis_data.insert(parent.id(), new_analysis);
}

fn get_best_merge_candidate(superdistricts : &HashMap<SuperDistrictID, SuperDistrict>, district_analysis_data : &HashMap<SuperDistrictID, DistrictAnalysis>, target : SuperDistrictID, options : Vec<SuperDistrictID>) -> Option<SuperDistrictID> {
    options.iter()
        // Only merge border districts with other border districts
        .filter(|other| {
            superdistricts.contains_key(other) && superdistricts.get(other).expect("Could not find district with id").is_border()
                == superdistricts.get(&target).expect("Could not find district with id").is_border()
        })
        .map(|other| {
            let score = get_candidate_score(superdistricts,  district_analysis_data, target, *other, true);
            (*other, score)
        })
        // Our best candidate has to be 0.33 at minimum
        .filter(|(_, score)| {
            *score > 0.33
        })
        .max_by(|(_, score1), (_, score2)| score1.partial_cmp(score2).expect("We should be able to compare scores"))
        .map(|(other, _score)| other)
}

const ADJACENCY_WEIGHT : f32 = 3.0;
fn get_candidate_score(districts : &HashMap<SuperDistrictID, SuperDistrict>, district_analysis_data : &HashMap<SuperDistrictID, DistrictAnalysis>, target : SuperDistrictID, candidate : SuperDistrictID, use_adjacency : bool) -> f32 {
    let target_analysis = district_analysis_data.get(&target).expect("Could not find district analysis data for target");
    let candidate_analysis = district_analysis_data.get(&candidate).expect("Could not find district analysis data for candidate");
    let target = districts.get(&target).expect("Could not find district with id");
    let candidate = districts.get(&candidate).expect("Could not find district with id");
    

    let adjacency_ratio = (*target.district_adjacency().get(&target.id()).unwrap_or(&0) as f32) / (target.adjacencies_count() as f32);
    let adjacency_score : f32 = 1000.0 * adjacency_ratio / (candidate.size() as f32);

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