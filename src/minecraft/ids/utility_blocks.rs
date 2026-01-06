use serde_derive::{Deserialize, Serialize};
use crate::minecraft::{Block, BlockID};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UtilityBlock {
    Furniture(Furniture),
    LightSource(LightSource),
    Storage(Storage),
    CraftingStation(CraftingStation),
    Decoration(Decoration),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Furniture {
    #[serde(rename = "minecraft:crafting_table")]
    CraftingTable,
    #[serde(rename = "minecraft:anvil")]
    Anvil,
    #[serde(rename = "minecraft:chipped_anvil")]
    ChippedAnvil,
    #[serde(rename = "minecraft:damaged_anvil")]
    DamagedAnvil,
    #[serde(rename = "minecraft:enchanting_table")]
    EnchantingTable,
    #[serde(rename = "minecraft:lectern")]
    Lectern,
    #[serde(rename = "minecraft:grindstone")]
    Grindstone,
    #[serde(rename = "minecraft:smithing_table")]
    SmithingTable,
    #[serde(rename = "minecraft:stonecutter")]
    Stonecutter,
    #[serde(rename = "minecraft:loom")]
    Loom,
    #[serde(rename = "minecraft:cartography_table")]
    CartographyTable,
    #[serde(rename = "minecraft:composter")]
    Composter,
    #[serde(rename = "minecraft:bell")]
    Bell,
    #[serde(rename = "minecraft:jukebox")]
    Jukebox,
    #[serde(rename = "minecraft:note_block")]
    NoteBlock,
    #[serde(rename = "minecraft:decorated_pot")]
    DecoratedPot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LightSource {
    #[serde(rename = "minecraft:torch")]
    Torch,
    #[serde(rename = "minecraft:wall_torch")]
    WallTorch,
    #[serde(rename = "minecraft:soul_torch")]
    SoulTorch,
    #[serde(rename = "minecraft:soul_wall_torch")]
    SoulWallTorch,
    #[serde(rename = "minecraft:redstone_torch")]
    RedstoneTorch,
    #[serde(rename = "minecraft:redstone_wall_torch")]
    RedstoneWallTorch,
    #[serde(rename = "minecraft:lantern")]
    Lantern,
    #[serde(rename = "minecraft:soul_lantern")]
    SoulLantern,
    #[serde(rename = "minecraft:campfire")]
    Campfire,
    #[serde(rename = "minecraft:soul_campfire")]
    SoulCampfire,
    #[serde(rename = "minecraft:jack_o_lantern")]
    JackOLantern,
    #[serde(rename = "minecraft:redstone_lamp")]
    RedstoneLamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Storage {
    #[serde(rename = "minecraft:chest")]
    Chest,
    #[serde(rename = "minecraft:trapped_chest")]
    TrappedChest,
    #[serde(rename = "minecraft:ender_chest")]
    EnderChest,
    #[serde(rename = "minecraft:barrel")]
    Barrel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CraftingStation {
    #[serde(rename = "minecraft:furnace")]
    Furnace,
    #[serde(rename = "minecraft:blast_furnace")]
    BlastFurnace,
    #[serde(rename = "minecraft:smoker")]
    Smoker,
    #[serde(rename = "minecraft:brewing_stand")]
    BrewingStand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Decoration {
    #[serde(rename = "minecraft:bookshelf")]
    Bookshelf,
    #[serde(rename = "minecraft:chiseled_bookshelf")]
    ChiseledBookshelf,
    #[serde(rename = "minecraft:painting")]
    Painting,
    #[serde(rename = "minecraft:item_frame")]
    ItemFrame,
    #[serde(rename = "minecraft:glow_item_frame")]
    GlowItemFrame,
    #[serde(rename = "minecraft:armor_stand")]
    ArmorStand,
    #[serde(rename = "minecraft:hay_block")]
    HayBlock,
    #[serde(rename = "minecraft:target")]
    Target,
}

impl Into<Block> for UtilityBlock {
    fn into(self) -> Block {
        BlockID::UtilityBlock(self).into()
    }
}

impl Into<BlockID> for UtilityBlock {
    fn into(self) -> BlockID {
        BlockID::UtilityBlock(self)
    }
}

impl Into<Block> for Furniture {
    fn into(self) -> Block {
        BlockID::UtilityBlock(UtilityBlock::Furniture(self)).into()
    }
}

impl Into<BlockID> for Furniture {
    fn into(self) -> BlockID {
        BlockID::UtilityBlock(UtilityBlock::Furniture(self))
    }
}

impl Into<Block> for LightSource {
    fn into(self) -> Block {
        BlockID::UtilityBlock(UtilityBlock::LightSource(self)).into()
    }
}

impl Into<BlockID> for LightSource {
    fn into(self) -> BlockID {
        BlockID::UtilityBlock(UtilityBlock::LightSource(self))
    }
}

impl Into<Block> for Storage {
    fn into(self) -> Block {
        BlockID::UtilityBlock(UtilityBlock::Storage(self)).into()
    }
}

impl Into<BlockID> for Storage {
    fn into(self) -> BlockID {
        BlockID::UtilityBlock(UtilityBlock::Storage(self))
    }
}

impl Into<Block> for CraftingStation {
    fn into(self) -> Block {
        BlockID::UtilityBlock(UtilityBlock::CraftingStation(self)).into()
    }
}

impl Into<BlockID> for CraftingStation {
    fn into(self) -> BlockID {
        BlockID::UtilityBlock(UtilityBlock::CraftingStation(self))
    }
}

impl Into<Block> for Decoration {
    fn into(self) -> Block {
        BlockID::UtilityBlock(UtilityBlock::Decoration(self)).into()
    }
}

impl Into<BlockID> for Decoration {
    fn into(self) -> BlockID {
        BlockID::UtilityBlock(UtilityBlock::Decoration(self))
    }
}
