use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlockID {

    // === AIR ===
    #[serde(rename = "minecraft:air")]
    Air,
    #[serde(rename = "minecraft:cave_air")]
    CaveAir,
    #[serde(rename = "minecraft:void_air")]
    VoidAir,

    // === GLOWSTONE & CO. ===
    #[serde(rename = "minecraft:glowstone")]
    Glowstone,
    #[serde(rename = "minecraft:shroomlight")]
    Shroomlight,
    #[serde(rename = "minecraft:sea_lantern")]
    SeaLantern,
    #[serde(rename = "minecraft:jack_o_lantern")]
    JackOLantern,
    #[serde(rename = "minecraft:redstone_lamp")]
    RedstoneLamp,

    // === NATURAL BLOCKS ===
    #[serde(rename = "minecraft:grass_block")]
    GrassBlock,
    #[serde(rename = "minecraft:dirt")]
    Dirt,
    #[serde(rename = "minecraft:coarse_dirt")]
    CoarseDirt,
    #[serde(rename = "minecraft:podzol")]
    Podzol,
    #[serde(rename = "minecraft:rooted_dirt")]
    RootedDirt,
    #[serde(rename = "minecraft:mud")]
    Mud,
    #[serde(rename = "minecraft:muddy_mangrove_roots")]
    MuddyMangroveRoots,
    #[serde(rename = "minecraft:mycelium")]
    Mycelium,
    #[serde(rename = "minecraft:grass")]
    Grass,
    #[serde(rename = "minecraft:tall_grass")]
    TallGrass,
    #[serde(rename = "minecraft:fern")]
    Fern,
    #[serde(rename = "minecraft:large_fern")]
    LargeFern,
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
    #[serde(rename = "minecraft:clay")]
    Clay,
    #[serde(rename = "minecraft:gravel")]
    Gravel,
    #[serde(rename = "minecraft:sand")]
    Sand,
    #[serde(rename = "minecraft:red_sand")]
    RedSand,
    #[serde(rename = "minecraft:sandstone")]
    Sandstone,
    #[serde(rename = "minecraft:chiseled_sandstone")]
    ChiseledSandstone,
    #[serde(rename = "minecraft:cut_sandstone")]
    CutSandstone,
    #[serde(rename = "minecraft:smooth_sandstone")]
    SmoothSandstone,
    #[serde(rename = "minecraft:red_sandstone")]
    RedSandstone,
    #[serde(rename = "minecraft:chiseled_red_sandstone")]
    ChiseledRedSandstone,
    #[serde(rename = "minecraft:cut_red_sandstone")]
    CutRedSandstone,
    #[serde(rename = "minecraft:smooth_red_sandstone")]
    SmoothRedSandstone,
    #[serde(rename = "minecraft:dripstone_block")]
    DripstoneBlock,
    #[serde(rename = "minecraft:pointed_dripstone")]
    PointedDripstone,
    #[serde(rename = "minecraft:calcite")]
    Calcite,
    #[serde(rename = "minecraft:tuff")]
    Tuff,
    #[serde(rename = "minecraft:tuff_bricks")]
    TuffBricks,
    #[serde(rename = "minecraft:chiseled_tuff")]
    ChiseledTuff,
    #[serde(rename = "minecraft:polished_tuff")]
    PolishedTuff,
    #[serde(rename = "minecraft:tuff_brick_slab")]
    TuffBrickSlab,
    #[serde(rename = "minecraft:tuff_brick_stairs")]
    TuffBrickStairs,
    #[serde(rename = "minecraft:tuff_brick_wall")]
    TuffBrickWall,
    #[serde(rename = "minecraft:polished_tuff_slab")]
    PolishedTuffSlab,
    #[serde(rename = "minecraft:polished_tuff_stairs")]
    PolishedTuffStairs,
    #[serde(rename = "minecraft:polished_tuff_wall")]
    PolishedTuffWall,
    #[serde(rename = "minecraft:netherrack")]
    Netherrack,

    // === STONE & VARIANTS ===
    #[serde(rename = "minecraft:stone")]
    Stone,
    #[serde(rename = "minecraft:smooth_stone")]
    SmoothStone,
    #[serde(rename = "minecraft:stone_slab")]
    StoneSlab,
    #[serde(rename = "minecraft:smooth_stone_slab")]
    SmoothStoneSlab,
    #[serde(rename = "minecraft:stone_stairs")]
    StoneStairs,
    #[serde(rename = "minecraft:cobblestone")]
    Cobblestone,
    #[serde(rename = "minecraft:mossy_cobblestone")]
    MossyCobblestone,
    #[serde(rename = "minecraft:cobblestone_stairs")]
    CobblestoneStairs,
    #[serde(rename = "minecraft:mossy_cobblestone_stairs")]
    MossyCobblestoneStairs,
    #[serde(rename = "minecraft:cobblestone_slab")]
    CobblestoneSlab,
    #[serde(rename = "minecraft:mossy_cobblestone_slab")]
    MossyCobblestoneSlab,
    #[serde(rename = "minecraft:cobblestone_wall")]
    CobblestoneWall,
    #[serde(rename = "minecraft:mossy_cobblestone_wall")]
    MossyCobblestoneWall,
    #[serde(rename = "minecraft:stone_bricks")]
    StoneBricks,
    #[serde(rename = "minecraft:mossy_stone_bricks")]
    MossyStoneBricks,
    #[serde(rename = "minecraft:cracked_stone_bricks")]
    CrackedStoneBricks,
    #[serde(rename = "minecraft:chiseled_stone_bricks")]
    ChiseledStoneBricks,
    #[serde(rename = "minecraft:stone_brick_slab")]
    StoneBrickSlab,
    #[serde(rename = "minecraft:stone_brick_stairs")]
    StoneBrickStairs,
    #[serde(rename = "minecraft:stone_brick_wall")]
    StoneBrickWall,
    #[serde(rename = "minecraft:mossy_stone_brick_slab")]
    MossyStoneBrickSlab,
    #[serde(rename = "minecraft:mossy_stone_brick_stairs")]
    MossyStoneBrickStairs,
    #[serde(rename = "minecraft:mossy_stone_brick_wall")]
    MossyStoneBrickWall,
    #[serde(rename = "minecraft:andesite")]
    Andesite,
    #[serde(rename = "minecraft:polished_andesite")]
    PolishedAndesite,
    #[serde(rename = "minecraft:andesite_slab")]
    AndesiteSlab,
    #[serde(rename = "minecraft:andesite_stairs")]
    AndesiteStairs,
    #[serde(rename = "minecraft:andesite_wall")]
    AndesiteWall,
    #[serde(rename = "minecraft:polished_andesite_slab")]
    PolishedAndesiteSlab,
    #[serde(rename = "minecraft:polished_andesite_stairs")]
    PolishedAndesiteStairs,
    #[serde(rename = "minecraft:diorite")]
    Diorite,
    #[serde(rename = "minecraft:polished_diorite")]
    PolishedDiorite,
    #[serde(rename = "minecraft:diorite_slab")]
    DioriteSlab,
    #[serde(rename = "minecraft:diorite_stairs")]
    DioriteStairs,
    #[serde(rename = "minecraft:diorite_wall")]
    DioriteWall,
    #[serde(rename = "minecraft:polished_diorite_slab")]
    PolishedDioriteSlab,
    #[serde(rename = "minecraft:polished_diorite_stairs")]
    PolishedDioriteStairs,
    #[serde(rename = "minecraft:granite")]
    Granite,
    #[serde(rename = "minecraft:polished_granite")]
    PolishedGranite,
    #[serde(rename = "minecraft:granite_slab")]
    GraniteSlab,
    #[serde(rename = "minecraft:granite_stairs")]
    GraniteStairs,
    #[serde(rename = "minecraft:granite_wall")]
    GraniteWall,
    #[serde(rename = "minecraft:polished_granite_slab")]
    PolishedGraniteSlab,
    #[serde(rename = "minecraft:polished_granite_stairs")]
    PolishedGraniteStairs,
    #[serde(rename = "minecraft:basalt")]
    Basalt,
    #[serde(rename = "minecraft:polished_basalt")]
    PolishedBasalt,
    #[serde(rename = "minecraft:smooth_basalt")]
    SmoothBasalt,

    // === DEEPSLATE ===
    #[serde(rename = "minecraft:deepslate")]
    Deepslate,
    #[serde(rename = "minecraft:cobbled_deepslate")]
    CobbledDeepslate,
    #[serde(rename = "minecraft:polished_deepslate")]
    PolishedDeepslate,
    #[serde(rename = "minecraft:cobbled_deepslate_slab")]
    CobbledDeepslateSlab,
    #[serde(rename = "minecraft:polished_deepslate_slab")]
    PolishedDeepslateSlab,
    #[serde(rename = "minecraft:deepslate_brick_slab")]
    DeepslateBrickSlab,
    #[serde(rename = "minecraft:deepslate_tile_slab")]
    DeepslateTileSlab,
    #[serde(rename = "minecraft:cobbled_deepslate_stairs")]
    CobbledDeepslateStairs,
    #[serde(rename = "minecraft:polished_deepslate_stairs")]
    PolishedDeepslateStairs,
    #[serde(rename = "minecraft:deepslate_brick_stairs")]
    DeepslateBrickStairs,
    #[serde(rename = "minecraft:deepslate_tile_stairs")]
    DeepslateTileStairs,
    #[serde(rename = "minecraft:cobbled_deepslate_wall")]
    CobbledDeepslateWall,
    #[serde(rename = "minecraft:deepslate_brick_wall")]
    DeepslateBrickWall,
    #[serde(rename = "minecraft:deepslate_tile_wall")]
    DeepslateTileWall,
    #[serde(rename = "minecraft:deepslate_bricks")]
    DeepslateBricks,
    #[serde(rename = "minecraft:deepslate_tiles")]
    DeepslateTiles,
    #[serde(rename = "minecraft:chiseled_deepslate")]
    ChiseledDeepslate,
    #[serde(rename = "minecraft:cracked_deepslate_bricks")]
    CrackedDeepslateBricks,
    #[serde(rename = "minecraft:cracked_deepslate_tiles")]
    CrackedDeepslateTiles,
    #[serde(rename = "minecraft:reinforced_deepslate")]
    ReinforcedDeepslate,

    // === BLACKSTONE ===
    #[serde(rename = "minecraft:blackstone")]
    Blackstone,
    #[serde(rename = "minecraft:polished_blackstone")]
    PolishedBlackstone,
    #[serde(rename = "minecraft:polished_blackstone_bricks")]
    PolishedBlackstoneBricks,
    #[serde(rename = "minecraft:cracked_polished_blackstone_bricks")]
    CrackedPolishedBlackstoneBricks,
    #[serde(rename = "minecraft:chiseled_polished_blackstone")]
    ChiseledPolishedBlackstone,
    #[serde(rename = "minecraft:blackstone_slab")]
    BlackstoneSlab,
    #[serde(rename = "minecraft:blackstone_stairs")]
    BlackstoneStairs,
    #[serde(rename = "minecraft:blackstone_wall")]
    BlackstoneWall,
    #[serde(rename = "minecraft:polished_blackstone_slab")]
    PolishedBlackstoneSlab,
    #[serde(rename = "minecraft:polished_blackstone_stairs")]
    PolishedBlackstoneStairs,
    #[serde(rename = "minecraft:polished_blackstone_wall")]
    PolishedBlackstoneWall,
    #[serde(rename = "minecraft:polished_blackstone_brick_slab")]
    PolishedBlackstoneBrickSlab,
    #[serde(rename = "minecraft:polished_blackstone_brick_stairs")]
    PolishedBlackstoneBrickStairs,
    #[serde(rename = "minecraft:polished_blackstone_brick_wall")]
    PolishedBlackstoneBrickWall,

    // === ORES ===
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
    #[serde(rename = "minecraft:nether_gold_ore")]
    NetherGoldOre,
    #[serde(rename = "minecraft:nether_quartz_ore")]
    NetherQuartzOre,
    #[serde(rename = "minecraft:ancient_debris")]
    AncientDebris,

    // === WOOL ===
    #[serde(rename = "minecraft:white_wool")]
    WhiteWool,
    #[serde(rename = "minecraft:orange_wool")]
    OrangeWool,
    #[serde(rename = "minecraft:magenta_wool")]
    MagentaWool,
    #[serde(rename = "minecraft:light_blue_wool")]
    LightBlueWool,
    #[serde(rename = "minecraft:yellow_wool")]
    YellowWool,
    #[serde(rename = "minecraft:lime_wool")]
    LimeWool,
    #[serde(rename = "minecraft:pink_wool")]
    PinkWool,
    #[serde(rename = "minecraft:gray_wool")]
    GrayWool,
    #[serde(rename = "minecraft:light_gray_wool")]
    LightGrayWool,
    #[serde(rename = "minecraft:cyan_wool")]
    CyanWool,
    #[serde(rename = "minecraft:purple_wool")]
    PurpleWool,
    #[serde(rename = "minecraft:blue_wool")]
    BlueWool,
    #[serde(rename = "minecraft:brown_wool")]
    BrownWool,
    #[serde(rename = "minecraft:green_wool")]
    GreenWool,
    #[serde(rename = "minecraft:red_wool")]
    RedWool,
    #[serde(rename = "minecraft:black_wool")]
    BlackWool,

    // === WOOD & PLANKS ===
    #[serde(rename = "minecraft:oak_planks")]
    OakPlanks,
    #[serde(rename = "minecraft:spruce_planks")]
    SprucePlanks,
    #[serde(rename = "minecraft:birch_planks")]
    BirchPlanks,
    #[serde(rename = "minecraft:jungle_planks")]
    JunglePlanks,
    #[serde(rename = "minecraft:acacia_planks")]
    AcaciaPlanks,
    #[serde(rename = "minecraft:dark_oak_planks")]
    DarkOakPlanks,
    #[serde(rename = "minecraft:mangrove_planks")]
    MangrovePlanks,
    #[serde(rename = "minecraft:bamboo_planks")]
    BambooPlanks,
    #[serde(rename = "minecraft:bamboo_mosaic")]
    BambooMosaic,
    #[serde(rename = "minecraft:cherry_planks")]
    CherryPlanks,
    #[serde(rename = "minecraft:crimson_planks")]
    CrimsonPlanks,
    #[serde(rename = "minecraft:warped_planks")]
    WarpedPlanks,

    // === LOGS ===
    #[serde(rename = "minecraft:oak_log")]
    OakLog,
    #[serde(rename = "minecraft:spruce_log")]
    SpruceLog,
    #[serde(rename = "minecraft:birch_log")]
    BirchLog,
    #[serde(rename = "minecraft:jungle_log")]
    JungleLog,
    #[serde(rename = "minecraft:acacia_log")]
    AcaciaLog,
    #[serde(rename = "minecraft:dark_oak_log")]
    DarkOakLog,
    #[serde(rename = "minecraft:mangrove_log")]
    MangroveLog,
    #[serde(rename = "minecraft:cherry_log")]
    CherryLog,
    #[serde(rename = "minecraft:crimson_stem")]
    CrimsonStem,
    #[serde(rename = "minecraft:warped_stem")]
    WarpedStem,

    // === WOOD (bark on all sides) ===
    #[serde(rename = "minecraft:oak_wood")]
    OakWood,
    #[serde(rename = "minecraft:spruce_wood")]
    SpruceWood,
    #[serde(rename = "minecraft:birch_wood")]
    BirchWood,
    #[serde(rename = "minecraft:jungle_wood")]
    JungleWood,
    #[serde(rename = "minecraft:acacia_wood")]
    AcaciaWood,
    #[serde(rename = "minecraft:dark_oak_wood")]
    DarkOakWood,
    #[serde(rename = "minecraft:mangrove_wood")]
    MangroveWood,
    #[serde(rename = "minecraft:cherry_wood")]
    CherryWood,
    #[serde(rename = "minecraft:crimson_hyphae")]
    CrimsonHyphae,
    #[serde(rename = "minecraft:warped_hyphae")]
    WarpedHyphae,

    // === STRIPPED LOGS ===
    #[serde(rename = "minecraft:stripped_oak_log")]
    StrippedOakLog,
    #[serde(rename = "minecraft:stripped_spruce_log")]
    StrippedSpruceLog,
    #[serde(rename = "minecraft:stripped_birch_log")]
    StrippedBirchLog,
    #[serde(rename = "minecraft:stripped_jungle_log")]
    StrippedJungleLog,
    #[serde(rename = "minecraft:stripped_acacia_log")]
    StrippedAcaciaLog,
    #[serde(rename = "minecraft:stripped_dark_oak_log")]
    StrippedDarkOakLog,
    #[serde(rename = "minecraft:stripped_mangrove_log")]
    StrippedMangroveLog,
    #[serde(rename = "minecraft:stripped_cherry_log")]
    StrippedCherryLog,
    #[serde(rename = "minecraft:stripped_crimson_stem")]
    StrippedCrimsonStem,
    #[serde(rename = "minecraft:stripped_warped_stem")]
    StrippedWarpedStem,

    // === STRIPPED WOOD ===
    #[serde(rename = "minecraft:stripped_oak_wood")]
    StrippedOakWood,
    #[serde(rename = "minecraft:stripped_spruce_wood")]
    StrippedSpruceWood,
    #[serde(rename = "minecraft:stripped_birch_wood")]
    StrippedBirchWood,
    #[serde(rename = "minecraft:stripped_jungle_wood")]
    StrippedJungleWood,
    #[serde(rename = "minecraft:stripped_acacia_wood")]
    StrippedAcaciaWood,
    #[serde(rename = "minecraft:stripped_dark_oak_wood")]
    StrippedDarkOakWood,
    #[serde(rename = "minecraft:stripped_mangrove_wood")]
    StrippedMangroveWood,
    #[serde(rename = "minecraft:stripped_cherry_wood")]
    StrippedCherryWood,
    #[serde(rename = "minecraft:stripped_crimson_hyphae")]
    StrippedCrimsonHyphae,
    #[serde(rename = "minecraft:stripped_warped_hyphae")]
    StrippedWarpedHyphae,

    // === STAIRS ===
    #[serde(rename = "minecraft:oak_stairs")]
    OakStairs,
    #[serde(rename = "minecraft:spruce_stairs")]
    SpruceStairs,
    #[serde(rename = "minecraft:birch_stairs")]
    BirchStairs,
    #[serde(rename = "minecraft:jungle_stairs")]
    JungleStairs,
    #[serde(rename = "minecraft:acacia_stairs")]
    AcaciaStairs,
    #[serde(rename = "minecraft:dark_oak_stairs")]
    DarkOakStairs,
    #[serde(rename = "minecraft:mangrove_stairs")]
    MangroveStairs,
    #[serde(rename = "minecraft:bamboo_stairs")]
    BambooStairs,
    #[serde(rename = "minecraft:bamboo_mosaic_stairs")]
    BambooMosaicStairs,
    #[serde(rename = "minecraft:cherry_stairs")]
    CherryStairs,
    #[serde(rename = "minecraft:crimson_stairs")]
    CrimsonStairs,
    #[serde(rename = "minecraft:warped_stairs")]
    WarpedStairs,
    #[serde(rename = "minecraft:sandstone_stairs")]
    SandstoneStairs,
    #[serde(rename = "minecraft:smooth_sandstone_stairs")]
    SmoothSandstoneStairs,
    #[serde(rename = "minecraft:red_sandstone_stairs")]
    RedSandstoneStairs,
    #[serde(rename = "minecraft:smooth_red_sandstone_stairs")]
    SmoothRedSandstoneStairs,

    // === SLABS ===
    #[serde(rename = "minecraft:oak_slab")]
    OakSlab,
    #[serde(rename = "minecraft:spruce_slab")]
    SpruceSlab,
    #[serde(rename = "minecraft:birch_slab")]
    BirchSlab,
    #[serde(rename = "minecraft:jungle_slab")]
    JungleSlab,
    #[serde(rename = "minecraft:acacia_slab")]
    AcaciaSlab,
    #[serde(rename = "minecraft:dark_oak_slab")]
    DarkOakSlab,
    #[serde(rename = "minecraft:mangrove_slab")]
    MangroveSlab,
    #[serde(rename = "minecraft:bamboo_slab")]
    BambooSlab,
    #[serde(rename = "minecraft:bamboo_mosaic_slab")]
    BambooMosaicSlab,
    #[serde(rename = "minecraft:cherry_slab")]
    CherrySlab,
    #[serde(rename = "minecraft:crimson_slab")]
    CrimsonSlab,
    #[serde(rename = "minecraft:warped_slab")]
    WarpedSlab,
    #[serde(rename = "minecraft:sandstone_slab")]
    SandstoneSlab,
    #[serde(rename = "minecraft:cut_sandstone_slab")]
    CutSandstoneSlab,
    #[serde(rename = "minecraft:smooth_sandstone_slab")]
    SmoothSandstoneSlab,
    #[serde(rename = "minecraft:red_sandstone_slab")]
    RedSandstoneSlab,
    #[serde(rename = "minecraft:cut_red_sandstone_slab")]
    CutRedSandstoneSlab,
    #[serde(rename = "minecraft:smooth_red_sandstone_slab")]
    SmoothRedSandstoneSlab,


    // === WALLS ===
    #[serde(rename = "minecraft:sandstone_wall")]
    SandstoneWall,
    #[serde(rename = "minecraft:red_sandstone_wall")]
    RedSandstoneWall,


    // === FENCES ===
    #[serde(rename = "minecraft:oak_fence")]
    OakFence,
    #[serde(rename = "minecraft:spruce_fence")]
    SpruceFence,
    #[serde(rename = "minecraft:birch_fence")]
    BirchFence,
    #[serde(rename = "minecraft:jungle_fence")]
    JungleFence,
    #[serde(rename = "minecraft:acacia_fence")]
    AcaciaFence,
    #[serde(rename = "minecraft:dark_oak_fence")]
    DarkOakFence,
    #[serde(rename = "minecraft:mangrove_fence")]
    MangroveFence,
    #[serde(rename = "minecraft:bamboo_fence")]
    BambooFence,
    #[serde(rename = "minecraft:bamboo_mosaic_fence")]
    BambooMosaicFence,
    #[serde(rename = "minecraft:cherry_fence")]
    CherryFence,
    #[serde(rename = "minecraft:crimson_fence")]
    CrimsonFence,
    #[serde(rename = "minecraft:warped_fence")]
    WarpedFence,

    // === FENCE GATES ===
    #[serde(rename = "minecraft:oak_fence_gate")]
    OakFenceGate,
    #[serde(rename = "minecraft:spruce_fence_gate")]
    SpruceFenceGate,
    #[serde(rename = "minecraft:birch_fence_gate")]
    BirchFenceGate,
    #[serde(rename = "minecraft:jungle_fence_gate")]
    JungleFenceGate,
    #[serde(rename = "minecraft:acacia_fence_gate")]
    AcaciaFenceGate,
    #[serde(rename = "minecraft:dark_oak_fence_gate")]
    DarkOakFenceGate,
    #[serde(rename = "minecraft:mangrove_fence_gate")]
    MangroveFenceGate,
    #[serde(rename = "minecraft:bamboo_fence_gate")]
    BambooFenceGate,
    #[serde(rename = "minecraft:cherry_fence_gate")]
    CherryFenceGate,
    #[serde(rename = "minecraft:crimson_fence_gate")]
    CrimsonFenceGate,
    #[serde(rename = "minecraft:warped_fence_gate")]
    WarpedFenceGate,

    // === DOORS ===
    #[serde(rename = "minecraft:oak_door")]
    OakDoor,
    #[serde(rename = "minecraft:spruce_door")]
    SpruceDoor,
    #[serde(rename = "minecraft:birch_door")]
    BirchDoor,
    #[serde(rename = "minecraft:jungle_door")]
    JungleDoor,
    #[serde(rename = "minecraft:acacia_door")]
    AcaciaDoor,
    #[serde(rename = "minecraft:dark_oak_door")]
    DarkOakDoor,
    #[serde(rename = "minecraft:mangrove_door")]
    MangroveDoor,
    #[serde(rename = "minecraft:bamboo_door")]
    BambooDoor,
    #[serde(rename = "minecraft:cherry_door")]
    CherryDoor,
    #[serde(rename = "minecraft:crimson_door")]
    CrimsonDoor,
    #[serde(rename = "minecraft:warped_door")]
    WarpedDoor,

    // === TRAPDOORS ===
    #[serde(rename = "minecraft:oak_trapdoor")]
    OakTrapdoor,
    #[serde(rename = "minecraft:spruce_trapdoor")]
    SpruceTrapdoor,
    #[serde(rename = "minecraft:birch_trapdoor")]
    BirchTrapdoor,
    #[serde(rename = "minecraft:jungle_trapdoor")]
    JungleTrapdoor,
    #[serde(rename = "minecraft:acacia_trapdoor")]
    AcaciaTrapdoor,
    #[serde(rename = "minecraft:dark_oak_trapdoor")]
    DarkOakTrapdoor,
    #[serde(rename = "minecraft:mangrove_trapdoor")]
    MangroveTrapdoor,
    #[serde(rename = "minecraft:bamboo_trapdoor")]
    BambooTrapdoor,
    #[serde(rename = "minecraft:cherry_trapdoor")]
    CherryTrapdoor,
    #[serde(rename = "minecraft:crimson_trapdoor")]
    CrimsonTrapdoor,
    #[serde(rename = "minecraft:warped_trapdoor")]
    WarpedTrapdoor,

    // === SIGNS ===
    #[serde(rename = "minecraft:oak_sign")]
    OakSign,
    #[serde(rename = "minecraft:spruce_sign")]
    SpruceSign,
    #[serde(rename = "minecraft:birch_sign")]
    BirchSign,
    #[serde(rename = "minecraft:jungle_sign")]
    JungleSign,
    #[serde(rename = "minecraft:acacia_sign")]
    AcaciaSign,
    #[serde(rename = "minecraft:dark_oak_sign")]
    DarkOakSign,
    #[serde(rename = "minecraft:mangrove_sign")]
    MangroveSign,
    #[serde(rename = "minecraft:bamboo_sign")]
    BambooSign,
    #[serde(rename = "minecraft:cherry_sign")]
    CherrySign,
    #[serde(rename = "minecraft:crimson_sign")]
    CrimsonSign,
    #[serde(rename = "minecraft:warped_sign")]
    WarpedSign,

    // === WALL SIGNS ===
    #[serde(rename = "minecraft:oak_wall_sign")]
    OakWallSign,
    #[serde(rename = "minecraft:spruce_wall_sign")]
    SpruceWallSign,
    #[serde(rename = "minecraft:birch_wall_sign")]
    BirchWallSign,
    #[serde(rename = "minecraft:jungle_wall_sign")]
    JungleWallSign,
    #[serde(rename = "minecraft:acacia_wall_sign")]
    AcaciaWallSign,
    #[serde(rename = "minecraft:dark_oak_wall_sign")]
    DarkOakWallSign,
    #[serde(rename = "minecraft:mangrove_wall_sign")]
    MangroveWallSign,
    #[serde(rename = "minecraft:bamboo_wall_sign")]
    BambooWallSign,
    #[serde(rename = "minecraft:cherry_wall_sign")]
    CherryWallSign,
    #[serde(rename = "minecraft:crimson_wall_sign")]
    CrimsonWallSign,
    #[serde(rename = "minecraft:warped_wall_sign")]
    WarpedWallSign,

    // === HANGING SIGNS ===
    #[serde(rename = "minecraft:oak_hanging_sign")]
    OakHangingSign,
    #[serde(rename = "minecraft:spruce_hanging_sign")]
    SpruceHangingSign,
    #[serde(rename = "minecraft:birch_hanging_sign")]
    BirchHangingSign,
    #[serde(rename = "minecraft:jungle_hanging_sign")]
    JungleHangingSign,
    #[serde(rename = "minecraft:acacia_hanging_sign")]
    AcaciaHangingSign,
    #[serde(rename = "minecraft:dark_oak_hanging_sign")]
    DarkOakHangingSign,
    #[serde(rename = "minecraft:mangrove_hanging_sign")]
    MangroveHangingSign,
    #[serde(rename = "minecraft:bamboo_hanging_sign")]
    BambooHangingSign,
    #[serde(rename = "minecraft:cherry_hanging_sign")]
    CherryHangingSign,
    #[serde(rename = "minecraft:crimson_hanging_sign")]
    CrimsonHangingSign,
    #[serde(rename = "minecraft:warped_hanging_sign")]
    WarpedHangingSign,

    // === HANGING WALL SIGNS ===
    #[serde(rename = "minecraft:oak_hanging_wall_sign")]
    OakHangingWallSign,
    #[serde(rename = "minecraft:spruce_hanging_wall_sign")]
    SpruceHangingWallSign,
    #[serde(rename = "minecraft:birch_hanging_wall_sign")]
    BirchHangingWallSign,
    #[serde(rename = "minecraft:jungle_hanging_wall_sign")]
    JungleHangingWallSign,
    #[serde(rename = "minecraft:acacia_hanging_wall_sign")]
    AcaciaHangingWallSign,
    #[serde(rename = "minecraft:dark_oak_hanging_wall_sign")]
    DarkOakHangingWallSign,
    #[serde(rename = "minecraft:mangrove_hanging_wall_sign")]
    MangroveHangingWallSign,
    #[serde(rename = "minecraft:bamboo_hanging_wall_sign")]
    BambooHangingWallSign,
    #[serde(rename = "minecraft:cherry_hanging_wall_sign")]
    CherryHangingWallSign,
    #[serde(rename = "minecraft:crimson_hanging_wall_sign")]
    CrimsonHangingWallSign,
    #[serde(rename = "minecraft:warped_hanging_wall_sign")]
    WarpedHangingWallSign,

    // === BUTTONS ===
    #[serde(rename = "minecraft:oak_button")]
    OakButton,
    #[serde(rename = "minecraft:spruce_button")]
    SpruceButton,
    #[serde(rename = "minecraft:birch_button")]
    BirchButton,
    #[serde(rename = "minecraft:jungle_button")]
    JungleButton,
    #[serde(rename = "minecraft:acacia_button")]
    AcaciaButton,
    #[serde(rename = "minecraft:dark_oak_button")]
    DarkOakButton,
    #[serde(rename = "minecraft:mangrove_button")]
    MangroveButton,
    #[serde(rename = "minecraft:bamboo_button")]
    BambooButton,
    #[serde(rename = "minecraft:cherry_button")]
    CherryButton,
    #[serde(rename = "minecraft:crimson_button")]
    CrimsonButton,
    #[serde(rename = "minecraft:warped_button")]
    WarpedButton,
    #[serde(rename = "minecraft:stone_button")]
    StoneButton,
    #[serde(rename = "minecraft:polished_blackstone_button", alias = "minecraft:blackstone_button")]
    PolishedBlackstoneButton,

    // === PRESSURE PLATES ===
    #[serde(rename = "minecraft:oak_pressure_plate")]
    OakPressurePlate,
    #[serde(rename = "minecraft:spruce_pressure_plate")]
    SprucePressurePlate,
    #[serde(rename = "minecraft:birch_pressure_plate")]
    BirchPressurePlate,
    #[serde(rename = "minecraft:jungle_pressure_plate")]
    JunglePressurePlate,
    #[serde(rename = "minecraft:acacia_pressure_plate")]
    AcaciaPressurePlate,
    #[serde(rename = "minecraft:dark_oak_pressure_plate")]
    DarkOakPressurePlate,
    #[serde(rename = "minecraft:mangrove_pressure_plate")]
    MangrovePressurePlate,
    #[serde(rename = "minecraft:bamboo_pressure_plate")]
    BambooPressurePlate,
    #[serde(rename = "minecraft:cherry_pressure_plate")]
    CherryPressurePlate,
    #[serde(rename = "minecraft:crimson_pressure_plate")]
    CrimsonPressurePlate,
    #[serde(rename = "minecraft:warped_pressure_plate")]
    WarpedPressurePlate,
    #[serde(rename = "minecraft:stone_pressure_plate")]
    StonePressurePlate,
    #[serde(rename = "minecraft:polished_blackstone_pressure_plate", alias = "minecraft:blackstone_pressure_plate")]
    PolishedBlackstonePressurePlate,

    // === WATER & FLUIDS ===
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

    // === CAULDRONS ===
    #[serde(rename = "minecraft:cauldron")]
    Cauldron,
    #[serde(rename = "minecraft:water_cauldron")]
    WaterCauldron,
    #[serde(rename = "minecraft:lava_cauldron")]
    LavaCauldron,
    #[serde(rename = "minecraft:powder_snow_cauldron")]
    PowderSnowCauldron,

    // === CHAINS ===
    #[serde(rename = "minecraft:chain")]
    Chain,

    // === IRON BARS ===
    #[serde(rename = "minecraft:iron_bars")]
    IronBars,

    // === BEDROCK ===
    #[serde(rename = "minecraft:bedrock")]
    Bedrock,

    // === (Add more sections and blocks as needed for full coverage) ===
    #[serde(rename = "minecraft:player_head")]
    PlayerHead,

    // NETHER BRICKS
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

    // CONCRETE
    #[serde(rename = "minecraft:white_concrete")]
    WhiteConcrete,
    #[serde(rename = "minecraft:orange_concrete")]
    OrangeConcrete,
    #[serde(rename = "minecraft:magenta_concrete")]
    MagentaConcrete,
    #[serde(rename = "minecraft:light_blue_concrete")]
    LightBlueConcrete,
    #[serde(rename = "minecraft:yellow_concrete")]
    YellowConcrete,
    #[serde(rename = "minecraft:lime_concrete")]
    LimeConcrete,
    #[serde(rename = "minecraft:pink_concrete")]
    PinkConcrete,
    #[serde(rename = "minecraft:gray_concrete")]
    GrayConcrete,
    #[serde(rename = "minecraft:light_gray_concrete")]
    LightGrayConcrete,
    #[serde(rename = "minecraft:cyan_concrete")]
    CyanConcrete,
    #[serde(rename = "minecraft:purple_concrete")]
    PurpleConcrete,
    #[serde(rename = "minecraft:blue_concrete")]
    BlueConcrete,
    #[serde(rename = "minecraft:brown_concrete")]
    BrownConcrete,
    #[serde(rename = "minecraft:green_concrete")]
    GreenConcrete,
    #[serde(rename = "minecraft:red_concrete")]
    RedConcrete,
    #[serde(rename = "minecraft:black_concrete")]
    BlackConcrete,

    #[serde(rename = "minecraft:white_concrete_powder")]
    WhiteConcretePowder,
    #[serde(rename = "minecraft:orange_concrete_powder")]
    OrangeConcretePowder,
    #[serde(rename = "minecraft:magenta_concrete_powder")]
    MagentaConcretePowder,
    #[serde(rename = "minecraft:light_blue_concrete_powder")]
    LightBlueConcretePowder,
    #[serde(rename = "minecraft:yellow_concrete_powder")]
    YellowConcretePowder,
    #[serde(rename = "minecraft:lime_concrete_powder")]
    LimeConcretePowder,
    #[serde(rename = "minecraft:pink_concrete_powder")]
    PinkConcretePowder,
    #[serde(rename = "minecraft:gray_concrete_powder")]
    GrayConcretePowder,
    #[serde(rename = "minecraft:light_gray_concrete_powder")]
    LightGrayConcretePowder,
    #[serde(rename = "minecraft:cyan_concrete_powder")]
    CyanConcretePowder,
    #[serde(rename = "minecraft:purple_concrete_powder")]
    PurpleConcretePowder,
    #[serde(rename = "minecraft:blue_concrete_powder")]
    BlueConcretePowder,
    #[serde(rename = "minecraft:brown_concrete_powder")]
    BrownConcretePowder,
    #[serde(rename = "minecraft:green_concrete_powder")]
    GreenConcretePowder,
    #[serde(rename = "minecraft:red_concrete_powder")]
    RedConcretePowder,
    #[serde(rename = "minecraft:black_concrete_powder")]
    BlackConcretePowder,

    // TERRACOTTA
    #[serde(rename = "minecraft:terracotta")]
    Terracotta,
    #[serde(rename = "minecraft:white_terracotta")]
    WhiteTerracotta,
    #[serde(rename = "minecraft:orange_terracotta")]
    OrangeTerracotta,
    #[serde(rename = "minecraft:magenta_terracotta")]
    MagentaTerracotta,
    #[serde(rename = "minecraft:light_blue_terracotta")]
    LightBlueTerracotta,
    #[serde(rename = "minecraft:yellow_terracotta")]
    YellowTerracotta,
    #[serde(rename = "minecraft:lime_terracotta")]
    LimeTerracotta,
    #[serde(rename = "minecraft:pink_terracotta")]
    PinkTerracotta,
    #[serde(rename = "minecraft:gray_terracotta")]
    GrayTerracotta,
    #[serde(rename = "minecraft:light_gray_terracotta")]
    LightGrayTerracotta,
    #[serde(rename = "minecraft:cyan_terracotta")]
    CyanTerracotta,
    #[serde(rename = "minecraft:purple_terracotta")]
    PurpleTerracotta,
    #[serde(rename = "minecraft:blue_terracotta")]
    BlueTerracotta,
    #[serde(rename = "minecraft:brown_terracotta")]
    BrownTerracotta,
    #[serde(rename = "minecraft:green_terracotta")]
    GreenTerracotta,
    #[serde(rename = "minecraft:red_terracotta")]
    RedTerracotta,
    #[serde(rename = "minecraft:black_terracotta")]
    BlackTerracotta,

    // MUD BRICKs
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

    // PRISMARINE
    #[serde(rename = "minecraft:prismarine")]
    Prismarine,
    #[serde(rename = "minecraft:prismarine_bricks")]
    PrismarineBricks,
    #[serde(rename = "minecraft:dark_prismarine")]
    DarkPrismarine,
    #[serde(rename = "minecraft:prismarine_slab")]
    PrismarineSlab,
    #[serde(rename = "minecraft:prismarine_brick_slab")]
    PrismarineBrickSlab,
    #[serde(rename = "minecraft:dark_prismarine_slab")]
    DarkPrismarineSlab,
    #[serde(rename = "minecraft:prismarine_stairs")]
    PrismarineStairs,
    #[serde(rename = "minecraft:prismarine_brick_stairs")]
    PrismarineBrickStairs,
    #[serde(rename = "minecraft:dark_prismarine_stairs")]
    DarkPrismarineStairs,
    #[serde(rename = "minecraft:prismarine_wall")]
    PrismarineWall,

    // QUARTS
    #[serde(rename = "minecraft:quartz_block")]
    QuartzBlock,
    #[serde(rename = "minecraft:smooth_quartz")]
    SmoothQuartz,
    #[serde(rename = "minecraft:chiseled_quartz_block")]
    ChiseledQuartzBlock,
    #[serde(rename = "minecraft:quartz_pillar")]
    QuartzPillar,
    #[serde(rename = "minecraft:quartz_bricks")]
    QuartzBricks,
    #[serde(rename = "minecraft:quartz_slab")]
    QuartzSlab,
    #[serde(rename = "minecraft:smooth_quartz_slab")]
    SmoothQuartzSlab,
    #[serde(rename = "minecraft:quartz_stairs")]
    QuartzStairs,
    #[serde(rename = "minecraft:smooth_quartz_stairs")]
    SmoothQuartzStairs,

    // PURPUR
    #[serde(rename = "minecraft:purpur_block")]
    PurpurBlock,
    #[serde(rename = "minecraft:purpur_pillar")]
    PurpurPillar,
    #[serde(rename = "minecraft:purpur_stairs")]
    PurpurStairs,
    #[serde(rename = "minecraft:purpur_slab")]
    PurpurSlab,

    // COPPER BLOCKS
    #[serde(rename = "minecraft:copper_block")]
    CopperBlock,
    #[serde(rename = "minecraft:cut_copper")]
    CutCopper,
    #[serde(rename = "minecraft:cut_copper_slab")]
    CutCopperSlab,
    #[serde(rename = "minecraft:cut_copper_stairs")]
    CutCopperStairs,
    #[serde(rename = "minecraft:exposed_copper")]
    ExposedCopper,
    #[serde(rename = "minecraft:exposed_cut_copper")]
    ExposedCutCopper,
    #[serde(rename = "minecraft:exposed_cut_copper_slab")]
    ExposedCutCopperSlab,
    #[serde(rename = "minecraft:exposed_cut_copper_stairs")]
    ExposedCutCopperStairs,
    #[serde(rename = "minecraft:weathered_copper")]
    WeatheredCopper,
    #[serde(rename = "minecraft:weathered_cut_copper")]
    WeatheredCutCopper,
    #[serde(rename = "minecraft:weathered_cut_copper_slab")]
    WeatheredCutCopperSlab,
    #[serde(rename = "minecraft:weathered_cut_copper_stairs")]
    WeatheredCutCopperStairs,
    #[serde(rename = "minecraft:oxidized_copper")]
    OxidizedCopper,
    #[serde(rename = "minecraft:oxidized_cut_copper")]
    OxidizedCutCopper,
    #[serde(rename = "minecraft:oxidized_cut_copper_slab")]
    OxidizedCutCopperSlab,
    #[serde(rename = "minecraft:oxidized_cut_copper_stairs")]
    OxidizedCutCopperStairs,
    #[serde(rename = "minecraft:waxed_copper_block")]
    WaxedCopperBlock,
    #[serde(rename = "minecraft:waxed_cut_copper")]
    WaxedCutCopper,
    #[serde(rename = "minecraft:waxed_cut_copper_slab")]
    WaxedCutCopperSlab,
    #[serde(rename = "minecraft:waxed_cut_copper_stairs")]
    WaxedCutCopperStairs,
    #[serde(rename = "minecraft:waxed_exposed_copper")]
    WaxedExposedCopper,
    #[serde(rename = "minecraft:waxed_exposed_cut_copper")]
    WaxedExposedCutCopper,
    #[serde(rename = "minecraft:waxed_exposed_cut_copper_slab")]
    WaxedExposedCutCopperSlab,
    #[serde(rename = "minecraft:waxed_exposed_cut_copper_stairs")]
    WaxedExposedCutCopperStairs,
    #[serde(rename = "minecraft:waxed_weathered_copper")]
    WaxedWeatheredCopper,
    #[serde(rename = "minecraft:waxed_weathered_cut_copper")]
    WaxedWeatheredCutCopper,
    #[serde(rename = "minecraft:waxed_weathered_cut_copper_slab")]
    WaxedWeatheredCutCopperSlab,
    #[serde(rename = "minecraft:waxed_weathered_cut_copper_stairs")]
    WaxedWeatheredCutCopperStairs,
    #[serde(rename = "minecraft:waxed_oxidized_copper")]
    WaxedOxidizedCopper,
    #[serde(rename = "minecraft:waxed_oxidized_cut_copper")]
    WaxedOxidizedCutCopper,
    #[serde(rename = "minecraft:waxed_oxidized_cut_copper_slab")]
    WaxedOxidizedCutCopperSlab,
    #[serde(rename = "minecraft:waxed_oxidized_cut_copper_stairs")]
    WaxedOxidizedCutCopperStairs,

    // GLASS BLOCKS
    #[serde(rename = "minecraft:glass")]
    Glass,
    #[serde(rename = "minecraft:white_stained_glass")]
    WhiteStainedGlass,
    #[serde(rename = "minecraft:orange_stained_glass")]
    OrangeStainedGlass,
    #[serde(rename = "minecraft:magenta_stained_glass")]
    MagentaStainedGlass,
    #[serde(rename = "minecraft:light_blue_stained_glass")]
    LightBlueStainedGlass,
    #[serde(rename = "minecraft:yellow_stained_glass")]
    YellowStainedGlass,
    #[serde(rename = "minecraft:lime_stained_glass")]
    LimeStainedGlass,
    #[serde(rename = "minecraft:pink_stained_glass")]
    PinkStainedGlass,
    #[serde(rename = "minecraft:gray_stained_glass")]
    GrayStainedGlass,
    #[serde(rename = "minecraft:light_gray_stained_glass")]
    LightGrayStainedGlass,
    #[serde(rename = "minecraft:cyan_stained_glass")]
    CyanStainedGlass,
    #[serde(rename = "minecraft:purple_stained_glass")]
    PurpleStainedGlass,
    #[serde(rename = "minecraft:blue_stained_glass")]
    BlueStainedGlass,
    #[serde(rename = "minecraft:brown_stained_glass")]
    BrownStainedGlass,
    #[serde(rename = "minecraft:green_stained_glass")]
    GreenStainedGlass,
    #[serde(rename = "minecraft:red_stained_glass")]
    RedStainedGlass,
    #[serde(rename = "minecraft:black_stained_glass")]
    BlackStainedGlass,

    // GLASS PANES
    #[serde(rename = "minecraft:glass_pane")]
    GlassPane,
    #[serde(rename = "minecraft:white_stained_glass_pane")]
    WhiteStainedGlassPane,
    #[serde(rename = "minecraft:orange_stained_glass_pane")]
    OrangeStainedGlassPane,
    #[serde(rename = "minecraft:magenta_stained_glass_pane")]
    MagentaStainedGlassPane,
    #[serde(rename = "minecraft:light_blue_stained_glass_pane")]
    LightBlueStainedGlassPane,
    #[serde(rename = "minecraft:yellow_stained_glass_pane")]
    YellowStainedGlassPane,
    #[serde(rename = "minecraft:lime_stained_glass_pane")]
    LimeStainedGlassPane,
    #[serde(rename = "minecraft:pink_stained_glass_pane")]
    PinkStainedGlassPane,
    #[serde(rename = "minecraft:gray_stained_glass_pane")]
    GrayStainedGlassPane,
    #[serde(rename = "minecraft:light_gray_stained_glass_pane")]
    LightGrayStainedGlassPane,
    #[serde(rename = "minecraft:cyan_stained_glass_pane")]
    CyanStainedGlassPane,
    #[serde(rename = "minecraft:purple_stained_glass_pane")]
    PurpleStainedGlassPane,
    #[serde(rename = "minecraft:blue_stained_glass_pane")]
    BlueStainedGlassPane,
    #[serde(rename = "minecraft:brown_stained_glass_pane")]
    BrownStainedGlassPane,
    #[serde(rename = "minecraft:green_stained_glass_pane")]
    GreenStainedGlassPane,
    #[serde(rename = "minecraft:red_stained_glass_pane")]
    RedStainedGlassPane,
    #[serde(rename = "minecraft:black_stained_glass_pane")]
    BlackStainedGlassPane,

    // FURNITURE
    #[serde(rename = "minecraft:decorated_pot")]
    DecoratedPot,
    #[serde(rename = "minecraft:crafting_table")]
    CraftingTable,
    #[serde(rename = "minecraft:furnace")]
    Furnace,
    #[serde(rename = "minecraft:blast_furnace")]
    BlastFurnace,
    #[serde(rename = "minecraft:smoker")]
    Smoker,
    #[serde(rename = "minecraft:cartography_table")]
    CartographyTable,
    #[serde(rename = "minecraft:loom")]
    Loom,
    #[serde(rename = "minecraft:barrel")]
    Barrel,
    #[serde(rename = "minecraft:chest")]
    Chest,
    #[serde(rename = "minecraft:trapped_chest")]
    TrappedChest,
    #[serde(rename = "minecraft:ender_chest")]
    EnderChest,
    #[serde(rename = "minecraft:shulker_box")]
    ShulkerBox,
    #[serde(rename = "minecraft:white_shulker_box")]
    WhiteShulkerBox,
    #[serde(rename = "minecraft:orange_shulker_box")]
    OrangeShulkerBox,
    #[serde(rename = "minecraft:magenta_shulker_box")]
    MagentaShulkerBox,
    #[serde(rename = "minecraft:light_blue_shulker_box")]
    LightBlueShulkerBox,
    #[serde(rename = "minecraft:yellow_shulker_box")]
    YellowShulkerBox,
    #[serde(rename = "minecraft:lime_shulker_box")]
    LimeShulkerBox,
    #[serde(rename = "minecraft:pink_shulker_box")]
    PinkShulkerBox,
    #[serde(rename = "minecraft:gray_shulker_box")]
    GrayShulkerBox,
    #[serde(rename = "minecraft:light_gray_shulker_box")]
    LightGrayShulkerBox,
    #[serde(rename = "minecraft:cyan_shulker_box")]
    CyanShulkerBox,
    #[serde(rename = "minecraft:purple_shulker_box")]
    PurpleShulkerBox,
    #[serde(rename = "minecraft:blue_shulker_box")]
    BlueShulkerBox,
    #[serde(rename = "minecraft:brown_shulker_box")]
    BrownShulkerBox,
    #[serde(rename = "minecraft:green_shulker_box")]
    GreenShulkerBox,
    #[serde(rename = "minecraft:red_shulker_box")]
    RedShulkerBox,
    #[serde(rename = "minecraft:black_shulker_box")]
    BlackShulkerBox,
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
    #[serde(rename = "minecraft:composter")]
    Composter,
    #[serde(rename = "minecraft:bell")]
    Bell,
    #[serde(rename = "minecraft:bed")]
    Bed,
    #[serde(rename = "minecraft:white_bed")]
    WhiteBed,
    #[serde(rename = "minecraft:orange_bed")]
    OrangeBed,
    #[serde(rename = "minecraft:magenta_bed")]
    MagentaBed,
    #[serde(rename = "minecraft:light_blue_bed")]
    LightBlueBed,
    #[serde(rename = "minecraft:yellow_bed")]
    YellowBed,
    #[serde(rename = "minecraft:lime_bed")]
    LimeBed,
    #[serde(rename = "minecraft:pink_bed")]
    PinkBed,
    #[serde(rename = "minecraft:gray_bed")]
    GrayBed,
    #[serde(rename = "minecraft:light_gray_bed")]
    LightGrayBed,
    #[serde(rename = "minecraft:cyan_bed")]
    CyanBed,
    #[serde(rename = "minecraft:purple_bed")]
    PurpleBed,
    #[serde(rename = "minecraft:blue_bed")]
    BlueBed,
    #[serde(rename = "minecraft:brown_bed")]
    BrownBed,
    #[serde(rename = "minecraft:green_bed")]
    GreenBed,
    #[serde(rename = "minecraft:red_bed")]
    RedBed,
    #[serde(rename = "minecraft:black_bed")]
    BlackBed,
    #[serde(rename = "minecraft:jukebox")]
    Jukebox,
    #[serde(rename = "minecraft:note_block")]
    NoteBlock,
    #[serde(rename = "minecraft:bookshelf")]
    Bookshelf,
    #[serde(rename = "minecraft:chiseled_bookshelf")]
    ChiseledBookshelf,
    #[serde(rename = "minecraft:flower_pot")]
    FlowerPot,
    #[serde(rename = "minecraft:painting")]
    Painting,
    #[serde(rename = "minecraft:item_frame")]
    ItemFrame,
    #[serde(rename = "minecraft:glow_item_frame")]
    GlowItemFrame,
    #[serde(rename = "minecraft:armor_stand")]
    ArmorStand,
    #[serde(rename = "minecraft:lantern")]
    Lantern,
    #[serde(rename = "minecraft:soul_lantern")]
    SoulLantern,

    // === PLANTS ===
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
    #[serde(rename = "minecraft:sunflower")]
    Sunflower,
    #[serde(rename = "minecraft:lilac")]
    Lilac,
    #[serde(rename = "minecraft:rose_bush")]
    RoseBush,
    #[serde(rename = "minecraft:peony")]
    Peony,
    #[serde(rename = "minecraft:bamboo")]
    Bamboo,
    #[serde(rename = "minecraft:sugar_cane")]
    SugarCane,
    #[serde(rename = "minecraft:cactus")]
    Cactus,
    #[serde(rename = "minecraft:dead_bush")]
    DeadBush,
    #[serde(rename = "minecraft:moss_block")]
    MossBlock,
    #[serde(rename = "minecraft:moss_carpet")]
    MossCarpet,
    #[serde(rename = "minecraft:azalea")]
    Azalea,
    #[serde(rename = "minecraft:flowering_azalea")]
    FloweringAzalea,
    #[serde(rename = "minecraft:spore_blossom")]
    SporeBlossom,
    #[serde(rename = "minecraft:small_dripleaf")]
    SmallDripleaf,
    #[serde(rename = "minecraft:big_dripleaf")]
    BigDripleaf,
    #[serde(rename = "minecraft:big_dripleaf_stem")]
    BigDripleafStem,
    #[serde(rename = "minecraft:lily_pad")]
    LilyPad,
    #[serde(rename = "minecraft:vine")]
    Vine,
    #[serde(rename = "minecraft:glow_lichen")]
    GlowLichen,
    #[serde(rename = "minecraft:torchflower")]
    Torchflower,
    #[serde(rename = "minecraft:pitcher_plant")]
    PitcherPlant,
    #[serde(rename = "minecraft:pitcher_crop")]
    PitcherCrop,

    // === POTTED PLANTS ===
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

    // LEAVES
    #[serde(rename = "minecraft:oak_leaves")]
    OakLeaves,
    #[serde(rename = "minecraft:spruce_leaves")]
    SpruceLeaves,
    #[serde(rename = "minecraft:birch_leaves")]
    BirchLeaves,
    #[serde(rename = "minecraft:jungle_leaves")]
    JungleLeaves,
    #[serde(rename = "minecraft:acacia_leaves")]
    AcaciaLeaves,
    #[serde(rename = "minecraft:dark_oak_leaves")]
    DarkOakLeaves,
    #[serde(rename = "minecraft:mangrove_leaves")]
    MangroveLeaves,
    #[serde(rename = "minecraft:cherry_leaves")]
    CherryLeaves,
    #[serde(rename = "minecraft:azalea_leaves")]
    AzaleaLeaves,
    #[serde(rename = "minecraft:flowering_azalea_leaves")]
    FloweringAzaleaLeaves,

    // === BEEHIVE ===
    #[serde(rename = "minecraft:beehive")]
    Beehive,
    #[serde(rename = "minecraft:bee_nest")]
    BeeNest,

    // === MUSHROOMS ===
    #[serde(rename = "minecraft:mushroom_stem")]
    MushroomStem,
    #[serde(rename = "minecraft:red_mushroom_block")]
    RedMushroomBlock,
    #[serde(rename = "minecraft:brown_mushroom_block")]
    BrownMushroomBlock,

<<<<<<< HEAD
=======
    // === BANNERS ===
    #[serde(rename = "minecraft:white_banner")]
    WhiteBanner,
    #[serde(rename = "minecraft:orange_banner")]
    OrangeBanner,
    #[serde(rename = "minecraft:magenta_banner")]
    MagentaBanner,
    #[serde(rename = "minecraft:light_blue_banner")]
    LightBlueBanner,
    #[serde(rename = "minecraft:yellow_banner")]
    YellowBanner,
    #[serde(rename = "minecraft:lime_banner")]
    LimeBanner,
    #[serde(rename = "minecraft:pink_banner")]
    PinkBanner,
    #[serde(rename = "minecraft:gray_banner")]
    GrayBanner,
    #[serde(rename = "minecraft:light_gray_banner")]
    LightGrayBanner,
    #[serde(rename = "minecraft:cyan_banner")]
    CyanBanner,
    #[serde(rename = "minecraft:purple_banner")]
    PurpleBanner,
    #[serde(rename = "minecraft:blue_banner")]
    BlueBanner,
    #[serde(rename = "minecraft:brown_banner")]
    BrownBanner,
    #[serde(rename = "minecraft:green_banner")]
    GreenBanner,
    #[serde(rename = "minecraft:red_banner")]
    RedBanner,
    #[serde(rename = "minecraft:black_banner")]
    BlackBanner,

    // === WALL BANNERS ===
    #[serde(rename = "minecraft:white_wall_banner")]
    WhiteWallBanner,
    #[serde(rename = "minecraft:orange_wall_banner")]
    OrangeWallBanner,
    #[serde(rename = "minecraft:magenta_wall_banner")]
    MagentaWallBanner,
    #[serde(rename = "minecraft:light_blue_wall_banner")]
    LightBlueWallBanner,
    #[serde(rename = "minecraft:yellow_wall_banner")]
    YellowWallBanner,
    #[serde(rename = "minecraft:lime_wall_banner")]
    LimeWallBanner,
    #[serde(rename = "minecraft:pink_wall_banner")]
    PinkWallBanner,
    #[serde(rename = "minecraft:gray_wall_banner")]
    GrayWallBanner,
    #[serde(rename = "minecraft:light_gray_wall_banner")]
    LightGrayWallBanner,
    #[serde(rename = "minecraft:cyan_wall_banner")]
    CyanWallBanner,
    #[serde(rename = "minecraft:purple_wall_banner")]
    PurpleWallBanner,
    #[serde(rename = "minecraft:blue_wall_banner")]
    BlueWallBanner,
    #[serde(rename = "minecraft:brown_wall_banner")]
    BrownWallBanner,
    #[serde(rename = "minecraft:green_wall_banner")]
    GreenWallBanner,
    #[serde(rename = "minecraft:red_wall_banner")]
    RedWallBanner,
    #[serde(rename = "minecraft:black_wall_banner")]
    BlackWallBanner,

>>>>>>> master
    #[serde(other)]
    Unknown,
}

