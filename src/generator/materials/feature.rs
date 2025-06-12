use std::collections::HashMap;
use serde_derive::{Serialize, Deserialize};

use crate::generator::materials::{Material, MaterialId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaterialFeature {
    Shade,
    Wear,
    Moisture,
    Decoration,
}

pub const MATERIAL_FEATURE_TRAVERSAL_ORDER : [MaterialFeature; 4] = [
    MaterialFeature::Decoration,
    MaterialFeature::Shade,
    MaterialFeature::Wear,
    MaterialFeature::Moisture,
];

pub enum MaterialFeatureMapping {
    Linear,
    Fitted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialParameters {
    pub shade : f32,
    pub wear : f32,
    pub moisture : f32,
    pub decoration : f32,
}

fn more(
    material : &MaterialId,
    feature : MaterialFeature,
    materials : &HashMap<MaterialId, Material>
) -> Vec<MaterialId> {
    let mut result = Vec::new();
    let mut current_material = material;

    while let Some(material_data) = materials.get(current_material) {
        if let Some(next_material) = material_data.more(feature) {
            result.push(next_material.clone());
            current_material = next_material;
        } else {
            break;
        }
    }

    result   
}

fn less(
    material : &MaterialId,
    feature : MaterialFeature,
    materials : &HashMap<MaterialId, Material>
) -> Vec<MaterialId> {
    let mut result = Vec::new();
    let mut current_material = material;

    while let Some(material_data) = materials.get(current_material) {
        if let Some(next_material) = material_data.less(feature) {
            result.push(next_material.clone());
            current_material = next_material;
        } else {
            break;
        }
    }

    result
}

pub fn map_features(
    parameters : &MaterialParameters,
    material : &MaterialId,
    materials : &HashMap<MaterialId, Material>,
) -> MaterialId {
    let mut material = material.clone();

    for feature in MATERIAL_FEATURE_TRAVERSAL_ORDER.iter() {
        material = map_feature(
            match feature {
                MaterialFeature::Shade => parameters.shade,
                MaterialFeature::Wear => parameters.wear,
                MaterialFeature::Moisture => parameters.moisture,
                MaterialFeature::Decoration => parameters.decoration,
            },
            &material,
            *feature,
            materials,
            MaterialFeatureMapping::Fitted, // or Linear based on your needs
        ); 
    }

    material
}

pub fn map_feature(
    value : f32,
    material : &MaterialId,
    feature : MaterialFeature,
    materials : &HashMap<MaterialId, Material>,
    mapping : MaterialFeatureMapping,
) -> MaterialId {
    let mut more = more(material, feature, materials);
    let mut less = less(material, feature, materials);

    match mapping {
        MaterialFeatureMapping::Linear => {
            let length = more.len().min(less.len());
            let mut materials : Vec<MaterialId> = vec![];

            for i in 0..length {
                materials.push(less[length - 1 - i].clone());
            }

            materials.push(material.clone());

            for i in 0..length {
                materials.push(more[i].clone());
            }

            materials
                .get((value * (length * 2 + 1) as f32) as usize)
                .cloned()
                .unwrap_or(material.clone())
        },
        MaterialFeatureMapping::Fitted => {
            more.push(material.clone());
            less.insert(0, material.clone());

            if value < 0.5 {
                let value = 2.0 * (0.5 - value); // Rescale
                let index = (value * less.len() as f32) as usize;
                less.get(index.min(less.len() - 1)).expect("Index out of range").clone()
            } else {
                let value = 1.0 - 2.0 * (value - 0.5);  // Rescale
                let index = (value * more.len() as f32) as usize;
                more.get(index.min(more.len() - 1)).expect("Index out of range").clone()
            }
        }
    }
}