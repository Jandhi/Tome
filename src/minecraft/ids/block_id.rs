use serde_derive::{Deserialize, Serialize};
use crate::minecraft::{Fluid, ids::{
    ColoredBlock, MetalBlock, MiscBlock, NaturalBlock, NetherBlock, OceanBlock, PlantBlock, Stone, UtilityBlock, WoodBlock
}};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlockID {

    // === AIR ===
    #[serde(rename = "minecraft:air")]
    Air,
    #[serde(rename = "minecraft:cave_air")]
    CaveAir,
    #[serde(rename = "minecraft:void_air")]
    VoidAir,

    // === NATURAL BLOCKS ===
    NaturalBlock(NaturalBlock),

    // === STONE ===
    Stone(Stone),

    // === WOOD ===
    WoodBlock(WoodBlock),

    // === COLORED BLOCKS ===
    ColoredBlock(ColoredBlock),

    // === NETHER BLOCKS ===
    NetherBlock(NetherBlock),

    // === METAL BLOCKS ===
    MetalBlock(MetalBlock),

    // === OCEAN BLOCKS ===
    OceanBlock(OceanBlock),

    // === PLANT BLOCKS ===
    PlantBlock(PlantBlock),

    // === UTILITY BLOCKS ===
    UtilityBlock(UtilityBlock),

    // === MISC BLOCKS ===
    MiscBlock(MiscBlock),

    // UNKNOWN
    #[serde(other)]
    Unknown,
}

impl BlockID {
    pub fn is_water(self) -> bool {
        matches!(
            self,
            BlockID::MiscBlock(MiscBlock::Fluid(Fluid::Water))
            | BlockID::MiscBlock(MiscBlock::Fluid(Fluid::BubbleColumn))
            | BlockID::MiscBlock(MiscBlock::Fluid(Fluid::Kelp))
            | BlockID::MiscBlock(MiscBlock::Fluid(Fluid::KelpPlant))
            | BlockID::MiscBlock(MiscBlock::Fluid(Fluid::Seagrass))
            | BlockID::MiscBlock(MiscBlock::Fluid(Fluid::TallSeagrass))
        ) || matches!(self, BlockID::NaturalBlock(NaturalBlock::IceAndSnow(_)))
    }
    
    pub fn is_log(self) -> bool {
        matches!(self, BlockID::WoodBlock(WoodBlock::Log(_)))
    }
    
    pub fn is_mushroom(self) -> bool {
        matches!(
            self,
            BlockID::PlantBlock(PlantBlock::Mushroom(_))
        )
    }
    
    pub fn is_leaf(self) -> bool {
        matches!(self, BlockID::WoodBlock(WoodBlock::Leaves(_)))
    }
    
    pub fn is_tree(self) -> bool {
        self.is_log() || self.is_mushroom()
    }
    
    pub fn is_tree_or_leaf(self) -> bool {
        self.is_leaf() || self.is_log() || self.is_mushroom()
    }

    pub fn is_stairs(self) -> bool {
        matches!(self, BlockID::WoodBlock(WoodBlock::Stairs(_))) || matches!(self, BlockID::Stone(_))
    }

    pub fn is_slab(self) -> bool {
        matches!(self, BlockID::WoodBlock(WoodBlock::Slab(_))) || matches!(self, BlockID::Stone(_))
    }

    pub fn is_fence(self) -> bool {
        matches!(self, BlockID::WoodBlock(WoodBlock::Fence(_)))
    }
}

impl From<&str> for BlockID {
    fn from(value: &str) -> Self {
        serde_json::from_str::<BlockID>(&format!("\"{}\"", value)).unwrap()
    }
}

impl Default for BlockID {
    fn default() -> Self {
        BlockID::Air
    }
}
