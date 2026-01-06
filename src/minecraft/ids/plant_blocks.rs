use serde_derive::{Deserialize, Serialize};
use crate::minecraft::{Block, BlockID};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlantBlock {
    Flower(Flower),
    TallFlower(TallFlower),
    PottedPlant(PottedPlant),
    Crop(Crop),
    Mushroom(Mushroom),
    Vine(Vine),
    
    // Grass and ferns
    #[serde(rename = "minecraft:short_grass")]
    ShortGrass,
    #[serde(rename = "minecraft:tall_grass")]
    TallGrass,
    #[serde(rename = "minecraft:fern")]
    Fern,
    #[serde(rename = "minecraft:large_fern")]
    LargeFern,
    
    // Other plants
    #[serde(rename = "minecraft:bamboo")]
    Bamboo,
    #[serde(rename = "minecraft:sugar_cane")]
    SugarCane,
    #[serde(rename = "minecraft:cactus")]
    Cactus,
    #[serde(rename = "minecraft:dead_bush")]
    DeadBush,
    #[serde(rename = "minecraft:lily_pad")]
    LilyPad,
    
    // Azalea
    #[serde(rename = "minecraft:azalea")]
    Azalea,
    #[serde(rename = "minecraft:flowering_azalea")]
    FloweringAzalea,
    
    // Dripleaf
    #[serde(rename = "minecraft:small_dripleaf")]
    SmallDripleaf,
    #[serde(rename = "minecraft:big_dripleaf")]
    BigDripleaf,
    #[serde(rename = "minecraft:big_dripleaf_stem")]
    BigDripleafStem,
    
    #[serde(rename = "minecraft:spore_blossom")]
    SporeBlossom,
    #[serde(rename = "minecraft:pink_petals")]
    PinkPetals,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Flower {
    #[serde(rename = "minecraft:dandelion")]
    Dandelion,
    #[serde(rename = "minecraft:poppy")]
    Poppy,
    #[serde(rename = "minecraft:blue_orchid")]
    BlueOrchid,
    #[serde(rename = "minecraft:allium")]
    Allium,
    #[serde(rename = "minecraft:azure_bluet")]
    AzureBluet,
    #[serde(rename = "minecraft:red_tulip")]
    RedTulip,
    #[serde(rename = "minecraft:orange_tulip")]
    OrangeTulip,
    #[serde(rename = "minecraft:white_tulip")]
    WhiteTulip,
    #[serde(rename = "minecraft:pink_tulip")]
    PinkTulip,
    #[serde(rename = "minecraft:oxeye_daisy")]
    OxeyeDaisy,
    #[serde(rename = "minecraft:cornflower")]
    Cornflower,
    #[serde(rename = "minecraft:lily_of_the_valley")]
    LilyOfTheValley,
    #[serde(rename = "minecraft:wither_rose")]
    WitherRose,
    #[serde(rename = "minecraft:torchflower")]
    Torchflower,
    #[serde(rename = "minecraft:pitcher_plant")]
    PitcherPlant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TallFlower {
    #[serde(rename = "minecraft:sunflower")]
    Sunflower,
    #[serde(rename = "minecraft:lilac")]
    Lilac,
    #[serde(rename = "minecraft:rose_bush")]
    RoseBush,
    #[serde(rename = "minecraft:peony")]
    Peony,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PottedPlant {
    #[serde(rename = "minecraft:flower_pot")]
    EmptyFlowerPot,
    #[serde(rename = "minecraft:potted_dandelion")]
    PottedDandelion,
    #[serde(rename = "minecraft:potted_poppy")]
    PottedPoppy,
    #[serde(rename = "minecraft:potted_blue_orchid")]
    PottedBlueOrchid,
    #[serde(rename = "minecraft:potted_allium")]
    PottedAllium,
    #[serde(rename = "minecraft:potted_azure_bluet")]
    PottedAzureBluet,
    #[serde(rename = "minecraft:potted_red_tulip")]
    PottedRedTulip,
    #[serde(rename = "minecraft:potted_orange_tulip")]
    PottedOrangeTulip,
    #[serde(rename = "minecraft:potted_white_tulip")]
    PottedWhiteTulip,
    #[serde(rename = "minecraft:potted_pink_tulip")]
    PottedPinkTulip,
    #[serde(rename = "minecraft:potted_oxeye_daisy")]
    PottedOxeyeDaisy,
    #[serde(rename = "minecraft:potted_cornflower")]
    PottedCornflower,
    #[serde(rename = "minecraft:potted_lily_of_the_valley")]
    PottedLilyOfTheValley,
    #[serde(rename = "minecraft:potted_wither_rose")]
    PottedWitherRose,
    #[serde(rename = "minecraft:potted_sunflower")]
    PottedSunflower,
    #[serde(rename = "minecraft:potted_lilac")]
    PottedLilac,
    #[serde(rename = "minecraft:potted_rose_bush")]
    PottedRoseBush,
    #[serde(rename = "minecraft:potted_peony")]
    PottedPeony,
    #[serde(rename = "minecraft:potted_fern")]
    PottedFern,
    #[serde(rename = "minecraft:potted_cactus")]
    PottedCactus,
    #[serde(rename = "minecraft:potted_bamboo")]
    PottedBamboo,
    #[serde(rename = "minecraft:potted_azalea_bush")]
    PottedAzaleaBush,
    #[serde(rename = "minecraft:potted_flowering_azalea_bush")]
    PottedFloweringAzaleaBush,
    #[serde(rename = "minecraft:potted_torchflower")]
    PottedTorchflower,
    #[serde(rename = "minecraft:potted_pitcher_plant")]
    PottedPitcherPlant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Crop {
    #[serde(rename = "minecraft:wheat")]
    Wheat,
    #[serde(rename = "minecraft:wheat_seeds")]
    WheatSeeds,
    #[serde(rename = "minecraft:carrots")]
    Carrots,
    #[serde(rename = "minecraft:potatoes")]
    Potatoes,
    #[serde(rename = "minecraft:beetroots")]
    Beetroots,
    #[serde(rename = "minecraft:beetroot")]
    Beetroot,
    #[serde(rename = "minecraft:beetroot_block")]
    BeetrootBlock,
    #[serde(rename = "minecraft:beetroot_seeds")]
    BeetrootSeeds,
    #[serde(rename = "minecraft:sweet_berry_bush")]
    SweetBerryBush,
    #[serde(rename = "minecraft:berry_bush")]
    BerryBush,
    #[serde(rename = "minecraft:cocoa")]
    Cocoa,
    #[serde(rename = "minecraft:nether_wart")]
    NetherWart,
    #[serde(rename = "minecraft:pumpkin")]
    Pumpkin,
    #[serde(rename = "minecraft:carved_pumpkin")]
    CarvedPumpkin,
    #[serde(rename = "minecraft:melon")]
    Melon,
    #[serde(rename = "minecraft:melon_stem")]
    MelonStem,
    #[serde(rename = "minecraft:pumpkin_stem")]
    PumpkinStem,
    #[serde(rename = "minecraft:attached_melon_stem")]
    AttachedMelonStem,
    #[serde(rename = "minecraft:attached_pumpkin_stem")]
    AttachedPumpkinStem,
    #[serde(rename = "minecraft:torchflower_crop")]
    TorchflowerCrop,
    #[serde(rename = "minecraft:pitcher_crop")]
    PitcherCrop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mushroom {
    #[serde(rename = "minecraft:red_mushroom")]
    RedMushroom,
    #[serde(rename = "minecraft:brown_mushroom")]
    BrownMushroom,
    #[serde(rename = "minecraft:red_mushroom_block")]
    RedMushroomBlock,
    #[serde(rename = "minecraft:brown_mushroom_block")]
    BrownMushroomBlock,
    #[serde(rename = "minecraft:mushroom_stem")]
    MushroomStem,
    #[serde(rename = "minecraft:huge_red_mushroom")]
    HugeRedMushroom,
    #[serde(rename = "minecraft:huge_brown_mushroom")]
    HugeBrownMushroom,
    #[serde(rename = "minecraft:crimson_fungus")]
    CrimsonFungus,
    #[serde(rename = "minecraft:warped_fungus")]
    WarpedFungus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Vine {
    #[serde(rename = "minecraft:vine")]
    Vine,
    #[serde(rename = "minecraft:glow_lichen")]
    GlowLichen,
    #[serde(rename = "minecraft:cave_vines")]
    CaveVines,
    #[serde(rename = "minecraft:cave_vines_plant")]
    CaveVinesPlant,
    #[serde(rename = "minecraft:crimson_roots")]
    CrimsonRoots,
    #[serde(rename = "minecraft:warped_roots")]
    WarpedRoots,
    #[serde(rename = "minecraft:nether_sprouts")]
    NetherSprouts,
}

impl Into<Block> for PlantBlock {
    fn into(self) -> Block {
        BlockID::PlantBlock(self).into()
    }
}

impl Into<BlockID> for PlantBlock {
    fn into(self) -> BlockID {
        BlockID::PlantBlock(self)
    }
}

impl Into<Block> for Flower {
    fn into(self) -> Block {
        BlockID::PlantBlock(PlantBlock::Flower(self)).into()
    }
}

impl Into<BlockID> for Flower {
    fn into(self) -> BlockID {
        BlockID::PlantBlock(PlantBlock::Flower(self))
    }
}

impl Into<Block> for TallFlower {
    fn into(self) -> Block {
        BlockID::PlantBlock(PlantBlock::TallFlower(self)).into()
    }
}

impl Into<BlockID> for TallFlower {
    fn into(self) -> BlockID {
        BlockID::PlantBlock(PlantBlock::TallFlower(self))
    }
}

impl Into<Block> for PottedPlant {
    fn into(self) -> Block {
        BlockID::PlantBlock(PlantBlock::PottedPlant(self)).into()
    }
}

impl Into<BlockID> for PottedPlant {
    fn into(self) -> BlockID {
        BlockID::PlantBlock(PlantBlock::PottedPlant(self))
    }
}

impl Into<Block> for Crop {
    fn into(self) -> Block {
        BlockID::PlantBlock(PlantBlock::Crop(self)).into()
    }
}

impl Into<BlockID> for Crop {
    fn into(self) -> BlockID {
        BlockID::PlantBlock(PlantBlock::Crop(self))
    }
}

impl Into<Block> for Mushroom {
    fn into(self) -> Block {
        BlockID::PlantBlock(PlantBlock::Mushroom(self)).into()
    }
}

impl Into<BlockID> for Mushroom {
    fn into(self) -> BlockID {
        BlockID::PlantBlock(PlantBlock::Mushroom(self))
    }
}

impl Into<Block> for Vine {
    fn into(self) -> Block {
        BlockID::PlantBlock(PlantBlock::Vine(self)).into()
    }
}

impl Into<BlockID> for Vine {
    fn into(self) -> BlockID {
        BlockID::PlantBlock(PlantBlock::Vine(self))
    }
}
