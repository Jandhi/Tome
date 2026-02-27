use crate::{generator::materials::PaletteId, minecraft::Biome};
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BiomeWoodtype {
    Oak,
    Spruce,
    Birch,
    Jungle,
    Acacia,
    DarkOak,
    Mangrove,
    Cherry,
}

impl BiomeWoodtype {
    pub fn get_wood_palette_id(self : BiomeWoodtype) -> PaletteId {
        use BiomeWoodtype::*;
        match self {
            Oak => "oak".into(),
            Spruce => "spruce".into(),
            Birch => "birch".into(),
            Jungle => "jungle".into(),
            Acacia => "acacia".into(),
            DarkOak => "dark_oak".into(),
            Mangrove => "mangrove".into(),
            Cherry => "cherry".into(),
        }
    }

    pub fn from_biome(biome: Biome) -> Option<BiomeWoodtype> {
        if biome.is_unknown() { return None; }
        match biome.name() {
            // Oak: temperate, generic, or mixed forests/plains
            "plains" | "forest" | "sunflower_plains" | "flower_forest" | "meadow" | "grove" | "windswept_forest" | "snowy_forest" => Some(BiomeWoodtype::Oak),
            // Birch: cool, pale, or birch-dominated
            "birch_forest" | "birch_forest_hills" | "tall_birch_forest" | "tall_birch_hills" | "old_growth_birch_forest" | "desert" | "desert_hills" | "desert_lakes" => Some(BiomeWoodtype::Birch),
            // Spruce: cold, taiga, snowy, or pine/spruce
            "taiga" | "taiga_hills" | "taiga_mountains" | "snowy_taiga" | "snowy_taiga_hills" | "snowy_taiga_mountains" | "giant_tree_taiga" | "giant_tree_taiga_hills" | "giant_spruce_taiga" | "giant_spruce_taiga_hills" | "old_growth_pine_taiga" | "old_growth_spruce_taiga" | "snowy_tundra" | "snowy_mountains" | "snowy_plains" | "frozen_peaks" | "jagged_peaks" | "stony_peaks" | "ice_spikes" | "frozen_river" | "frozen_ocean" | "snowy_beach" | "snowy_slopes" => Some(BiomeWoodtype::Spruce),
            // Jungle: jungle, bamboo, lush
            "jungle" | "jungle_hills" | "jungle_edge" | "modified_jungle" | "modified_jungle_edge" | "sparse_jungle" | "bamboo_jungle" | "bamboo_jungle_hills" | "lush_caves" => Some(BiomeWoodtype::Jungle),
            // Acacia: savanna, badlands, dry
            "savanna" | "savanna_plateau" | "shattered_savanna" | "shattered_savanna_plateau" | "badlands" | "badlands_plateau" | "wooded_badlands_plateau" | "modified_badlands_plateau" | "modified_wooded_badlands_plateau" | "eroded_badlands" | "wooded_badlands" | "windswept_savanna" => Some(BiomeWoodtype::Acacia),
            // Dark Oak: dark forest, wooded mountains
            "dark_forest" | "dark_forest_hills" | "wooded_hills" | "wooded_mountains" | "modified_gravelly_mountains" | "gravelly_mountains" | "windswept_hills" | "windswept_gravelly_hills" | "mountain_edge" => Some(BiomeWoodtype::DarkOak),
            // Mangrove
            "mangrove_swamp" => Some(BiomeWoodtype::Mangrove),
            // Cherry
            "cherry_grove" => Some(BiomeWoodtype::Cherry),
            // Swamp: oak
            "swamp" | "swamp_hills" => Some(BiomeWoodtype::Oak),
            // River, ocean, beach, etc: oak (neutral)
            "river" | "beach" | "stone_shore" | "stony_shore" | "deep_ocean" | "ocean" | "warm_ocean" | "lukewarm_ocean" | "cold_ocean" | "deep_warm_ocean" | "deep_lukewarm_ocean" | "deep_cold_ocean" | "deep_frozen_ocean" | "mushroom_fields" | "mushroom_field_shore" => Some(BiomeWoodtype::Oak),
            // Nether: no wood
            "nether" | "soul_sand_valley" | "crimson_forest" | "warped_forest" | "basalt_deltas" | "nether_wastes" => None,
            // The End: no wood
            "the_end" | "small_end_islands" | "end_midlands" | "end_highlands" | "end_barrens" | "deep_dark" => None,
            // Caves: no wood
            "dripstone_caves" => None,
            // Default to oak for any unknown biome
            _ => Some(BiomeWoodtype::Oak),
        }
    }

}
