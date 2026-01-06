use serde_derive::{Deserialize, Serialize};

use crate::minecraft::{Block, BlockID};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stone {
    BasicStone(BasicStone),
    Cobblestone(Cobblestone),
    StoneBricks(StoneBricks),
    Sandstone(Sandstone),
    
    // stone variants
    Andesite(Andesite),
    Diorite(Diorite),
    Granite(Granite),
    
    Basalt(Basalt),
    Tuff(Tuff),
    Deepslate(Deepslate),
    Blackstone(Blackstone),

    Infested(Infested),
}

impl Into<Block> for Stone {
    fn into(self) -> Block {
        BlockID::Stone(self).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BasicStone {
    #[serde(rename = "minecraft:stone")]
    Stone,
    #[serde(rename = "minecraft:stone_slab")]
    StoneSlab,
    #[serde(rename = "minecraft:stone_stairs")]
    StoneStairs,
    #[serde(rename = "minecraft:stone_pressure_plate")]
    StonePressurePlate,
    #[serde(rename = "minecraft:stone_button")]
    StoneButton,

    // smooth stone
    #[serde(rename = "minecraft:smooth_stone")]
    SmoothStone,
    #[serde(rename = "minecraft:smooth_stone_slab")]
    SmoothStoneSlab,
}

impl Into<Block> for BasicStone {
    fn into(self) -> Block {
        BlockID::Stone(Stone::BasicStone(self)).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StoneBricks {
    #[serde(rename = "minecraft:stone_bricks")]
    StoneBricks,
    #[serde(rename = "minecraft:stone_brick_slab")]
    StoneBrickSlab,
    #[serde(rename = "minecraft:stone_brick_stairs")]
    StoneBrickStairs,
    #[serde(rename = "minecraft:stone_brick_wall")]
    StoneBrickWall,

    #[serde(rename = "minecraft:chiseled_stone_bricks")]
    ChiseledStoneBricks,
    #[serde(rename = "minecraft:cracked_stone_bricks")]
    CrackedStoneBricks,

    #[serde(rename = "minecraft:mossy_stone_bricks")]
    MossyStoneBricks,
    #[serde(rename = "minecraft:mossy_stone_brick_slab")]
    MossyStoneBrickSlab,
    #[serde(rename = "minecraft:mossy_stone_brick_stairs")]
    MossyStoneBrickStairs,
    #[serde(rename = "minecraft:mossy_stone_brick_wall")]
    MossyStoneBrickWall,
}

impl Into<Block> for StoneBricks {
    fn into(self) -> Block {
        BlockID::Stone(Stone::StoneBricks(self)).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Cobblestone {
    #[serde(rename = "minecraft:cobblestone")]
    Cobblestone,
    #[serde(rename = "minecraft:cobblestone_slab")]
    CobblestoneSlab,
    #[serde(rename = "minecraft:cobblestone_stairs")]
    CobblestoneStairs,
    #[serde(rename = "minecraft:cobblestone_wall")]
    CobblestoneWall,

    #[serde(rename = "minecraft:mossy_cobblestone")]
    MossyCobblestone,
    #[serde(rename = "minecraft:mossy_cobblestone_slab")]
    MossyCobblestoneSlab,
    #[serde(rename = "minecraft:mossy_cobblestone_stairs")]
    MossyCobblestoneStairs,
    #[serde(rename = "minecraft:mossy_cobblestone_wall")]
    MossyCobblestoneWall,
}

impl Into<Block> for Cobblestone {
    fn into(self) -> Block {
        BlockID::Stone(Stone::Cobblestone(self)).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Sandstone {
    #[serde(rename = "minecraft:sandstone")]
    Sandstone,
    #[serde(rename = "minecraft:sandstone_slab")]
    SandstoneSlab,
    #[serde(rename = "minecraft:sandstone_stairs")]
    SandstoneStairs,
    #[serde(rename = "minecraft:sandstone_wall")]
    SandstoneWall,
    #[serde(rename = "minecraft:chiseled_sandstone")]
    ChiseledSandstone,
    #[serde(rename = "minecraft:cut_sandstone")]
    CutSandstone,
    #[serde(rename = "minecraft:cut_sandstone_slab")]
    CutSandstoneSlab,
    #[serde(rename = "minecraft:smooth_sandstone")]
    SmoothSandstone,
    #[serde(rename = "minecraft:smooth_sandstone_slab")]
    SmoothSandstoneSlab,
    #[serde(rename = "minecraft:smooth_sandstone_stairs")]
    SmoothSandstoneStairs,
    
    #[serde(rename = "minecraft:red_sandstone")]
    RedSandstone,
    #[serde(rename = "minecraft:red_sandstone_slab")]
    RedSandstoneSlab,
    #[serde(rename = "minecraft:red_sandstone_stairs")]
    RedSandstoneStairs,
    #[serde(rename = "minecraft:red_sandstone_wall")]
    RedSandstoneWall,
    #[serde(rename = "minecraft:chiseled_red_sandstone")]
    ChiseledRedSandstone,
    #[serde(rename = "minecraft:cut_red_sandstone")]
    CutRedSandstone,
    #[serde(rename = "minecraft:cut_red_sandstone_slab")]
    CutRedSandstoneSlab,
    #[serde(rename = "minecraft:smooth_red_sandstone")]
    SmoothRedSandstone,
    #[serde(rename = "minecraft:smooth_red_sandstone_slab")]
    SmoothRedSandstoneSlab,
    #[serde(rename = "minecraft:smooth_red_sandstone_stairs")]
    SmoothRedSandstoneStairs,
}

impl Into<Block> for Sandstone {
    fn into(self) -> Block {
        BlockID::Stone(Stone::Sandstone(self)).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tuff {
    #[serde(rename = "minecraft:tuff")]
    Tuff,
    #[serde(rename = "minecraft:tuff_slab")]
    TuffSlab,
    #[serde(rename = "minecraft:tuff_stairs")]
    TuffStairs,
    #[serde(rename = "minecraft:tuff_wall")]
    TuffWall,

    #[serde(rename = "minecraft:chiseled_tuff")]
    ChiseledTuff,
    #[serde(rename = "minecraft:chiseled_tuff_bricks")]
    ChiseledTuffBricks,
    
    #[serde(rename = "minecraft:polished_tuff")]
    PolishedTuff,
    #[serde(rename = "minecraft:polished_tuff_slab")]
    PolishedTuffSlab,
    #[serde(rename = "minecraft:polished_tuff_stairs")]
    PolishedTuffStairs,
    #[serde(rename = "minecraft:polished_tuff_wall")]
    PolishedTuffWall,
    
    #[serde(rename = "minecraft:tuff_bricks")]
    TuffBricks,
    #[serde(rename = "minecraft:tuff_brick_slab")]
    TuffBrickSlab,
    #[serde(rename = "minecraft:tuff_brick_stairs")]
    TuffBrickStairs,
    #[serde(rename = "minecraft:tuff_brick_wall")]
    TuffBrickWall,
}

impl Into<Block> for Tuff {
    fn into(self) -> Block {
        BlockID::Stone(Stone::Tuff(self)).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Andesite {
    #[serde(rename = "minecraft:andesite")]
    Andesite,
    #[serde(rename = "minecraft:andesite_slab")]
    AndesiteSlab,
    #[serde(rename = "minecraft:andesite_stairs")]
    AndesiteStairs,
    #[serde(rename = "minecraft:andesite_wall")]
    AndesiteWall,

    
    #[serde(rename = "minecraft:polished_andesite")]
    PolishedAndesite,
    #[serde(rename = "minecraft:polished_andesite_slab")]
    PolishedAndesiteSlab,
    #[serde(rename = "minecraft:polished_andesite_stairs")]
    PolishedAndesiteStairs,
}

impl Into<Block> for Andesite {
    fn into(self) -> Block {
        BlockID::Stone(Stone::Andesite(self)).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Diorite {
    #[serde(rename = "minecraft:diorite")]
    Diorite,
    #[serde(rename = "minecraft:diorite_slab")]
    DioriteSlab,
    #[serde(rename = "minecraft:diorite_stairs")]
    DioriteStairs,
    #[serde(rename = "minecraft:diorite_wall")]
    DioriteWall,

    #[serde(rename = "minecraft:polished_diorite")]
    PolishedDiorite,
    #[serde(rename = "minecraft:polished_diorite_slab")]
    PolishedDioriteSlab,
    #[serde(rename = "minecraft:polished_diorite_stairs")]
    PolishedDioriteStairs,
}

impl Into<Block> for Diorite {
    fn into(self) -> Block {
        BlockID::Stone(Stone::Diorite(self)).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Granite {
    #[serde(rename = "minecraft:granite")]
    Granite,
    #[serde(rename = "minecraft:granite_slab")]
    GraniteSlab,
    #[serde(rename = "minecraft:granite_stairs")]
    GraniteStairs,
    #[serde(rename = "minecraft:granite_wall")]
    GraniteWall,

    #[serde(rename = "minecraft:polished_granite")]
    PolishedGranite,
    #[serde(rename = "minecraft:polished_granite_slab")]
    PolishedGraniteSlab,
    #[serde(rename = "minecraft:polished_granite_stairs")]
    PolishedGraniteStairs,
}

impl Into<Block> for Granite {
    fn into(self) -> Block {
        BlockID::Stone(Stone::Granite(self)).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Basalt {
    #[serde(rename = "minecraft:basalt")]
    Basalt,
    #[serde(rename = "minecraft:smooth_basalt")]
    SmoothBasalt,
    #[serde(rename = "minecraft:polished_basalt")]
    PolishedBasalt,
}

impl Into<Block> for Basalt {
    fn into(self) -> Block {
        BlockID::Stone(Stone::Basalt(self)).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Infested { 
    #[serde(rename = "minecraft:infested_stone")]
    InfestedStone,
    #[serde(rename = "minecraft:infested_cobblestone")]
    InfestedCobblestone,
    #[serde(rename = "minecraft:infested_stone_bricks")]
    InfestedStoneBricks,
    #[serde(rename = "minecraft:infested_mossy_stone_bricks")]
    InfestedMossyStoneBricks,
    #[serde(rename = "minecraft:infested_cracked_stone_bricks")]
    InfestedCrackedStoneBricks,
    #[serde(rename = "minecraft:infested_chiseled_stone_bricks")]
    InfestedChiseledStoneBricks,
    #[serde(rename = "minecraft:infested_deepslate")]
    InfestedDeepslate,
}

impl Into<Block> for Infested {
    fn into(self) -> Block {
        BlockID::Stone(Stone::Infested(self)).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Deepslate {
    #[serde(rename = "minecraft:deepslate")]
    Deepslate,
    #[serde(rename = "minecraft:cobbled_deepslate")]
    CobbledDeepslate,
    #[serde(rename = "minecraft:cobbled_deepslate_slab")]
    CobbledDeepslateSlab,
    #[serde(rename = "minecraft:cobbled_deepslate_stairs")]
    CobbledDeepslateStairs,
    #[serde(rename = "minecraft:cobbled_deepslate_wall")]
    CobbledDeepslateWall,
    
    #[serde(rename = "minecraft:polished_deepslate")]
    PolishedDeepslate,
    #[serde(rename = "minecraft:polished_deepslate_slab")]
    PolishedDeepslateSlab,
    #[serde(rename = "minecraft:polished_deepslate_stairs")]
    PolishedDeepslateStairs,
    #[serde(rename = "minecraft:polished_deepslate_wall")]
    PolishedDeepslateWall,
    
    #[serde(rename = "minecraft:deepslate_bricks")]
    DeepslateBricks,
    #[serde(rename = "minecraft:deepslate_brick_slab")]
    DeepslateBrickSlab,
    #[serde(rename = "minecraft:deepslate_brick_stairs")]
    DeepslateBrickStairs,
    #[serde(rename = "minecraft:deepslate_brick_wall")]
    DeepslateBrickWall,
    #[serde(rename = "minecraft:cracked_deepslate_bricks")]
    CrackedDeepslateBricks,
    
    #[serde(rename = "minecraft:deepslate_tiles")]
    DeepslateTiles,
    #[serde(rename = "minecraft:deepslate_tile_slab")]
    DeepslateTileSlab,
    #[serde(rename = "minecraft:deepslate_tile_stairs")]
    DeepslateTileStairs,
    #[serde(rename = "minecraft:deepslate_tile_wall")]
    DeepslateTileWall,
    #[serde(rename = "minecraft:cracked_deepslate_tiles")]
    CrackedDeepslateTiles,
    
    #[serde(rename = "minecraft:chiseled_deepslate")]
    ChiseledDeepslate,
    #[serde(rename = "minecraft:reinforced_deepslate")]
    ReinforcedDeepslate,
}

impl Into<Block> for Deepslate {
    fn into(self) -> Block {
        BlockID::Stone(Stone::Deepslate(self)).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Blackstone {
    #[serde(rename = "minecraft:blackstone")]
    Blackstone,
    #[serde(rename = "minecraft:blackstone_slab")]
    BlackstoneSlab,
    #[serde(rename = "minecraft:blackstone_stairs")]
    BlackstoneStairs,
    #[serde(rename = "minecraft:blackstone_wall")]
    BlackstoneWall,
    
    #[serde(rename = "minecraft:polished_blackstone")]
    PolishedBlackstone,
    #[serde(rename = "minecraft:polished_blackstone_slab")]
    PolishedBlackstoneSlab,
    #[serde(rename = "minecraft:polished_blackstone_stairs")]
    PolishedBlackstoneStairs,
    #[serde(rename = "minecraft:polished_blackstone_wall")]
    PolishedBlackstoneWall,
    #[serde(rename = "minecraft:polished_blackstone_button", alias = "minecraft:blackstone_button")]
    PolishedBlackstoneButton,
    #[serde(rename = "minecraft:polished_blackstone_pressure_plate", alias = "minecraft:blackstone_pressure_plate")]
    PolishedBlackstonePressurePlate,
    
    #[serde(rename = "minecraft:polished_blackstone_bricks")]
    PolishedBlackstoneBricks,
    #[serde(rename = "minecraft:polished_blackstone_brick_slab")]
    PolishedBlackstoneBrickSlab,
    #[serde(rename = "minecraft:polished_blackstone_brick_stairs")]
    PolishedBlackstoneBrickStairs,
    #[serde(rename = "minecraft:polished_blackstone_brick_wall")]
    PolishedBlackstoneBrickWall,
    #[serde(rename = "minecraft:cracked_polished_blackstone_bricks")]
    CrackedPolishedBlackstoneBricks,
    
    #[serde(rename = "minecraft:chiseled_polished_blackstone")]
    ChiseledPolishedBlackstone,
    #[serde(rename = "minecraft:gilded_blackstone")]
    GildedBlackstone,
}

impl Into<Block> for Blackstone {
    fn into(self) -> Block {
        BlockID::Stone(Stone::Blackstone(self)).into()
    }
}

impl Into<BlockID> for Stone {
    fn into(self) -> BlockID {
        BlockID::Stone(self)
    }
}

impl Into<BlockID> for BasicStone {
    fn into(self) -> BlockID {
        BlockID::Stone(Stone::BasicStone(self))
    }
}

impl Into<BlockID> for StoneBricks {
    fn into(self) -> BlockID {
        BlockID::Stone(Stone::StoneBricks(self))
    }
}

impl Into<BlockID> for Cobblestone {
    fn into(self) -> BlockID {
        BlockID::Stone(Stone::Cobblestone(self))
    }
}

impl Into<BlockID> for Sandstone {
    fn into(self) -> BlockID {
        BlockID::Stone(Stone::Sandstone(self))
    }
}

impl Into<BlockID> for Tuff {
    fn into(self) -> BlockID {
        BlockID::Stone(Stone::Tuff(self))
    }
}

impl Into<BlockID> for Andesite {
    fn into(self) -> BlockID {
        BlockID::Stone(Stone::Andesite(self))
    }
}

impl Into<BlockID> for Diorite {
    fn into(self) -> BlockID {
        BlockID::Stone(Stone::Diorite(self))
    }
}

impl Into<BlockID> for Granite {
    fn into(self) -> BlockID {
        BlockID::Stone(Stone::Granite(self))
    }
}

impl Into<BlockID> for Basalt {
    fn into(self) -> BlockID {
        BlockID::Stone(Stone::Basalt(self))
    }
}

impl Into<BlockID> for Infested {
    fn into(self) -> BlockID {
        BlockID::Stone(Stone::Infested(self))
    }
}

impl Into<BlockID> for Deepslate {
    fn into(self) -> BlockID {
        BlockID::Stone(Stone::Deepslate(self))
    }
}

impl Into<BlockID> for Blackstone {
    fn into(self) -> BlockID {
        BlockID::Stone(Stone::Blackstone(self))
    }
}
