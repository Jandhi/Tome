use serde_derive::{Deserialize, Serialize};
use crate::minecraft::{Block, BlockID};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OceanBlock {
    Prismarine(Prismarine),
    Coral(Coral),
    
    #[serde(rename = "minecraft:sea_lantern")]
    SeaLantern,
    #[serde(rename = "minecraft:sponge")]
    Sponge,
    #[serde(rename = "minecraft:wet_sponge")]
    WetSponge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Prismarine {
    #[serde(rename = "minecraft:prismarine")]
    Prismarine,
    #[serde(rename = "minecraft:prismarine_slab")]
    PrismarineSlab,
    #[serde(rename = "minecraft:prismarine_stairs")]
    PrismarineStairs,
    #[serde(rename = "minecraft:prismarine_wall")]
    PrismarineWall,
    
    #[serde(rename = "minecraft:prismarine_bricks")]
    PrismarineBricks,
    #[serde(rename = "minecraft:prismarine_brick_slab")]
    PrismarineBrickSlab,
    #[serde(rename = "minecraft:prismarine_brick_stairs")]
    PrismarineBrickStairs,
    
    #[serde(rename = "minecraft:dark_prismarine")]
    DarkPrismarine,
    #[serde(rename = "minecraft:dark_prismarine_slab")]
    DarkPrismarineSlab,
    #[serde(rename = "minecraft:dark_prismarine_stairs")]
    DarkPrismarineStairs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Coral {
    // Coral blocks (alive)
    #[serde(rename = "minecraft:tube_coral_block")]
    TubeCoralBlock,
    #[serde(rename = "minecraft:brain_coral_block")]
    BrainCoralBlock,
    #[serde(rename = "minecraft:bubble_coral_block")]
    BubbleCoralBlock,
    #[serde(rename = "minecraft:fire_coral_block")]
    FireCoralBlock,
    #[serde(rename = "minecraft:horn_coral_block")]
    HornCoralBlock,
    
    // Dead coral blocks
    #[serde(rename = "minecraft:dead_tube_coral_block")]
    DeadTubeCoralBlock,
    #[serde(rename = "minecraft:dead_brain_coral_block")]
    DeadBrainCoralBlock,
    #[serde(rename = "minecraft:dead_bubble_coral_block")]
    DeadBubbleCoralBlock,
    #[serde(rename = "minecraft:dead_fire_coral_block")]
    DeadFireCoralBlock,
    #[serde(rename = "minecraft:dead_horn_coral_block")]
    DeadHornCoralBlock,
    
    // Coral (alive)
    #[serde(rename = "minecraft:tube_coral")]
    TubeCoral,
    #[serde(rename = "minecraft:brain_coral")]
    BrainCoral,
    #[serde(rename = "minecraft:bubble_coral")]
    BubbleCoral,
    #[serde(rename = "minecraft:fire_coral")]
    FireCoral,
    #[serde(rename = "minecraft:horn_coral")]
    HornCoral,
    
    // Dead coral
    #[serde(rename = "minecraft:dead_tube_coral")]
    DeadTubeCoral,
    #[serde(rename = "minecraft:dead_brain_coral")]
    DeadBrainCoral,
    #[serde(rename = "minecraft:dead_bubble_coral")]
    DeadBubbleCoral,
    #[serde(rename = "minecraft:dead_fire_coral")]
    DeadFireCoral,
    #[serde(rename = "minecraft:dead_horn_coral")]
    DeadHornCoral,
    
    // Coral fans (alive)
    #[serde(rename = "minecraft:tube_coral_fan")]
    TubeCoralFan,
    #[serde(rename = "minecraft:brain_coral_fan")]
    BrainCoralFan,
    #[serde(rename = "minecraft:bubble_coral_fan")]
    BubbleCoralFan,
    #[serde(rename = "minecraft:fire_coral_fan")]
    FireCoralFan,
    #[serde(rename = "minecraft:horn_coral_fan")]
    HornCoralFan,
    
    // Dead coral fans
    #[serde(rename = "minecraft:dead_tube_coral_fan")]
    DeadTubeCoralFan,
    #[serde(rename = "minecraft:dead_brain_coral_fan")]
    DeadBrainCoralFan,
    #[serde(rename = "minecraft:dead_bubble_coral_fan")]
    DeadBubbleCoralFan,
    #[serde(rename = "minecraft:dead_fire_coral_fan")]
    DeadFireCoralFan,
    #[serde(rename = "minecraft:dead_horn_coral_fan")]
    DeadHornCoralFan,
}

impl Into<Block> for OceanBlock {
    fn into(self) -> Block {
        BlockID::OceanBlock(self).into()
    }
}

impl Into<BlockID> for OceanBlock {
    fn into(self) -> BlockID {
        BlockID::OceanBlock(self)
    }
}

impl Into<Block> for Prismarine {
    fn into(self) -> Block {
        BlockID::OceanBlock(OceanBlock::Prismarine(self)).into()
    }
}

impl Into<BlockID> for Prismarine {
    fn into(self) -> BlockID {
        BlockID::OceanBlock(OceanBlock::Prismarine(self))
    }
}

impl Into<Block> for Coral {
    fn into(self) -> Block {
        BlockID::OceanBlock(OceanBlock::Coral(self)).into()
    }
}

impl Into<BlockID> for Coral {
    fn into(self) -> BlockID {
        BlockID::OceanBlock(OceanBlock::Coral(self))
    }
}
