use serde_derive::{Deserialize, Serialize};
use crate::minecraft::{Block, BlockID};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WoodBlock {
    Planks(Planks),
    Log(Log),
    StrippedLog(StrippedLog),
    Wood(Wood),
    StrippedWood(StrippedWood),
    Stairs(WoodStairs),
    Slab(WoodSlab),
    Fence(WoodFence),
    FenceGate(WoodFenceGate),
    Door(WoodDoor),
    Trapdoor(WoodTrapdoor),
    Button(WoodButton),
    PressurePlate(WoodPressurePlate),
    Sign(WoodSign),
    WallSign(WoodWallSign),
    HangingSign(WoodHangingSign),
    HangingWallSign(WoodHangingWallSign),
    Sapling(Sapling),
    Leaves(Leaves),

}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TreeType {
    Oak,
    Spruce,
    Birch,
    Jungle,
    Acacia,
    DarkOak,
    Mangrove,
    Cherry,
    Azalea,
    FloweringAzalea,
}

// Planks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Planks {
    #[serde(rename = "minecraft:oak_planks")]
    Oak,
    #[serde(rename = "minecraft:spruce_planks")]
    Spruce,
    #[serde(rename = "minecraft:birch_planks")]
    Birch,
    #[serde(rename = "minecraft:jungle_planks")]
    Jungle,
    #[serde(rename = "minecraft:acacia_planks")]
    Acacia,
    #[serde(rename = "minecraft:dark_oak_planks")]
    DarkOak,
    #[serde(rename = "minecraft:mangrove_planks")]
    Mangrove,
    #[serde(rename = "minecraft:cherry_planks")]
    Cherry,
    #[serde(rename = "minecraft:bamboo_planks")]
    Bamboo,
    #[serde(rename = "minecraft:bamboo_mosaic")]
    BambooMosaic,
    #[serde(rename = "minecraft:crimson_planks")]
    Crimson,
    #[serde(rename = "minecraft:warped_planks")]
    Warped,
}

// Logs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Log {
    #[serde(rename = "minecraft:oak_log")]
    Oak,
    #[serde(rename = "minecraft:spruce_log")]
    Spruce,
    #[serde(rename = "minecraft:birch_log")]
    Birch,
    #[serde(rename = "minecraft:jungle_log")]
    Jungle,
    #[serde(rename = "minecraft:acacia_log")]
    Acacia,
    #[serde(rename = "minecraft:dark_oak_log")]
    DarkOak,
    #[serde(rename = "minecraft:mangrove_log")]
    Mangrove,
    #[serde(rename = "minecraft:cherry_log")]
    Cherry,
    #[serde(rename = "minecraft:crimson_stem")]
    CrimsonStem,
    #[serde(rename = "minecraft:warped_stem")]
    WarpedStem,
}

// Stripped Logs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StrippedLog {
    #[serde(rename = "minecraft:stripped_oak_log")]
    Oak,
    #[serde(rename = "minecraft:stripped_spruce_log")]
    Spruce,
    #[serde(rename = "minecraft:stripped_birch_log")]
    Birch,
    #[serde(rename = "minecraft:stripped_jungle_log")]
    Jungle,
    #[serde(rename = "minecraft:stripped_acacia_log")]
    Acacia,
    #[serde(rename = "minecraft:stripped_dark_oak_log")]
    DarkOak,
    #[serde(rename = "minecraft:stripped_mangrove_log")]
    Mangrove,
    #[serde(rename = "minecraft:stripped_cherry_log")]
    Cherry,
    #[serde(rename = "minecraft:stripped_crimson_stem")]
    CrimsonStem,
    #[serde(rename = "minecraft:stripped_warped_stem")]
    WarpedStem,
}

// Wood (bark on all sides)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Wood {
    #[serde(rename = "minecraft:oak_wood")]
    Oak,
    #[serde(rename = "minecraft:spruce_wood")]
    Spruce,
    #[serde(rename = "minecraft:birch_wood")]
    Birch,
    #[serde(rename = "minecraft:jungle_wood")]
    Jungle,
    #[serde(rename = "minecraft:acacia_wood")]
    Acacia,
    #[serde(rename = "minecraft:dark_oak_wood")]
    DarkOak,
    #[serde(rename = "minecraft:mangrove_wood")]
    Mangrove,
    #[serde(rename = "minecraft:cherry_wood")]
    Cherry,
    #[serde(rename = "minecraft:crimson_hyphae")]
    CrimsonHyphae,
    #[serde(rename = "minecraft:warped_hyphae")]
    WarpedHyphae,
}

