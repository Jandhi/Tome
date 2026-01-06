use serde_derive::{Deserialize, Serialize};
use crate::minecraft::{Block, BlockID};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetalBlock {
    Ore(Ore),
    RawOre(RawOre),
    MetalStorage(MetalStorage),
    Copper(Copper),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Ore {
    #[serde(rename = "minecraft:coal_ore")]
    CoalOre,
    #[serde(rename = "minecraft:deepslate_coal_ore")]
    DeepslateCoalOre,
    
    #[serde(rename = "minecraft:iron_ore")]
    IronOre,
    #[serde(rename = "minecraft:deepslate_iron_ore")]
    DeepslateIronOre,
    
    #[serde(rename = "minecraft:copper_ore")]
    CopperOre,
    #[serde(rename = "minecraft:deepslate_copper_ore")]
    DeepslateCopperOre,
    
    #[serde(rename = "minecraft:gold_ore")]
    GoldOre,
    #[serde(rename = "minecraft:deepslate_gold_ore")]
    DeepslateGoldOre,
    #[serde(rename = "minecraft:nether_gold_ore")]
    NetherGoldOre,
    
    #[serde(rename = "minecraft:redstone_ore")]
    RedstoneOre,
    #[serde(rename = "minecraft:deepslate_redstone_ore")]
    DeepslateRedstoneOre,
    
    #[serde(rename = "minecraft:emerald_ore")]
    EmeraldOre,
    #[serde(rename = "minecraft:deepslate_emerald_ore")]
    DeepslateEmeraldOre,
    
    #[serde(rename = "minecraft:lapis_ore")]
    LapisOre,
    #[serde(rename = "minecraft:deepslate_lapis_ore")]
    DeepslateLapisOre,
    
    #[serde(rename = "minecraft:diamond_ore")]
    DiamondOre,
    #[serde(rename = "minecraft:deepslate_diamond_ore")]
    DeepslateDiamondOre,
    
    #[serde(rename = "minecraft:nether_quartz_ore")]
    NetherQuartzOre,
    
    #[serde(rename = "minecraft:ancient_debris")]
    AncientDebris,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RawOre {
    #[serde(rename = "minecraft:raw_iron_block")]
    RawIronBlock,
    #[serde(rename = "minecraft:raw_copper_block")]
    RawCopperBlock,
    #[serde(rename = "minecraft:raw_gold_block")]
    RawGoldBlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetalStorage {
    #[serde(rename = "minecraft:iron_block")]
    IronBlock,
    #[serde(rename = "minecraft:gold_block")]
    GoldBlock,
    #[serde(rename = "minecraft:diamond_block")]
    DiamondBlock,
    #[serde(rename = "minecraft:emerald_block")]
    EmeraldBlock,
    #[serde(rename = "minecraft:lapis_block")]
    LapisBlock,
    #[serde(rename = "minecraft:coal_block")]
    CoalBlock,
    #[serde(rename = "minecraft:redstone_block")]
    RedstoneBlock,
    #[serde(rename = "minecraft:netherite_block")]
    NetheriteBlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Copper {
    // Base copper
    #[serde(rename = "minecraft:copper_block")]
    CopperBlock,
    #[serde(rename = "minecraft:cut_copper")]
    CutCopper,
    #[serde(rename = "minecraft:cut_copper_slab")]
    CutCopperSlab,
    #[serde(rename = "minecraft:cut_copper_stairs")]
    CutCopperStairs,
    
    // Exposed copper
    #[serde(rename = "minecraft:exposed_copper")]
    ExposedCopper,
    #[serde(rename = "minecraft:exposed_cut_copper")]
    ExposedCutCopper,
    #[serde(rename = "minecraft:exposed_cut_copper_slab")]
    ExposedCutCopperSlab,
    #[serde(rename = "minecraft:exposed_cut_copper_stairs")]
    ExposedCutCopperStairs,
    
    // Weathered copper
    #[serde(rename = "minecraft:weathered_copper")]
    WeatheredCopper,
    #[serde(rename = "minecraft:weathered_cut_copper")]
    WeatheredCutCopper,
    #[serde(rename = "minecraft:weathered_cut_copper_slab")]
    WeatheredCutCopperSlab,
    #[serde(rename = "minecraft:weathered_cut_copper_stairs")]
    WeatheredCutCopperStairs,
    
    // Oxidized copper
    #[serde(rename = "minecraft:oxidized_copper")]
    OxidizedCopper,
    #[serde(rename = "minecraft:oxidized_cut_copper")]
    OxidizedCutCopper,
    #[serde(rename = "minecraft:oxidized_cut_copper_slab")]
    OxidizedCutCopperSlab,
    #[serde(rename = "minecraft:oxidized_cut_copper_stairs")]
    OxidizedCutCopperStairs,
    
    // Waxed base copper
    #[serde(rename = "minecraft:waxed_copper_block")]
    WaxedCopperBlock,
    #[serde(rename = "minecraft:waxed_cut_copper")]
    WaxedCutCopper,
    #[serde(rename = "minecraft:waxed_cut_copper_slab")]
    WaxedCutCopperSlab,
    #[serde(rename = "minecraft:waxed_cut_copper_stairs")]
    WaxedCutCopperStairs,
    
    // Waxed exposed copper
    #[serde(rename = "minecraft:waxed_exposed_copper")]
    WaxedExposedCopper,
    #[serde(rename = "minecraft:waxed_exposed_cut_copper")]
    WaxedExposedCutCopper,
    #[serde(rename = "minecraft:waxed_exposed_cut_copper_slab")]
    WaxedExposedCutCopperSlab,
    #[serde(rename = "minecraft:waxed_exposed_cut_copper_stairs")]
    WaxedExposedCutCopperStairs,
    
    // Waxed weathered copper
    #[serde(rename = "minecraft:waxed_weathered_copper")]
    WaxedWeatheredCopper,
    #[serde(rename = "minecraft:waxed_weathered_cut_copper")]
    WaxedWeatheredCutCopper,
    #[serde(rename = "minecraft:waxed_weathered_cut_copper_slab")]
    WaxedWeatheredCutCopperSlab,
    #[serde(rename = "minecraft:waxed_weathered_cut_copper_stairs")]
    WaxedWeatheredCutCopperStairs,
    
    // Waxed oxidized copper
    #[serde(rename = "minecraft:waxed_oxidized_copper")]
    WaxedOxidizedCopper,
    #[serde(rename = "minecraft:waxed_oxidized_cut_copper")]
    WaxedOxidizedCutCopper,
    #[serde(rename = "minecraft:waxed_oxidized_cut_copper_slab")]
    WaxedOxidizedCutCopperSlab,
    #[serde(rename = "minecraft:waxed_oxidized_cut_copper_stairs")]
    WaxedOxidizedCutCopperStairs,
}

impl Into<Block> for MetalBlock {
    fn into(self) -> Block {
        BlockID::MetalBlock(self).into()
    }
}

impl Into<BlockID> for MetalBlock {
    fn into(self) -> BlockID {
        BlockID::MetalBlock(self)
    }
}

impl Into<Block> for Ore {
    fn into(self) -> Block {
        BlockID::MetalBlock(MetalBlock::Ore(self)).into()
    }
}

impl Into<BlockID> for Ore {
    fn into(self) -> BlockID {
        BlockID::MetalBlock(MetalBlock::Ore(self))
    }
}

impl Into<Block> for RawOre {
    fn into(self) -> Block {
        BlockID::MetalBlock(MetalBlock::RawOre(self)).into()
    }
}

impl Into<BlockID> for RawOre {
    fn into(self) -> BlockID {
        BlockID::MetalBlock(MetalBlock::RawOre(self))
    }
}

impl Into<Block> for MetalStorage {
    fn into(self) -> Block {
        BlockID::MetalBlock(MetalBlock::MetalStorage(self)).into()
    }
}

impl Into<BlockID> for MetalStorage {
    fn into(self) -> BlockID {
        BlockID::MetalBlock(MetalBlock::MetalStorage(self))
    }
}

impl Into<Block> for Copper {
    fn into(self) -> Block {
        BlockID::MetalBlock(MetalBlock::Copper(self)).into()
    }
}

impl Into<BlockID> for Copper {
    fn into(self) -> BlockID {
        BlockID::MetalBlock(MetalBlock::Copper(self))
    }
}
