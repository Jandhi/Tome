use serde_derive::{Deserialize, Serialize};
use crate::minecraft::{Block, BlockID};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColoredBlock {
    Wool(Wool),
    Concrete(Concrete),
    ConcretePowder(ConcretePowder),
    Terracotta(Terracotta),
    StainedGlass(StainedGlass),
    StainedGlassPane(StainedGlassPane),
    Bed(Bed),
    Banner(Banner),
    WallBanner(WallBanner),
    ShulkerBox(ShulkerBox),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Wool {
    #[serde(rename = "minecraft:white_wool")]
    White,
    #[serde(rename = "minecraft:orange_wool")]
    Orange,
    #[serde(rename = "minecraft:magenta_wool")]
    Magenta,
    #[serde(rename = "minecraft:light_blue_wool")]
    LightBlue,
    #[serde(rename = "minecraft:yellow_wool")]
    Yellow,
    #[serde(rename = "minecraft:lime_wool")]
    Lime,
    #[serde(rename = "minecraft:pink_wool")]
    Pink,
    #[serde(rename = "minecraft:gray_wool")]
    Gray,
    #[serde(rename = "minecraft:light_gray_wool")]
    LightGray,
    #[serde(rename = "minecraft:cyan_wool")]
    Cyan,
    #[serde(rename = "minecraft:purple_wool")]
    Purple,
    #[serde(rename = "minecraft:blue_wool")]
    Blue,
    #[serde(rename = "minecraft:brown_wool")]
    Brown,
    #[serde(rename = "minecraft:green_wool")]
    Green,
    #[serde(rename = "minecraft:red_wool")]
    Red,
    #[serde(rename = "minecraft:black_wool")]
    Black,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Concrete {
    #[serde(rename = "minecraft:white_concrete")]
    White,
    #[serde(rename = "minecraft:orange_concrete")]
    Orange,
    #[serde(rename = "minecraft:magenta_concrete")]
    Magenta,
    #[serde(rename = "minecraft:light_blue_concrete")]
    LightBlue,
    #[serde(rename = "minecraft:yellow_concrete")]
    Yellow,
    #[serde(rename = "minecraft:lime_concrete")]
    Lime,
    #[serde(rename = "minecraft:pink_concrete")]
    Pink,
    #[serde(rename = "minecraft:gray_concrete")]
    Gray,
    #[serde(rename = "minecraft:light_gray_concrete")]
    LightGray,
    #[serde(rename = "minecraft:cyan_concrete")]
    Cyan,
    #[serde(rename = "minecraft:purple_concrete")]
    Purple,
    #[serde(rename = "minecraft:blue_concrete")]
    Blue,
    #[serde(rename = "minecraft:brown_concrete")]
    Brown,
    #[serde(rename = "minecraft:green_concrete")]
    Green,
    #[serde(rename = "minecraft:red_concrete")]
    Red,
    #[serde(rename = "minecraft:black_concrete")]
    Black,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConcretePowder {
    #[serde(rename = "minecraft:white_concrete_powder")]
    White,
    #[serde(rename = "minecraft:orange_concrete_powder")]
    Orange,
    #[serde(rename = "minecraft:magenta_concrete_powder")]
    Magenta,
    #[serde(rename = "minecraft:light_blue_concrete_powder")]
    LightBlue,
    #[serde(rename = "minecraft:yellow_concrete_powder")]
    Yellow,
    #[serde(rename = "minecraft:lime_concrete_powder")]
    Lime,
    #[serde(rename = "minecraft:pink_concrete_powder")]
    Pink,
    #[serde(rename = "minecraft:gray_concrete_powder")]
    Gray,
    #[serde(rename = "minecraft:light_gray_concrete_powder")]
    LightGray,
    #[serde(rename = "minecraft:cyan_concrete_powder")]
    Cyan,
    #[serde(rename = "minecraft:purple_concrete_powder")]
    Purple,
    #[serde(rename = "minecraft:blue_concrete_powder")]
    Blue,
    #[serde(rename = "minecraft:brown_concrete_powder")]
    Brown,
    #[serde(rename = "minecraft:green_concrete_powder")]
    Green,
    #[serde(rename = "minecraft:red_concrete_powder")]
    Red,
    #[serde(rename = "minecraft:black_concrete_powder")]
    Black,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Terracotta {
    #[serde(rename = "minecraft:terracotta")]
    Base,
    #[serde(rename = "minecraft:white_terracotta")]
    White,
    #[serde(rename = "minecraft:orange_terracotta")]
    Orange,
    #[serde(rename = "minecraft:magenta_terracotta")]
    Magenta,
    #[serde(rename = "minecraft:light_blue_terracotta")]
    LightBlue,
    #[serde(rename = "minecraft:yellow_terracotta")]
    Yellow,
    #[serde(rename = "minecraft:lime_terracotta")]
    Lime,
    #[serde(rename = "minecraft:pink_terracotta")]
    Pink,
    #[serde(rename = "minecraft:gray_terracotta")]
    Gray,
    #[serde(rename = "minecraft:light_gray_terracotta")]
    LightGray,
    #[serde(rename = "minecraft:cyan_terracotta")]
    Cyan,
    #[serde(rename = "minecraft:purple_terracotta")]
    Purple,
    #[serde(rename = "minecraft:blue_terracotta")]
    Blue,
    #[serde(rename = "minecraft:brown_terracotta")]
    Brown,
    #[serde(rename = "minecraft:green_terracotta")]
    Green,
    #[serde(rename = "minecraft:red_terracotta")]
    Red,
    #[serde(rename = "minecraft:black_terracotta")]
    Black,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StainedGlass {
    #[serde(rename = "minecraft:glass")]
    Clear,
    #[serde(rename = "minecraft:white_stained_glass")]
    White,
    #[serde(rename = "minecraft:orange_stained_glass")]
    Orange,
    #[serde(rename = "minecraft:magenta_stained_glass")]
    Magenta,
    #[serde(rename = "minecraft:light_blue_stained_glass")]
    LightBlue,
    #[serde(rename = "minecraft:yellow_stained_glass")]
    Yellow,
    #[serde(rename = "minecraft:lime_stained_glass")]
    Lime,
    #[serde(rename = "minecraft:pink_stained_glass")]
    Pink,
    #[serde(rename = "minecraft:gray_stained_glass")]
    Gray,
    #[serde(rename = "minecraft:light_gray_stained_glass")]
    LightGray,
    #[serde(rename = "minecraft:cyan_stained_glass")]
    Cyan,
    #[serde(rename = "minecraft:purple_stained_glass")]
    Purple,
    #[serde(rename = "minecraft:blue_stained_glass")]
    Blue,
    #[serde(rename = "minecraft:brown_stained_glass")]
    Brown,
    #[serde(rename = "minecraft:green_stained_glass")]
    Green,
    #[serde(rename = "minecraft:red_stained_glass")]
    Red,
    #[serde(rename = "minecraft:black_stained_glass")]
    Black,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StainedGlassPane {
    #[serde(rename = "minecraft:glass_pane")]
    Clear,
    #[serde(rename = "minecraft:white_stained_glass_pane")]
    White,
    #[serde(rename = "minecraft:orange_stained_glass_pane")]
    Orange,
    #[serde(rename = "minecraft:magenta_stained_glass_pane")]
    Magenta,
    #[serde(rename = "minecraft:light_blue_stained_glass_pane")]
    LightBlue,
    #[serde(rename = "minecraft:yellow_stained_glass_pane")]
    Yellow,
    #[serde(rename = "minecraft:lime_stained_glass_pane")]
    Lime,
    #[serde(rename = "minecraft:pink_stained_glass_pane")]
    Pink,
    #[serde(rename = "minecraft:gray_stained_glass_pane")]
    Gray,
    #[serde(rename = "minecraft:light_gray_stained_glass_pane")]
    LightGray,
    #[serde(rename = "minecraft:cyan_stained_glass_pane")]
    Cyan,
    #[serde(rename = "minecraft:purple_stained_glass_pane")]
    Purple,
    #[serde(rename = "minecraft:blue_stained_glass_pane")]
    Blue,
    #[serde(rename = "minecraft:brown_stained_glass_pane")]
    Brown,
    #[serde(rename = "minecraft:green_stained_glass_pane")]
    Green,
    #[serde(rename = "minecraft:red_stained_glass_pane")]
    Red,
    #[serde(rename = "minecraft:black_stained_glass_pane")]
    Black,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Bed {
    #[serde(rename = "minecraft:bed")]
    Base,
    #[serde(rename = "minecraft:white_bed")]
    White,
    #[serde(rename = "minecraft:orange_bed")]
    Orange,
    #[serde(rename = "minecraft:magenta_bed")]
    Magenta,
    #[serde(rename = "minecraft:light_blue_bed")]
    LightBlue,
    #[serde(rename = "minecraft:yellow_bed")]
    Yellow,
    #[serde(rename = "minecraft:lime_bed")]
    Lime,
    #[serde(rename = "minecraft:pink_bed")]
    Pink,
    #[serde(rename = "minecraft:gray_bed")]
    Gray,
    #[serde(rename = "minecraft:light_gray_bed")]
    LightGray,
    #[serde(rename = "minecraft:cyan_bed")]
    Cyan,
    #[serde(rename = "minecraft:purple_bed")]
    Purple,
    #[serde(rename = "minecraft:blue_bed")]
    Blue,
    #[serde(rename = "minecraft:brown_bed")]
    Brown,
    #[serde(rename = "minecraft:green_bed")]
    Green,
    #[serde(rename = "minecraft:red_bed")]
    Red,
    #[serde(rename = "minecraft:black_bed")]
    Black,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Banner {
    #[serde(rename = "minecraft:white_banner")]
    White,
    #[serde(rename = "minecraft:orange_banner")]
    Orange,
    #[serde(rename = "minecraft:magenta_banner")]
    Magenta,
    #[serde(rename = "minecraft:light_blue_banner")]
    LightBlue,
    #[serde(rename = "minecraft:yellow_banner")]
    Yellow,
    #[serde(rename = "minecraft:lime_banner")]
    Lime,
    #[serde(rename = "minecraft:pink_banner")]
    Pink,
    #[serde(rename = "minecraft:gray_banner")]
    Gray,
    #[serde(rename = "minecraft:light_gray_banner")]
    LightGray,
    #[serde(rename = "minecraft:cyan_banner")]
    Cyan,
    #[serde(rename = "minecraft:purple_banner")]
    Purple,
    #[serde(rename = "minecraft:blue_banner")]
    Blue,
    #[serde(rename = "minecraft:brown_banner")]
    Brown,
    #[serde(rename = "minecraft:green_banner")]
    Green,
    #[serde(rename = "minecraft:red_banner")]
    Red,
    #[serde(rename = "minecraft:black_banner")]
    Black,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WallBanner {
    #[serde(rename = "minecraft:white_wall_banner")]
    White,
    #[serde(rename = "minecraft:orange_wall_banner")]
    Orange,
    #[serde(rename = "minecraft:magenta_wall_banner")]
    Magenta,
    #[serde(rename = "minecraft:light_blue_wall_banner")]
    LightBlue,
    #[serde(rename = "minecraft:yellow_wall_banner")]
    Yellow,
    #[serde(rename = "minecraft:lime_wall_banner")]
    Lime,
    #[serde(rename = "minecraft:pink_wall_banner")]
    Pink,
    #[serde(rename = "minecraft:gray_wall_banner")]
    Gray,
    #[serde(rename = "minecraft:light_gray_wall_banner")]
    LightGray,
    #[serde(rename = "minecraft:cyan_wall_banner")]
    Cyan,
    #[serde(rename = "minecraft:purple_wall_banner")]
    Purple,
    #[serde(rename = "minecraft:blue_wall_banner")]
    Blue,
    #[serde(rename = "minecraft:brown_wall_banner")]
    Brown,
    #[serde(rename = "minecraft:green_wall_banner")]
    Green,
    #[serde(rename = "minecraft:red_wall_banner")]
    Red,
    #[serde(rename = "minecraft:black_wall_banner")]
    Black,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShulkerBox {
    #[serde(rename = "minecraft:shulker_box")]
    Base,
    #[serde(rename = "minecraft:white_shulker_box")]
    White,
    #[serde(rename = "minecraft:orange_shulker_box")]
    Orange,
    #[serde(rename = "minecraft:magenta_shulker_box")]
    Magenta,
    #[serde(rename = "minecraft:light_blue_shulker_box")]
    LightBlue,
    #[serde(rename = "minecraft:yellow_shulker_box")]
    Yellow,
    #[serde(rename = "minecraft:lime_shulker_box")]
    Lime,
    #[serde(rename = "minecraft:pink_shulker_box")]
    Pink,
    #[serde(rename = "minecraft:gray_shulker_box")]
    Gray,
    #[serde(rename = "minecraft:light_gray_shulker_box")]
    LightGray,
    #[serde(rename = "minecraft:cyan_shulker_box")]
    Cyan,
    #[serde(rename = "minecraft:purple_shulker_box")]
    Purple,
    #[serde(rename = "minecraft:blue_shulker_box")]
    Blue,
    #[serde(rename = "minecraft:brown_shulker_box")]
    Brown,
    #[serde(rename = "minecraft:green_shulker_box")]
    Green,
    #[serde(rename = "minecraft:red_shulker_box")]
    Red,
    #[serde(rename = "minecraft:black_shulker_box")]
    Black,
}

impl Into<Block> for ColoredBlock {
    fn into(self) -> Block {
        BlockID::ColoredBlock(self).into()
    }
}

impl Into<BlockID> for ColoredBlock {
    fn into(self) -> BlockID {
        BlockID::ColoredBlock(self)
    }
}

impl Into<Block> for Wool {
    fn into(self) -> Block {
        BlockID::ColoredBlock(ColoredBlock::Wool(self)).into()
    }
}

impl Into<BlockID> for Wool {
    fn into(self) -> BlockID {
        BlockID::ColoredBlock(ColoredBlock::Wool(self))
    }
}

impl Into<Block> for Concrete {
    fn into(self) -> Block {
        BlockID::ColoredBlock(ColoredBlock::Concrete(self)).into()
    }
}

impl Into<BlockID> for Concrete {
    fn into(self) -> BlockID {
        BlockID::ColoredBlock(ColoredBlock::Concrete(self))
    }
}

impl Into<Block> for ConcretePowder {
    fn into(self) -> Block {
        BlockID::ColoredBlock(ColoredBlock::ConcretePowder(self)).into()
    }
}

impl Into<BlockID> for ConcretePowder {
    fn into(self) -> BlockID {
        BlockID::ColoredBlock(ColoredBlock::ConcretePowder(self))
    }
}

impl Into<Block> for Terracotta {
    fn into(self) -> Block {
        BlockID::ColoredBlock(ColoredBlock::Terracotta(self)).into()
    }
}

impl Into<BlockID> for Terracotta {
    fn into(self) -> BlockID {
        BlockID::ColoredBlock(ColoredBlock::Terracotta(self))
    }
}

impl Into<Block> for StainedGlass {
    fn into(self) -> Block {
        BlockID::ColoredBlock(ColoredBlock::StainedGlass(self)).into()
    }
}

impl Into<BlockID> for StainedGlass {
    fn into(self) -> BlockID {
        BlockID::ColoredBlock(ColoredBlock::StainedGlass(self))
    }
}

impl Into<Block> for StainedGlassPane {
    fn into(self) -> Block {
        BlockID::ColoredBlock(ColoredBlock::StainedGlassPane(self)).into()
    }
}

impl Into<BlockID> for StainedGlassPane {
    fn into(self) -> BlockID {
        BlockID::ColoredBlock(ColoredBlock::StainedGlassPane(self))
    }
}

impl Into<Block> for Bed {
    fn into(self) -> Block {
        BlockID::ColoredBlock(ColoredBlock::Bed(self)).into()
    }
}

impl Into<BlockID> for Bed {
    fn into(self) -> BlockID {
        BlockID::ColoredBlock(ColoredBlock::Bed(self))
    }
}

impl Into<Block> for Banner {
    fn into(self) -> Block {
        BlockID::ColoredBlock(ColoredBlock::Banner(self)).into()
    }
}

impl Into<BlockID> for Banner {
    fn into(self) -> BlockID {
        BlockID::ColoredBlock(ColoredBlock::Banner(self))
    }
}

impl Into<Block> for WallBanner {
    fn into(self) -> Block {
        BlockID::ColoredBlock(ColoredBlock::WallBanner(self)).into()
    }
}

impl Into<BlockID> for WallBanner {
    fn into(self) -> BlockID {
        BlockID::ColoredBlock(ColoredBlock::WallBanner(self))
    }
}

impl Into<Block> for ShulkerBox {
    fn into(self) -> Block {
        BlockID::ColoredBlock(ColoredBlock::ShulkerBox(self)).into()
    }
}

impl Into<BlockID> for ShulkerBox {
    fn into(self) -> BlockID {
        BlockID::ColoredBlock(ColoredBlock::ShulkerBox(self))
    }
}
