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
    /// Name of the production painter to run after placing this recipe's building.
    /// Only meaningful on gather (no-input) recipes.
    #[serde(default)]
    pub production_painter: Option<String>,
}

/// The resource and gathering building assigned to a single district after selection.
#[derive(Debug, Clone)]
pub struct DistrictResourceAssignment {
    /// The raw resource this district will produce (e.g. `"wood"`).
    pub resource: String,
    /// The gathering building required to produce it (e.g. `"logging_camp"`).
    pub building: String,
    /// Painter name to run after placing the building, if any.
    pub production_painter: Option<String>,
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
