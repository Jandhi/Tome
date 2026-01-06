use serde_derive::{Deserialize, Serialize};
use crate::minecraft::{Block, BlockID};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NaturalBlock {
    Dirt(Dirt),
    Sand(Sand),
    IceAndSnow(IceAndSnow),
    #[serde(rename = "minecraft:gravel")]
    Gravel,
    #[serde(rename = "minecraft:clay")]
    Clay,
    #[serde(rename = "minecraft:mud")]
    Mud,
    #[serde(rename = "minecraft:muddy_mangrove_roots")]
    MuddyMangroveRoots,

    #[serde(rename = "minecraft:moss_block")]
    MossBlock,
    #[serde(rename = "minecraft:moss_carpet")]
    MossCarpet,
    #[serde(rename = "minecraft:pale_moss_block")]
    PaleMossBlock,
    #[serde(rename = "minecraft:pale_moss_carpet")]
    PaleMossCarpet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dirt {
    #[serde(rename = "minecraft:dirt")]
    Dirt,
    #[serde(rename = "minecraft:farmland")]
    Farmland,
    #[serde(rename = "minecraft:dirt_path")]
    DirtPath,
    #[serde(rename = "minecraft:coarse_dirt")]
    CoarseDirt,
    #[serde(rename = "minecraft:grass_block")]
    GrassBlock,
    #[serde(rename = "minecraft:podzol")]
    Podzol,
    #[serde(rename = "minecraft:rooted_dirt")]
    RootedDirt,
    #[serde(rename = "minecraft:mycelium")]
    Mycelium,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IceAndSnow {
    #[serde(rename = "minecraft:snow")]
    Snow,
    #[serde(rename = "minecraft:snow_block")]
    SnowBlock,
    #[serde(rename = "minecraft:ice")]
    Ice,
    #[serde(rename = "minecraft:packed_ice")]
    PackedIce,
    #[serde(rename = "minecraft:blue_ice")]
    BlueIce,
    #[serde(rename = "minecraft:frosted_ice")]
    FrostedIce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Sand {
    #[serde(rename = "minecraft:sand")]
    Sand,
    #[serde(rename = "minecraft:suspicious_sand")]
    SuspiciousSand,
    #[serde(rename = "minecraft:suspicious_gravel")]
    SuspiciousGravel,
    #[serde(rename = "minecraft:red_sand")]
    RedSand,
}

impl Into<Block> for NaturalBlock {
    fn into(self) -> Block {
        BlockID::NaturalBlock(self).into()
    }
}

impl Into<BlockID> for NaturalBlock {
    fn into(self) -> BlockID {
        BlockID::NaturalBlock(self)
    }
}

impl Into<Block> for Dirt {
    fn into(self) -> Block {
        BlockID::NaturalBlock(NaturalBlock::Dirt(self)).into()
    }
}

impl Into<BlockID> for Dirt {
    fn into(self) -> BlockID {
        BlockID::NaturalBlock(NaturalBlock::Dirt(self))
    }
}

impl Into<Block> for Sand {
    fn into(self) -> Block {
        BlockID::NaturalBlock(NaturalBlock::Sand(self)).into()
    }
}

impl Into<BlockID> for Sand {
    fn into(self) -> BlockID {
        BlockID::NaturalBlock(NaturalBlock::Sand(self))
    }
}

impl Into<Block> for IceAndSnow {
    fn into(self) -> Block {
        BlockID::NaturalBlock(NaturalBlock::IceAndSnow(self)).into()
    }
}

impl Into<BlockID> for IceAndSnow {
    fn into(self) -> BlockID {
        BlockID::NaturalBlock(NaturalBlock::IceAndSnow(self))
    }
}