impl BlockID {
    pub fn is_water(self) -> bool {
        matches!(
            self,
            BlockID::Water
            | BlockID::Ice
            | BlockID::PackedIce
            | BlockID::BlueIce
            | BlockID::FrostedIce
            | BlockID::BubbleColumn
            | BlockID::Kelp
            | BlockID::KelpPlant
            | BlockID::Seagrass
            | BlockID::TallSeagrass
            | BlockID::WaterCauldron
        )
    }
    pub fn is_log(self) -> bool {
        matches!(
            self,
            BlockID::OakLog
            | BlockID::SpruceLog
            | BlockID::BirchLog
            | BlockID::JungleLog
            | BlockID::AcaciaLog
            | BlockID::DarkOakLog
            | BlockID::MangroveLog
            | BlockID::CherryLog
        )
    }
    pub fn is_mushroom(self) -> bool {
        matches!(
            self,
            BlockID::MushroomStem
            | BlockID::RedMushroomBlock
            | BlockID::BrownMushroomBlock
        )
    }
    pub fn is_leaf(self) -> bool {
        matches!(
            self,
            BlockID::OakLeaves
            | BlockID::SpruceLeaves
            | BlockID::BirchLeaves
            | BlockID::JungleLeaves
            | BlockID::AcaciaLeaves
            | BlockID::DarkOakLeaves
            | BlockID::MangroveLeaves
            | BlockID::CherryLeaves
            | BlockID::AzaleaLeaves
            | BlockID::FloweringAzaleaLeaves
        )
    }
    pub fn is_tree(self) -> bool {
        self.is_log() || self.is_mushroom()
    }
    pub fn is_tree_or_leaf(self) -> bool {
        self.is_leaf() || self.is_log() || self.is_mushroom()
    }

