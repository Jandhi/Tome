use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Biome {
    Unknown, // Placeholder for unknown biomes
    #[serde(rename = "minecraft:ocean")]
    Ocean,
    #[serde(rename = "minecraft:plains")]
    Plains,
    #[serde(rename = "minecraft:desert")]
    Desert,
    #[serde(rename = "minecraft:mountains")]
    Mountains,
    #[serde(rename = "minecraft:forest")]
    Forest,
    #[serde(rename = "minecraft:taiga")]
    Taiga,
    #[serde(rename = "minecraft:swamp")]
    Swamp,
    #[serde(rename = "minecraft:river")]
    River,
    #[serde(rename = "minecraft:nether")]
    Nether,
    #[serde(rename = "minecraft:the_end")]
    TheEnd,
    #[serde(rename = "minecraft:frozen_ocean")]
    FrozenOcean,
    #[serde(rename = "minecraft:frozen_river")]
    FrozenRiver,
    #[serde(rename = "minecraft:snowy_tundra")]
    SnowyTundra,
    #[serde(rename = "minecraft:snowy_mountains")]
    SnowyMountains,
    #[serde(rename = "minecraft:mushroom_fields")]
    MushroomFields,
    #[serde(rename = "minecraft:mushroom_field_shore")]
    MushroomFieldShore,
    #[serde(rename = "minecraft:beach")]
    Beach,
    #[serde(rename = "minecraft:desert_hills")]
    DesertHills,
    #[serde(rename = "minecraft:wooded_hills")]
    WoodedHills,
    #[serde(rename = "minecraft:taiga_hills")]
    TaigaHills,
    #[serde(rename = "minecraft:mountain_edge")]
    MountainEdge,
    #[serde(rename = "minecraft:jungle")]
    Jungle,
    #[serde(rename = "minecraft:jungle_hills")]
    JungleHills,
    #[serde(rename = "minecraft:jungle_edge")]
    JungleEdge,
    #[serde(rename = "minecraft:deep_ocean")]
    DeepOcean,
    #[serde(rename = "minecraft:stone_shore")]
    StoneShore,
    #[serde(rename = "minecraft:snowy_beach")]
    SnowyBeach,
    #[serde(rename = "minecraft:birch_forest")]
    BirchForest,
    #[serde(rename = "minecraft:birch_forest_hills")]
    BirchForestHills,
    #[serde(rename = "minecraft:dark_forest")]
    DarkForest,
    #[serde(rename = "minecraft:snowy_taiga")]
    SnowyTaiga,
    #[serde(rename = "minecraft:snowy_taiga_hills")]
    SnowyTaigaHills,
    #[serde(rename = "minecraft:giant_tree_taiga")]
    GiantTreeTaiga,
    #[serde(rename = "minecraft:giant_tree_taiga_hills")]
    GiantTreeTaigaHills,
    #[serde(rename = "minecraft:wooded_mountains")]
    WoodedMountains,
    #[serde(rename = "minecraft:savanna")]
    Savanna,
    #[serde(rename = "minecraft:savanna_plateau")]
    SavannaPlateau,
    #[serde(rename = "minecraft:badlands")]
    Badlands,
    #[serde(rename = "minecraft:wooded_badlands_plateau")]
    WoodedBadlandsPlateau,
    #[serde(rename = "minecraft:badlands_plateau")]
    BadlandsPlateau,
    #[serde(rename = "minecraft:small_end_islands")]
    SmallEndIslands,
    #[serde(rename = "minecraft:end_midlands")]
    EndMidlands,
    #[serde(rename = "minecraft:end_highlands")]
    EndHighlands,
    #[serde(rename = "minecraft:end_barrens")]
    EndBarrens,
    #[serde(rename = "minecraft:warm_ocean")]
    WarmOcean,
    #[serde(rename = "minecraft:lukewarm_ocean")]
    LukewarmOcean,
    #[serde(rename = "minecraft:cold_ocean")]
    ColdOcean,
    #[serde(rename = "minecraft:deep_warm_ocean")]
    DeepWarmOcean,
    #[serde(rename = "minecraft:deep_lukewarm_ocean")]
    DeepLukewarmOcean,
    #[serde(rename = "minecraft:deep_cold_ocean")]
    DeepColdOcean,
    #[serde(rename = "minecraft:deep_frozen_ocean")]
    DeepFrozenOcean,
    #[serde(rename = "minecraft:sunflower_plains")]
    SunflowerPlains,
    #[serde(rename = "minecraft:desert_lakes")]
    DesertLakes,
    #[serde(rename = "minecraft:gravelly_mountains")]
    GravellyMountains,
    #[serde(rename = "minecraft:flower_forest")]
    FlowerForest,
    #[serde(rename = "minecraft:taiga_mountains")]
    TaigaMountains,
    #[serde(rename = "minecraft:swamp_hills")]
    SwampHills,
    #[serde(rename = "minecraft:ice_spikes")]
    IceSpikes,
    #[serde(rename = "minecraft:modified_jungle")]
    ModifiedJungle,
    #[serde(rename = "minecraft:modified_jungle_edge")]
    ModifiedJungleEdge,
    #[serde(rename = "minecraft:tall_birch_forest")]
    TallBirchForest,
    #[serde(rename = "minecraft:tall_birch_hills")]
    TallBirchHills,
    #[serde(rename = "minecraft:dark_forest_hills")]
    DarkForestHills,
    #[serde(rename = "minecraft:snowy_taiga_mountains")]
    SnowyTaigaMountains,
    #[serde(rename = "minecraft:giant_spruce_taiga")]
    GiantSpruceTaiga,
    #[serde(rename = "minecraft:giant_spruce_taiga_hills")]
    GiantSpruceTaigaHills,
    #[serde(rename = "minecraft:modified_gravelly_mountains")]
    ModifiedGravellyMountains,
    #[serde(rename = "minecraft:shattered_savanna")]
    ShatteredSavanna,
    #[serde(rename = "minecraft:shattered_savanna_plateau")]
    ShatteredSavannaPlateau,
    #[serde(rename = "minecraft:eroded_badlands")]
    ErodedBadlands,
    #[serde(rename = "minecraft:modified_wooded_badlands_plateau")]
    ModifiedWoodedBadlandsPlateau,
    #[serde(rename = "minecraft:modified_badlands_plateau")]
    ModifiedBadlandsPlateau,
    #[serde(rename = "minecraft:bamboo_jungle")]
    BambooJungle,
    #[serde(rename = "minecraft:bamboo_jungle_hills")]
    BambooJungleHills,
    #[serde(rename = "minecraft:soul_sand_valley")]
    SoulSandValley,
    #[serde(rename = "minecraft:crimson_forest")]
    CrimsonForest,
    #[serde(rename = "minecraft:warped_forest")]
    WarpedForest,
    #[serde(rename = "minecraft:basalt_deltas")]
    BasaltDeltas,
    #[serde(rename = "minecraft:nether_wastes")]
    NetherWastes,
    #[serde(rename = "minecraft:dripstone_caves")]
    DripstoneCaves,
    #[serde(rename = "minecraft:lush_caves")]
    LushCaves,
    #[serde(rename = "minecraft:meadow")]
    Meadow,
    #[serde(rename = "minecraft:grove")]
    Grove,
    #[serde(rename = "minecraft:snowy_slopes")]
    SnowySlopes,
    #[serde(rename = "minecraft:frozen_peaks")]
    FrozenPeaks,
    #[serde(rename = "minecraft:jagged_peaks")]
    JaggedPeaks,
    #[serde(rename = "minecraft:stony_peaks")]
    StonyPeaks,
    #[serde(rename = "minecraft:deep_dark")]
    DeepDark,
    #[serde(rename = "minecraft:mangrove_swamp")]
    MangroveSwamp,
}
