use serde_derive::{Deserialize, Serialize};
use crate::minecraft::{Block, BlockID};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NetherBlock {
    NetherBricks(NetherBricks),
    RedNetherBricks(RedNetherBricks),
    
    #[serde(rename = "minecraft:netherrack")]
    Netherrack,
    
    #[serde(rename = "minecraft:soul_sand")]
    SoulSand,
    #[serde(rename = "minecraft:soul_soil")]
    SoulSoil,
    
    #[serde(rename = "minecraft:magma_block")]
    MagmaBlock,
    #[serde(rename = "minecraft:glowstone")]
    Glowstone,
    #[serde(rename = "minecraft:shroomlight")]
    Shroomlight,
    
    #[serde(rename = "minecraft:crimson_nylium")]
    CrimsonNylium,
    #[serde(rename = "minecraft:warped_nylium")]
    WarpedNylium,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NetherBricks {
    #[serde(rename = "minecraft:nether_bricks")]
    NetherBricks,
    #[serde(rename = "minecraft:nether_brick_slab")]
    NetherBrickSlab,
    #[serde(rename = "minecraft:nether_brick_stairs")]
    NetherBrickStairs,
    #[serde(rename = "minecraft:nether_brick_wall")]
    NetherBrickWall,
    #[serde(rename = "minecraft:nether_brick_fence")]
    NetherBrickFence,
    #[serde(rename = "minecraft:chiseled_nether_bricks")]
    ChiseledNetherBricks,
    #[serde(rename = "minecraft:cracked_nether_bricks")]
    CrackedNetherBricks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RedNetherBricks {
    #[serde(rename = "minecraft:red_nether_bricks")]
    RedNetherBricks,
    #[serde(rename = "minecraft:red_nether_brick_slab")]
    RedNetherBrickSlab,
    #[serde(rename = "minecraft:red_nether_brick_stairs")]
    RedNetherBrickStairs,
    #[serde(rename = "minecraft:red_nether_brick_wall")]
    RedNetherBrickWall,
    #[serde(rename = "minecraft:red_nether_brick_fence")]
    RedNetherBrickFence,
}

impl Into<Block> for NetherBlock {
    fn into(self) -> Block {
        BlockID::NetherBlock(self).into()
    }
}

impl Into<BlockID> for NetherBlock {
    fn into(self) -> BlockID {
        BlockID::NetherBlock(self)
    }
}

impl Into<Block> for NetherBricks {
    fn into(self) -> Block {
        BlockID::NetherBlock(NetherBlock::NetherBricks(self)).into()
    }
}

impl Into<BlockID> for NetherBricks {
    fn into(self) -> BlockID {
        BlockID::NetherBlock(NetherBlock::NetherBricks(self))
    }
}

impl Into<Block> for RedNetherBricks {
    fn into(self) -> Block {
        BlockID::NetherBlock(NetherBlock::RedNetherBricks(self)).into()
    }
}

impl Into<BlockID> for RedNetherBricks {
    fn into(self) -> BlockID {
        BlockID::NetherBlock(NetherBlock::RedNetherBricks(self))
    }
}