// Stripped Wood
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StrippedWood {
    #[serde(rename = "minecraft:stripped_oak_wood")]
    Oak,
    #[serde(rename = "minecraft:stripped_spruce_wood")]
    Spruce,
    #[serde(rename = "minecraft:stripped_birch_wood")]
    Birch,
    #[serde(rename = "minecraft:stripped_jungle_wood")]
    Jungle,
    #[serde(rename = "minecraft:stripped_acacia_wood")]
    Acacia,
    #[serde(rename = "minecraft:stripped_dark_oak_wood")]
    DarkOak,
    #[serde(rename = "minecraft:stripped_mangrove_wood")]
    Mangrove,
    #[serde(rename = "minecraft:stripped_cherry_wood")]
    Cherry,
    #[serde(rename = "minecraft:stripped_crimson_hyphae")]
    CrimsonHyphae,
    #[serde(rename = "minecraft:stripped_warped_hyphae")]
    WarpedHyphae,
}

// Stairs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WoodStairs {
    #[serde(rename = "minecraft:oak_stairs")]
    Oak,
    #[serde(rename = "minecraft:spruce_stairs")]
    Spruce,
    #[serde(rename = "minecraft:birch_stairs")]
    Birch,
    #[serde(rename = "minecraft:jungle_stairs")]
    Jungle,
    #[serde(rename = "minecraft:acacia_stairs")]
    Acacia,
    #[serde(rename = "minecraft:dark_oak_stairs")]
    DarkOak,
    #[serde(rename = "minecraft:mangrove_stairs")]
    Mangrove,
    #[serde(rename = "minecraft:bamboo_stairs")]
    Bamboo,
    #[serde(rename = "minecraft:bamboo_mosaic_stairs")]
    BambooMosaic,
    #[serde(rename = "minecraft:cherry_stairs")]
    Cherry,
    #[serde(rename = "minecraft:crimson_stairs")]
    Crimson,
    #[serde(rename = "minecraft:warped_stairs")]
    Warped,
}

// Slabs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WoodSlab {
    #[serde(rename = "minecraft:oak_slab")]
    Oak,
    #[serde(rename = "minecraft:spruce_slab")]
    Spruce,
    #[serde(rename = "minecraft:birch_slab")]
    Birch,
    #[serde(rename = "minecraft:jungle_slab")]
    Jungle,
    #[serde(rename = "minecraft:acacia_slab")]
    Acacia,
    #[serde(rename = "minecraft:dark_oak_slab")]
    DarkOak,
    #[serde(rename = "minecraft:mangrove_slab")]
    Mangrove,
    #[serde(rename = "minecraft:bamboo_slab")]
    Bamboo,
    #[serde(rename = "minecraft:bamboo_mosaic_slab")]
    BambooMosaic,
    #[serde(rename = "minecraft:cherry_slab")]
    Cherry,
    #[serde(rename = "minecraft:crimson_slab")]
    Crimson,
    #[serde(rename = "minecraft:warped_slab")]
    Warped,
}

// Fences
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WoodFence {
    #[serde(rename = "minecraft:oak_fence")]
    Oak,
    #[serde(rename = "minecraft:spruce_fence")]
    Spruce,
    #[serde(rename = "minecraft:birch_fence")]
    Birch,
    #[serde(rename = "minecraft:jungle_fence")]
    Jungle,
    #[serde(rename = "minecraft:acacia_fence")]
    Acacia,
    #[serde(rename = "minecraft:dark_oak_fence")]
    DarkOak,
    #[serde(rename = "minecraft:mangrove_fence")]
    Mangrove,
    #[serde(rename = "minecraft:bamboo_fence")]
    Bamboo,
    #[serde(rename = "minecraft:bamboo_mosaic_fence")]
    BambooMosaicFence,
    #[serde(rename = "minecraft:cherry_fence")]
    Cherry,
    #[serde(rename = "minecraft:crimson_fence")]
    Crimson,
    #[serde(rename = "minecraft:warped_fence")]
    Warped,
}

