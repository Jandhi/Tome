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
            Cherry => "cherry".into(), // 1.20+
        }
    }

    pub fn from_biome(biome: Biome) -> Option<BiomeWoodtype> {
        use super::Biome::*;
        match biome {
            Unknown => None,
            // Oak: temperate, generic, or mixed forests/plains
            Plains | Forest | SunflowerPlains | FlowerForest | Meadow | Grove | WindsweptForest | SnowyForest => Some(BiomeWoodtype::Oak),
            // Birch: cool, pale, or birch-dominated
            BirchForest | BirchForestHills | TallBirchForest | TallBirchHills | OldGrowthBirchForest | Desert => Some(BiomeWoodtype::Birch),
            // Spruce: cold, taiga, snowy, or pine/spruce
            Taiga | TaigaHills | TaigaMountains | SnowyTaiga | SnowyTaigaHills | SnowyTaigaMountains | GiantTreeTaiga | GiantTreeTaigaHills | GiantSpruceTaiga | GiantSpruceTaigaHills | OldGrowthPineTaiga | OldGrowthSpruceTaiga | SnowyTundra | SnowyMountains | SnowyPlains | FrozenPeaks | JaggedPeaks | StonyPeaks | IceSpikes | FrozenRiver | FrozenOcean | SnowyBeach | SnowySlopes => Some(BiomeWoodtype::Spruce),
            // Jungle: jungle, bamboo, lush
            Jungle | JungleHills | JungleEdge | ModifiedJungle | ModifiedJungleEdge | SparseJungle | BambooJungle | BambooJungleHills | LushCaves => Some(BiomeWoodtype::Jungle),
            // Acacia: savanna, badlands, dry, orange
            Savanna | SavannaPlateau | ShatteredSavanna | ShatteredSavannaPlateau | Badlands | BadlandsPlateau | WoodedBadlandsPlateau | ModifiedBadlandsPlateau | ModifiedWoodedBadlandsPlateau | ErodedBadlands | WoodedBadlands | WindsweptSavanna => Some(BiomeWoodtype::Acacia),
            // Dark Oak: dark forest, wooded mountains, wooded hills
            DarkForest | DarkForestHills | WoodedHills | WoodedMountains | ModifiedGravellyMountains | GravellyMountains | WindsweptHills | WindsweptGravellyHills => Some(BiomeWoodtype::DarkOak),
            // Mangrove: mangrove swamp
            MangroveSwamp => Some(BiomeWoodtype::Mangrove),
            // Cherry: cherry grove (1.20+)
            CherryGroveNew => Some(BiomeWoodtype::Cherry),
            // Swamp: oak or mangrove, but vanilla is oak
            Swamp | SwampHills => Some(BiomeWoodtype::Oak),
            // River, ocean, beach, stone shore, stony shore: oak (neutral)
            River | Beach | StoneShore | StonyShore | DeepOcean | Ocean | WarmOcean | LukewarmOcean | ColdOcean | DeepWarmOcean | DeepLukewarmOcean | DeepColdOcean | DeepFrozenOcean | MushroomFields | MushroomFieldShore => Some(BiomeWoodtype::Oak),
            // Nether: crimson/warped, but not a vanilla wood, so None
            Nether | SoulSandValley | CrimsonForest | WarpedForest | BasaltDeltas | NetherWastes => None,
            // The End: no wood
            TheEnd | SmallEndIslands | EndMidlands | EndHighlands | EndBarrens | DeepDark => None,
            // Caves: no wood
            DripstoneCaves => None,
            // Misc: default to oak if not matched above
            DesertHills | DesertLakes => Some(BiomeWoodtype::Birch),
            MountainEdge => Some(BiomeWoodtype::DarkOak),
            // If not matched, default to oak
            _ => Some(BiomeWoodtype::Oak),
        }
    }

}

