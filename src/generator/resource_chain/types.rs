use std::collections::HashMap;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ResourceDef {
    pub name: String,
    pub category: String,
    pub tier: u8,
    /// How strongly this resource wants flat terrain, in `[0,1]` (tweakable per
    /// resource in `resources.yaml`). Crop fields and grazing pastures (wheat, cows)
    /// need terrain we can flatten cheaply, so they set a high value; the parcel
    /// assignment scales its rough/steep-terrain penalty by this weight to keep them
    /// off parcels that would need heavy post-placement terraforming.
    /// `0.0` (the default) means terrain-agnostic; `1.0` means a full penalty.
    #[serde(default)]
    pub flat_terrain: f32,
    /// For mined resources, the Minecraft ore block this resource appears as in the
    /// world (stone variant, e.g. `minecraft:coal_ore`). The mine painter places it
    /// in outcrops and surface seams, switching to the deepslate variant where the
    /// local rock is deepslate. `None` for non-mined resources.
    #[serde(default)]
    pub ore_block: Option<String>,
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

/// The resource and gathering building assigned to a single parcel after selection.
#[derive(Debug, Clone)]
pub struct ParcelResourceAssignment {
    /// The biome-selected raw resource used to identify this parcel's gather recipe
    /// (e.g. `"wood"`). For multi-output gather recipes (e.g. `gather_bees` → honey +
    /// beeswax), this is the primary output; all outputs are credited to the supply pool.
    pub primary_resource: String,
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