// Fence Gates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WoodFenceGate {
    #[serde(rename = "minecraft:oak_fence_gate")]
    Oak,
    #[serde(rename = "minecraft:spruce_fence_gate")]
    Spruce,
    #[serde(rename = "minecraft:birch_fence_gate")]
    Birch,
    #[serde(rename = "minecraft:jungle_fence_gate")]
    Jungle,
    #[serde(rename = "minecraft:acacia_fence_gate")]
    Acacia,
    #[serde(rename = "minecraft:dark_oak_fence_gate")]
    DarkOak,
    #[serde(rename = "minecraft:mangrove_fence_gate")]
    Mangrove,
    #[serde(rename = "minecraft:bamboo_fence_gate")]
    Bamboo,
    #[serde(rename = "minecraft:cherry_fence_gate")]
    Cherry,
    #[serde(rename = "minecraft:crimson_fence_gate")]
    Crimson,
    #[serde(rename = "minecraft:warped_fence_gate")]
    Warped,
}

// Doors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WoodDoor {
    #[serde(rename = "minecraft:oak_door")]
    Oak,
    #[serde(rename = "minecraft:spruce_door")]
    Spruce,
    #[serde(rename = "minecraft:birch_door")]
    Birch,
    #[serde(rename = "minecraft:jungle_door")]
    Jungle,
    #[serde(rename = "minecraft:acacia_door")]
    Acacia,
    #[serde(rename = "minecraft:dark_oak_door")]
    DarkOak,
    #[serde(rename = "minecraft:mangrove_door")]
    Mangrove,
    #[serde(rename = "minecraft:bamboo_door")]
    Bamboo,
    #[serde(rename = "minecraft:cherry_door")]
    Cherry,
    #[serde(rename = "minecraft:crimson_door")]
    Crimson,
    #[serde(rename = "minecraft:warped_door")]
    Warped,
}

// Trapdoors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WoodTrapdoor {
    #[serde(rename = "minecraft:oak_trapdoor")]
    Oak,
    #[serde(rename = "minecraft:spruce_trapdoor")]
    Spruce,
    #[serde(rename = "minecraft:birch_trapdoor")]
    Birch,
    #[serde(rename = "minecraft:jungle_trapdoor")]
    Jungle,
    #[serde(rename = "minecraft:acacia_trapdoor")]
    Acacia,
    #[serde(rename = "minecraft:dark_oak_trapdoor")]
    DarkOak,
    #[serde(rename = "minecraft:mangrove_trapdoor")]
    Mangrove,
    #[serde(rename = "minecraft:bamboo_trapdoor")]
    Bamboo,
    #[serde(rename = "minecraft:cherry_trapdoor")]
    Cherry,
    #[serde(rename = "minecraft:crimson_trapdoor")]
    Crimson,
    #[serde(rename = "minecraft:warped_trapdoor")]
    Warped,
}

// Buttons
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WoodButton {
    #[serde(rename = "minecraft:oak_button")]
    Oak,
    #[serde(rename = "minecraft:spruce_button")]
    Spruce,
    #[serde(rename = "minecraft:birch_button")]
    Birch,
    #[serde(rename = "minecraft:jungle_button")]
    Jungle,
    #[serde(rename = "minecraft:acacia_button")]
    Acacia,
    #[serde(rename = "minecraft:dark_oak_button")]
    DarkOak,
    #[serde(rename = "minecraft:mangrove_button")]
    Mangrove,
    #[serde(rename = "minecraft:bamboo_button")]
    Bamboo,
    #[serde(rename = "minecraft:cherry_button")]
    Cherry,
    #[serde(rename = "minecraft:crimson_button")]
    Crimson,
    #[serde(rename = "minecraft:warped_button")]
    Warped,
}

// Pressure Plates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WoodPressurePlate {
    #[serde(rename = "minecraft:oak_pressure_plate")]
    Oak,
    #[serde(rename = "minecraft:spruce_pressure_plate")]
    Spruce,
    #[serde(rename = "minecraft:birch_pressure_plate")]
    Birch,
    #[serde(rename = "minecraft:jungle_pressure_plate")]
    Jungle,
    #[serde(rename = "minecraft:acacia_pressure_plate")]
    Acacia,
    #[serde(rename = "minecraft:dark_oak_pressure_plate")]
    DarkOak,
    #[serde(rename = "minecraft:mangrove_pressure_plate")]
    Mangrove,
    #[serde(rename = "minecraft:bamboo_pressure_plate")]
    Bamboo,
    #[serde(rename = "minecraft:cherry_pressure_plate")]
    Cherry,
    #[serde(rename = "minecraft:crimson_pressure_plate")]
    Crimson,
    #[serde(rename = "minecraft:warped_pressure_plate")]
    Warped,
}

