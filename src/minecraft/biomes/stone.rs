use crate::{generator::materials::PaletteId, minecraft::Biome};
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BiomeStonetype {
    Stone,
    Deepslate,
    Blackstone,
    Sandstone,
    RedSandstone,
}

impl BiomeStonetype {
    pub fn get_stone_palette_ids(self: BiomeStonetype) -> Vec<PaletteId> {
        use BiomeStonetype::*;
        match self {
            Stone => vec!["stone_bricks".into(), "cobblestone".into()],
            Deepslate => vec!["deepslate".into()],
            Blackstone => vec!["blackstone".into()],
            Sandstone => vec!["sandstone".into()],
            RedSandstone => vec!["red_sandstone".into()],
        }
    }

    pub fn from_biome(biome: Biome) -> Vec<BiomeStonetype> {
        match biome.name() {
            "desert" | "desert_hills" | "desert_lakes" | "beach" => vec![BiomeStonetype::Sandstone],
            "badlands" | "eroded_badlands" | "wooded_badlands" | "savanna" | "savanna_plateau" | "shattered_savanna" | "shattered_savanna_plateau" => vec![BiomeStonetype::RedSandstone],
            _ => vec![BiomeStonetype::Stone, BiomeStonetype::Deepslate, BiomeStonetype::Blackstone],
        }
    }
}