    pub fn is_stairs(self) -> bool {
        matches!(
            self,
            BlockID::OakStairs | BlockID::SpruceStairs | BlockID::BirchStairs | BlockID::JungleStairs
            | BlockID::AcaciaStairs | BlockID::DarkOakStairs | BlockID::MangroveStairs
            | BlockID::BambooStairs | BlockID::BambooMosaicStairs | BlockID::CherryStairs
            | BlockID::CrimsonStairs | BlockID::WarpedStairs | BlockID::SandstoneStairs
            | BlockID::SmoothSandstoneStairs | BlockID::RedSandstoneStairs | BlockID::SmoothRedSandstoneStairs
            | BlockID::StoneStairs | BlockID::CobblestoneStairs | BlockID::MossyCobblestoneStairs
            | BlockID::StoneBrickStairs | BlockID::MossyStoneBrickStairs | BlockID::AndesiteStairs
            | BlockID::PolishedAndesiteStairs | BlockID::DioriteStairs | BlockID::PolishedDioriteStairs
            | BlockID::GraniteStairs | BlockID::PolishedGraniteStairs | BlockID::DeepslateBrickStairs
            | BlockID::DeepslateTileStairs | BlockID::CobbledDeepslateStairs | BlockID::PolishedDeepslateStairs
            | BlockID::TuffBrickStairs | BlockID::PolishedTuffStairs | BlockID::BlackstoneStairs
            | BlockID::PolishedBlackstoneStairs | BlockID::PolishedBlackstoneBrickStairs
            | BlockID::NetherBrickStairs | BlockID::RedNetherBrickStairs | BlockID::PrismarineStairs
            | BlockID::PrismarineBrickStairs | BlockID::DarkPrismarineStairs | BlockID::MudBrickStairs
            | BlockID::QuartzStairs | BlockID::SmoothQuartzStairs | BlockID::PurpurStairs
            | BlockID::CutCopperStairs | BlockID::ExposedCutCopperStairs | BlockID::WeatheredCutCopperStairs
            | BlockID::OxidizedCutCopperStairs | BlockID::WaxedCutCopperStairs
            | BlockID::WaxedExposedCutCopperStairs | BlockID::WaxedWeatheredCutCopperStairs
            | BlockID::WaxedOxidizedCutCopperStairs
        )
    }