// Signs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WoodSign {
    #[serde(rename = "minecraft:oak_sign")]
    Oak,
    #[serde(rename = "minecraft:spruce_sign")]
    Spruce,
    #[serde(rename = "minecraft:birch_sign")]
    Birch,
    #[serde(rename = "minecraft:jungle_sign")]
    Jungle,
    #[serde(rename = "minecraft:acacia_sign")]
    Acacia,
    #[serde(rename = "minecraft:dark_oak_sign")]
    DarkOak,
    #[serde(rename = "minecraft:mangrove_sign")]
    Mangrove,
    #[serde(rename = "minecraft:bamboo_sign")]
    Bamboo,
    #[serde(rename = "minecraft:cherry_sign")]
    Cherry,
    #[serde(rename = "minecraft:crimson_sign")]
    Crimson,
    #[serde(rename = "minecraft:warped_sign")]
    Warped,
}

// Wall Signs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WoodWallSign {
    #[serde(rename = "minecraft:oak_wall_sign")]
    Oak,
    #[serde(rename = "minecraft:spruce_wall_sign")]
    Spruce,
    #[serde(rename = "minecraft:birch_wall_sign")]
    Birch,
    #[serde(rename = "minecraft:jungle_wall_sign")]
    Jungle,
    #[serde(rename = "minecraft:acacia_wall_sign")]
    Acacia,
    #[serde(rename = "minecraft:dark_oak_wall_sign")]
    DarkOak,
    #[serde(rename = "minecraft:mangrove_wall_sign")]
    Mangrove,
    #[serde(rename = "minecraft:bamboo_wall_sign")]
    Bamboo,
    #[serde(rename = "minecraft:cherry_wall_sign")]
    Cherry,
    #[serde(rename = "minecraft:crimson_wall_sign")]
    Crimson,
    #[serde(rename = "minecraft:warped_wall_sign")]
    Warped,
}

// Hanging Signs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WoodHangingSign {
    #[serde(rename = "minecraft:oak_hanging_sign")]
    Oak,
    #[serde(rename = "minecraft:spruce_hanging_sign")]
    Spruce,
    #[serde(rename = "minecraft:birch_hanging_sign")]
    Birch,
    #[serde(rename = "minecraft:jungle_hanging_sign")]
    Jungle,
    #[serde(rename = "minecraft:acacia_hanging_sign")]
    Acacia,
    #[serde(rename = "minecraft:dark_oak_hanging_sign")]
    DarkOak,
    #[serde(rename = "minecraft:mangrove_hanging_sign")]
    Mangrove,
    #[serde(rename = "minecraft:bamboo_hanging_sign")]
    Bamboo,
    #[serde(rename = "minecraft:cherry_hanging_sign")]
    Cherry,
    #[serde(rename = "minecraft:crimson_hanging_sign")]
    Crimson,
    #[serde(rename = "minecraft:warped_hanging_sign")]
    Warped,
}

// Hanging Wall Signs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WoodHangingWallSign {
    #[serde(rename = "minecraft:oak_hanging_wall_sign")]
    Oak,
    #[serde(rename = "minecraft:spruce_hanging_wall_sign")]
    Spruce,
    #[serde(rename = "minecraft:birch_hanging_wall_sign")]
    Birch,
    #[serde(rename = "minecraft:jungle_hanging_wall_sign")]
    Jungle,
    #[serde(rename = "minecraft:acacia_hanging_wall_sign")]
    Acacia,
    #[serde(rename = "minecraft:dark_oak_hanging_wall_sign")]
    DarkOak,
    #[serde(rename = "minecraft:mangrove_hanging_wall_sign")]
    Mangrove,
    #[serde(rename = "minecraft:bamboo_hanging_wall_sign")]
    Bamboo,
    #[serde(rename = "minecraft:cherry_hanging_wall_sign")]
    Cherry,
    #[serde(rename = "minecraft:crimson_hanging_wall_sign")]
    Crimson,
    #[serde(rename = "minecraft:warped_hanging_wall_sign")]
    Warped,
}

// Saplings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Sapling {
    #[serde(rename = "minecraft:oak_sapling")]
    Oak,
    #[serde(rename = "minecraft:spruce_sapling")]
    Spruce,
    #[serde(rename = "minecraft:birch_sapling")]
    Birch,
    #[serde(rename = "minecraft:jungle_sapling")]
    Jungle,
    #[serde(rename = "minecraft:acacia_sapling")]
    Acacia,
    #[serde(rename = "minecraft:dark_oak_sapling")]
    DarkOak,
    #[serde(rename = "minecraft:mangrove_propagule")]
    MangrovePropagule,
    #[serde(rename = "minecraft:cherry_sapling")]
    Cherry,
}

