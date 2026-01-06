use serde_derive::{Deserialize, Serialize};
use crate::minecraft::{Block, BlockID};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MiscBlock {
    Quartz(Quartz),
    Purpur(Purpur),
    Bricks(Bricks),
    MudBricks(MudBricks),
    Sculk(Sculk),
    Amethyst(Amethyst),
    Special(Special),
    Rail(Rail),
    Obsidian(Obsidian),
    Fluid(Fluid),
    Cauldron(Cauldron),
    Fire(Fire),
    Dripstone(Dripstone),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Quartz {
    #[serde(rename = "minecraft:quartz_block")]
    QuartzBlock,
    #[serde(rename = "minecraft:quartz_pillar")]
    QuartzPillar,
    #[serde(rename = "minecraft:quartz_bricks")]
    QuartzBricks,
    #[serde(rename = "minecraft:chiseled_quartz_block")]
    ChiseledQuartzBlock,
    #[serde(rename = "minecraft:smooth_quartz")]
    SmoothQuartz,
    #[serde(rename = "minecraft:quartz_slab")]
    QuartzSlab,
    #[serde(rename = "minecraft:smooth_quartz_slab")]
    SmoothQuartzSlab,
    #[serde(rename = "minecraft:quartz_stairs")]
    QuartzStairs,
    #[serde(rename = "minecraft:smooth_quartz_stairs")]
    SmoothQuartzStairs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Purpur {
    #[serde(rename = "minecraft:purpur_block")]
    PurpurBlock,
    #[serde(rename = "minecraft:purpur_pillar")]
    PurpurPillar,
    #[serde(rename = "minecraft:purpur_stairs")]
    PurpurStairs,
    #[serde(rename = "minecraft:purpur_slab")]
    PurpurSlab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Bricks {
    #[serde(rename = "minecraft:bricks")]
    Bricks,
    #[serde(rename = "minecraft:brick_slab")]
    BrickSlab,
    #[serde(rename = "minecraft:brick_stairs")]
    BrickStairs,
    #[serde(rename = "minecraft:brick_wall")]
    BrickWall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MudBricks {
    #[serde(rename = "minecraft:packed_mud")]
    PackedMud,
    #[serde(rename = "minecraft:mud_bricks")]
    MudBricks,
    #[serde(rename = "minecraft:mud_brick_slab")]
    MudBrickSlab,
    #[serde(rename = "minecraft:mud_brick_stairs")]
    MudBrickStairs,
    #[serde(rename = "minecraft:mud_brick_wall")]
    MudBrickWall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Sculk {
    #[serde(rename = "minecraft:sculk")]
    Sculk,
    #[serde(rename = "minecraft:sculk_vein")]
    SculkVein,
    #[serde(rename = "minecraft:sculk_catalyst")]
    SculkCatalyst,
    #[serde(rename = "minecraft:sculk_shrieker")]
    SculkShrieker,
    #[serde(rename = "minecraft:sculk_sensor")]
    SculkSensor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Amethyst {
    #[serde(rename = "minecraft:amethyst_block")]
    AmethystBlock,
    #[serde(rename = "minecraft:budding_amethyst")]
    BuddingAmethyst,
    #[serde(rename = "minecraft:small_amethyst_bud")]
    SmallAmethystBud,
    #[serde(rename = "minecraft:medium_amethyst_bud")]
    MediumAmethystBud,
    #[serde(rename = "minecraft:large_amethyst_bud")]
    LargeAmethystBud,
    #[serde(rename = "minecraft:amethyst_cluster")]
    AmethystCluster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Special {
    #[serde(rename = "minecraft:spawner")]
    Spawner,
    #[serde(rename = "minecraft:scaffolding")]
    Scaffolding,
    #[serde(rename = "minecraft:powder_snow")]
    PowderSnow,
    #[serde(rename = "minecraft:light")]
    Light,
    #[serde(rename = "minecraft:barrier")]
    Barrier,
    #[serde(rename = "minecraft:structure_block")]
    StructureBlock,
    #[serde(rename = "minecraft:jigsaw")]
    Jigsaw,
    #[serde(rename = "minecraft:command_block")]
    CommandBlock,
    #[serde(rename = "minecraft:repeating_command_block")]
    RepeatingCommandBlock,
    #[serde(rename = "minecraft:chain_command_block")]
    ChainCommandBlock,
    #[serde(rename = "minecraft:structure_void")]
    StructureVoid,
    #[serde(rename = "minecraft:lightning_rod")]
    LightningRod,
    #[serde(rename = "minecraft:lodestone")]
    Lodestone,
    #[serde(rename = "minecraft:beehive")]
    Beehive,
    #[serde(rename = "minecraft:bee_nest")]
    BeeNest,
    #[serde(rename = "minecraft:cobweb")]
    Cobweb,
    #[serde(rename = "minecraft:bedrock")]
    Bedrock,
    #[serde(rename = "minecraft:chain")]
    Chain,
    #[serde(rename = "minecraft:iron_bars")]
    IronBars,
    #[serde(rename = "minecraft:player_head")]
    PlayerHead,
    #[serde(rename = "minecraft:respawn_anchor")]
    RespawnAnchor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rail {
    #[serde(rename = "minecraft:rail")]
    Rail,
    #[serde(rename = "minecraft:powered_rail")]
    PoweredRail,
    #[serde(rename = "minecraft:detector_rail")]
    DetectorRail,
    #[serde(rename = "minecraft:activator_rail")]
    ActivatorRail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Obsidian {
    #[serde(rename = "minecraft:obsidian")]
    Obsidian,
    #[serde(rename = "minecraft:crying_obsidian")]
    CryingObsidian,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Fluid {
    #[serde(rename = "minecraft:water")]
    Water,
    #[serde(rename = "minecraft:lava")]
    Lava,
    #[serde(rename = "minecraft:bubble_column")]
    BubbleColumn,
    #[serde(rename = "minecraft:kelp")]
    Kelp,
    #[serde(rename = "minecraft:kelp_plant")]
    KelpPlant,
    #[serde(rename = "minecraft:seagrass")]
    Seagrass,
    #[serde(rename = "minecraft:tall_seagrass")]
    TallSeagrass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Cauldron {
    #[serde(rename = "minecraft:cauldron")]
    Cauldron,
    #[serde(rename = "minecraft:water_cauldron")]
    WaterCauldron,
    #[serde(rename = "minecraft:lava_cauldron")]
    LavaCauldron,
    #[serde(rename = "minecraft:powder_snow_cauldron")]
    PowderSnowCauldron,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Fire {
    #[serde(rename = "minecraft:fire")]
    Fire,
    #[serde(rename = "minecraft:soul_fire")]
    SoulFire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dripstone {
    #[serde(rename = "minecraft:dripstone_block")]
    DripstoneBlock,
    #[serde(rename = "minecraft:pointed_dripstone")]
    PointedDripstone,
    #[serde(rename = "minecraft:calcite")]
    Calcite,
}

impl Into<Block> for MiscBlock {
    fn into(self) -> Block {
        BlockID::MiscBlock(self).into()
    }
}

impl Into<BlockID> for MiscBlock {
    fn into(self) -> BlockID {
        BlockID::MiscBlock(self)
    }
}

impl Into<Block> for Quartz {
    fn into(self) -> Block {
        BlockID::MiscBlock(MiscBlock::Quartz(self)).into()
    }
}

impl Into<BlockID> for Quartz {
    fn into(self) -> BlockID {
        BlockID::MiscBlock(MiscBlock::Quartz(self))
    }
}

impl Into<Block> for Purpur {
    fn into(self) -> Block {
        BlockID::MiscBlock(MiscBlock::Purpur(self)).into()
    }
}

impl Into<BlockID> for Purpur {
    fn into(self) -> BlockID {
        BlockID::MiscBlock(MiscBlock::Purpur(self))
    }
}

impl Into<Block> for Bricks {
    fn into(self) -> Block {
        BlockID::MiscBlock(MiscBlock::Bricks(self)).into()
    }
}

impl Into<BlockID> for Bricks {
    fn into(self) -> BlockID {
        BlockID::MiscBlock(MiscBlock::Bricks(self))
    }
}

impl Into<Block> for MudBricks {
    fn into(self) -> Block {
        BlockID::MiscBlock(MiscBlock::MudBricks(self)).into()
    }
}

impl Into<BlockID> for MudBricks {
    fn into(self) -> BlockID {
        BlockID::MiscBlock(MiscBlock::MudBricks(self))
    }
}

impl Into<Block> for Sculk {
    fn into(self) -> Block {
        BlockID::MiscBlock(MiscBlock::Sculk(self)).into()
    }
}

impl Into<BlockID> for Sculk {
    fn into(self) -> BlockID {
        BlockID::MiscBlock(MiscBlock::Sculk(self))
    }
}

impl Into<Block> for Amethyst {
    fn into(self) -> Block {
        BlockID::MiscBlock(MiscBlock::Amethyst(self)).into()
    }
}

impl Into<BlockID> for Amethyst {
    fn into(self) -> BlockID {
        BlockID::MiscBlock(MiscBlock::Amethyst(self))
    }
}

impl Into<Block> for Special {
    fn into(self) -> Block {
        BlockID::MiscBlock(MiscBlock::Special(self)).into()
    }
}

impl Into<BlockID> for Special {
    fn into(self) -> BlockID {
        BlockID::MiscBlock(MiscBlock::Special(self))
    }
}

impl Into<Block> for Rail {
    fn into(self) -> Block {
        BlockID::MiscBlock(MiscBlock::Rail(self)).into()
    }
}

impl Into<BlockID> for Rail {
    fn into(self) -> BlockID {
        BlockID::MiscBlock(MiscBlock::Rail(self))
    }
}

impl Into<Block> for Obsidian {
    fn into(self) -> Block {
        BlockID::MiscBlock(MiscBlock::Obsidian(self)).into()
    }
}

impl Into<BlockID> for Obsidian {
    fn into(self) -> BlockID {
        BlockID::MiscBlock(MiscBlock::Obsidian(self))
    }
}

impl Into<Block> for Fluid {
    fn into(self) -> Block {
        BlockID::MiscBlock(MiscBlock::Fluid(self)).into()
    }
}

impl Into<BlockID> for Fluid {
    fn into(self) -> BlockID {
        BlockID::MiscBlock(MiscBlock::Fluid(self))
    }
}

impl Into<Block> for Cauldron {
    fn into(self) -> Block {
        BlockID::MiscBlock(MiscBlock::Cauldron(self)).into()
    }
}

impl Into<BlockID> for Cauldron {
    fn into(self) -> BlockID {
        BlockID::MiscBlock(MiscBlock::Cauldron(self))
    }
}

impl Into<Block> for Fire {
    fn into(self) -> Block {
        BlockID::MiscBlock(MiscBlock::Fire(self)).into()
    }
}

impl Into<BlockID> for Fire {
    fn into(self) -> BlockID {
        BlockID::MiscBlock(MiscBlock::Fire(self))
    }
}

impl Into<Block> for Dripstone {
    fn into(self) -> Block {
        BlockID::MiscBlock(MiscBlock::Dripstone(self)).into()
    }
}

impl Into<BlockID> for Dripstone {
    fn into(self) -> BlockID {
        BlockID::MiscBlock(MiscBlock::Dripstone(self))
    }
}