    pub fn is_slab(self) -> bool {
        matches!(
            self,
            BlockID::OakSlab | BlockID::SpruceSlab | BlockID::BirchSlab | BlockID::JungleSlab
            | BlockID::AcaciaSlab | BlockID::DarkOakSlab | BlockID::MangroveSlab | BlockID::BambooSlab
            | BlockID::BambooMosaicSlab | BlockID::CherrySlab | BlockID::CrimsonSlab | BlockID::WarpedSlab
            | BlockID::SandstoneSlab | BlockID::CutSandstoneSlab | BlockID::SmoothSandstoneSlab
            | BlockID::RedSandstoneSlab | BlockID::CutRedSandstoneSlab | BlockID::SmoothRedSandstoneSlab
            | BlockID::StoneSlab | BlockID::SmoothStoneSlab | BlockID::CobblestoneSlab | BlockID::MossyCobblestoneSlab
            | BlockID::StoneBrickSlab | BlockID::MossyStoneBrickSlab | BlockID::AndesiteSlab | BlockID::PolishedAndesiteSlab
            | BlockID::DioriteSlab | BlockID::PolishedDioriteSlab | BlockID::GraniteSlab | BlockID::PolishedGraniteSlab
            | BlockID::CobbledDeepslateSlab | BlockID::PolishedDeepslateSlab | BlockID::DeepslateBrickSlab
            | BlockID::DeepslateTileSlab | BlockID::TuffBrickSlab | BlockID::PolishedTuffSlab | BlockID::BlackstoneSlab
            | BlockID::PolishedBlackstoneSlab | BlockID::PolishedBlackstoneBrickSlab | BlockID::NetherBrickSlab
            | BlockID::RedNetherBrickSlab | BlockID::PrismarineSlab | BlockID::PrismarineBrickSlab
            | BlockID::DarkPrismarineSlab | BlockID::MudBrickSlab | BlockID::QuartzSlab | BlockID::SmoothQuartzSlab
            | BlockID::PurpurSlab | BlockID::CutCopperSlab | BlockID::ExposedCutCopperSlab
            | BlockID::WeatheredCutCopperSlab | BlockID::OxidizedCutCopperSlab | BlockID::WaxedCutCopperSlab
            | BlockID::WaxedExposedCutCopperSlab | BlockID::WaxedWeatheredCutCopperSlab | BlockID::WaxedOxidizedCutCopperSlab
        )
    }

    pub fn is_fence(self) -> bool {
        matches!(
            self,
            BlockID::OakFence | BlockID::SpruceFence | BlockID::BirchFence | BlockID::JungleFence
            | BlockID::AcaciaFence | BlockID::DarkOakFence | BlockID::MangroveFence | BlockID::CherryFence
            | BlockID::CrimsonFence | BlockID::WarpedFence | BlockID::NetherBrickFence
            | BlockID::RedNetherBrickFence | BlockID::BambooFence | BlockID::BambooMosaicFence
            | BlockID::PolishedBlackstoneWall
        )
    }
}

impl From<&str> for BlockID {
    fn from(value: &str) -> Self {
        serde_json::from_str::<BlockID>(&format!("\"{}\"", value)).unwrap_or(BlockID::Unknown)
    }
}