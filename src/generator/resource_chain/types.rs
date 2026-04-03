use std::collections::HashMap;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ResourceDef {
    pub name: String,
    pub category: String,
    pub tier: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecipeDef {
    pub inputs: HashMap<String, u32>,
    pub outputs: HashMap<String, u32>,
    pub building: String,
}

/// Wrapper structs matching the top-level YAML keys.
#[derive(Debug, Deserialize)]
pub struct ResourcesFile {
    pub resources: HashMap<String, ResourceDef>,
}

#[derive(Debug, Deserialize)]
pub struct RecipesFile {
    pub recipes: HashMap<String, RecipeDef>,
}

#[derive(Debug, Deserialize)]
pub struct BiomeResourcesFile {
    pub biome_resources: HashMap<String, Vec<String>>,
}
