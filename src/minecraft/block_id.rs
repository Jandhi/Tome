use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlockID {
    Unknown,

    #[serde(rename = "minecraft:air")]
    Air,

    #[serde(rename = "minecraft:grass_block")]
    GrassBlock,
    #[serde(rename = "minecraft:dirt")]
    Dirt,

    #[serde(rename = "minecraft:water")]
    Water,

    // Wool
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

    // Wood

    // PLANKS
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

    // STAIRS
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

    // SLABS
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

    // LOGS
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

    // FENCES
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
    #[serde(rename = "minecraft:cherry_fence")]
    CherryFence,
    #[serde(rename = "minecraft:crimson_fence")]
    CrimsonFence,
    #[serde(rename = "minecraft:warped_fence")]
    WarpedFence,

    // FENCE GATES
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

    // BUTTONS
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

    // PRESSURE PLATES
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

    // DOORS
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

    // TRAPDOORS
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

    // SIGNS
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

    // HANGING SIGNS
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

    // WOOD (bark on all sides)
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

    // STRIPPED LOGS
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

    // STRIPPED WOOD
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

    // STONE

    // Stone
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

    // Misc
    #[serde(rename = "minecraft:gravel")]
    Gravel,

    // Cobblestone
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

    // Stone Bricks
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

    // Andesite
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

    // Diorite
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

    // Granite
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


    // Deepslate
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

    // Blackstone
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

    // Stone buttons and pressure plates
    #[serde(rename = "minecraft:stone_button")]
    StoneButton,
    #[serde(rename = "minecraft:polished_blackstone_button")]
    PolishedBlackstoneButton,
    #[serde(rename = "minecraft:stone_pressure_plate")]
    StonePressurePlate,
    #[serde(rename = "minecraft:polished_blackstone_pressure_plate")]
    PolishedBlackstonePressurePlate,

    #[serde(rename = "minecraft:bedrock")]
    Bedrock,
}

impl BlockID {
    pub fn is_water(self) -> bool {
        matches!(self, BlockID::Water)
    }
}

impl From<&str> for BlockID {
    fn from(value: &str) -> Self {
        serde_json::from_str::<BlockID>(&format!("\"{}\"", value)).unwrap_or(BlockID::Unknown)
    }
}