// Leaves
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Leaves {
    #[serde(rename = "minecraft:oak_leaves")]
    Oak,
    #[serde(rename = "minecraft:spruce_leaves")]
    Spruce,
    #[serde(rename = "minecraft:birch_leaves")]
    Birch,
    #[serde(rename = "minecraft:jungle_leaves")]
    Jungle,
    #[serde(rename = "minecraft:acacia_leaves")]
    Acacia,
    #[serde(rename = "minecraft:dark_oak_leaves")]
    DarkOak,
    #[serde(rename = "minecraft:mangrove_leaves")]
    Mangrove,
    #[serde(rename = "minecraft:cherry_leaves")]
    Cherry,
    #[serde(rename = "minecraft:azalea_leaves")]
    Azalea,
    #[serde(rename = "minecraft:flowering_azalea_leaves")]
    FloweringAzalea,
}

impl Into<Block> for WoodBlock {
    fn into(self) -> Block {
        BlockID::WoodBlock(self).into()
    }
}

impl Into<BlockID> for WoodBlock {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(self)
    }
}

impl Into<Block> for Planks {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::Planks(self)).into()
    }
}

impl Into<BlockID> for Planks {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::Planks(self))
    }
}

impl Into<Block> for Log {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::Log(self)).into()
    }
}

impl Into<BlockID> for Log {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::Log(self))
    }
}

impl Into<Block> for StrippedLog {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::StrippedLog(self)).into()
    }
}

impl Into<BlockID> for StrippedLog {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::StrippedLog(self))
    }
}

impl Into<Block> for Wood {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::Wood(self)).into()
    }
}

impl Into<BlockID> for Wood {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::Wood(self))
    }
}

impl Into<Block> for StrippedWood {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::StrippedWood(self)).into()
    }
}

impl Into<BlockID> for StrippedWood {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::StrippedWood(self))
    }
}

impl Into<Block> for WoodStairs {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::Stairs(self)).into()
    }
}

impl Into<BlockID> for WoodStairs {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::Stairs(self))
    }
}

impl Into<Block> for WoodSlab {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::Slab(self)).into()
    }
}

impl Into<BlockID> for WoodSlab {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::Slab(self))
    }
}

impl Into<Block> for WoodFence {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::Fence(self)).into()
    }
}

impl Into<BlockID> for WoodFence {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::Fence(self))
    }
}

impl Into<Block> for WoodFenceGate {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::FenceGate(self)).into()
    }
}

impl Into<BlockID> for WoodFenceGate {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::FenceGate(self))
    }
}

impl Into<Block> for WoodDoor {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::Door(self)).into()
    }
}

impl Into<BlockID> for WoodDoor {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::Door(self))
    }
}

impl Into<Block> for WoodTrapdoor {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::Trapdoor(self)).into()
    }
}

impl Into<BlockID> for WoodTrapdoor {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::Trapdoor(self))
    }
}

impl Into<Block> for WoodButton {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::Button(self)).into()
    }
}

impl Into<BlockID> for WoodButton {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::Button(self))
    }
}

impl Into<Block> for WoodPressurePlate {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::PressurePlate(self)).into()
    }
}

impl Into<BlockID> for WoodPressurePlate {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::PressurePlate(self))
    }
}

impl Into<Block> for WoodSign {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::Sign(self)).into()
    }
}

impl Into<BlockID> for WoodSign {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::Sign(self))
    }
}

impl Into<Block> for WoodWallSign {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::WallSign(self)).into()
    }
}

impl Into<BlockID> for WoodWallSign {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::WallSign(self))
    }
}

impl Into<Block> for WoodHangingSign {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::HangingSign(self)).into()
    }
}

impl Into<BlockID> for WoodHangingSign {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::HangingSign(self))
    }
}

impl Into<Block> for WoodHangingWallSign {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::HangingWallSign(self)).into()
    }
}

impl Into<BlockID> for WoodHangingWallSign {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::HangingWallSign(self))
    }
}

impl Into<Block> for Sapling {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::Sapling(self)).into()
    }
}

impl Into<BlockID> for Sapling {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::Sapling(self))
    }
}

impl Into<Block> for Leaves {
    fn into(self) -> Block {
        BlockID::WoodBlock(WoodBlock::Leaves(self)).into()
    }
}

impl Into<BlockID> for Leaves {
    fn into(self) -> BlockID {
        BlockID::WoodBlock(WoodBlock::Leaves(self))
    }
}
