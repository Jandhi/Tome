//! Per-culture material theme for open-space furnishing, so plazas, parks,
//! cemeteries, and the rest use blocks that match the settlement — cobble &
//! grass for a medieval town, sandstone, sand & mud bricks for a desert one.

use crate::generator::buildings_v2::Culture;

/// Percent chance that any single park / nook tree in a [`Theme::cherry_blossom`]
/// settlement (Japanese) grows as a flowering cherry instead of its usual
/// biome species.
pub const CHERRY_CHANCE: i32 = 35;

/// Block ids (and a few option lists) the open-space furnishers draw from, so a
/// desert town reads as sandstone instead of cobblestone.
#[derive(Clone, Copy)]
pub struct Theme {
    /// Top surface laid by the flatten pass (grass / sand).
    pub ground: &'static str,
    /// Fill placed under the surface when raising ground (dirt / sandstone).
    pub subsoil: &'static str,
    /// Plaza paving fill (cobblestone / mud bricks).
    pub pave: &'static str,
    /// Plaza paving border accent (stone bricks / smooth sandstone).
    pub pave_border: &'static str,
    /// Primary worked stone for monuments, fountains, wells (stone bricks / cut sandstone).
    pub stone: &'static str,
    /// Decorative accent stone (chiseled stone bricks / chiseled sandstone).
    pub stone_accent: &'static str,
    /// A matching `*_wall` block (cobblestone wall / sandstone wall).
    pub wall: &'static str,
    /// A matching slab (stone brick slab / smooth sandstone slab).
    pub slab: &'static str,
    /// Laid path / gravel-spoke surface (gravel / sandstone).
    pub path: &'static str,
    /// Zen-garden raked bed (gravel / red sand).
    pub rake: &'static str,
    /// Hedge / topiary foliage.
    pub hedge: &'static str,
    /// Grave mound plot block (podzol / coarse dirt).
    pub grave_mound: &'static str,
    /// Wood family for benches, fence posts, and planters (`"oak"` / `"birch"`).
    pub wood: &'static str,
    /// Loose-rock options for boulders / zen stones.
    pub rocks: &'static [&'static str],
    /// Headstone options.
    pub graves: &'static [&'static str],
    /// Arid (desert) style: gates the cactus park and grows the wooded park as
    /// jungle trees, regardless of the underlying world biome.
    pub arid: bool,
    /// Japanese style: any park / nook tree has a [`CHERRY_CHANCE`] chance to
    /// grow as a flowering cherry instead of its biome species.
    pub cherry_blossom: bool,
}

const MEDIEVAL: Theme = Theme {
    ground: "minecraft:grass_block",
    subsoil: "minecraft:dirt",
    pave: "minecraft:cobblestone",
    pave_border: "minecraft:stone_bricks",
    stone: "minecraft:stone_bricks",
    stone_accent: "minecraft:chiseled_stone_bricks",
    wall: "minecraft:cobblestone_wall",
    slab: "minecraft:stone_brick_slab",
    path: "minecraft:gravel",
    rake: "minecraft:gravel",
    hedge: "minecraft:oak_leaves[persistent=true]",
    grave_mound: "minecraft:podzol",
    wood: "oak",
    rocks: &[
        "minecraft:stone",
        "minecraft:cobblestone",
        "minecraft:mossy_cobblestone",
        "minecraft:andesite",
    ],
    graves: &[
        "minecraft:cobblestone",
        "minecraft:stone_bricks",
        "minecraft:mossy_cobblestone",
        "minecraft:mossy_stone_bricks",
    ],
    arid: false,
    cherry_blossom: false,
};

const DESERT: Theme = Theme {
    ground: "minecraft:sand",
    subsoil: "minecraft:sandstone",
    pave: "minecraft:mud_bricks",
    pave_border: "minecraft:smooth_sandstone",
    stone: "minecraft:cut_sandstone",
    stone_accent: "minecraft:chiseled_sandstone",
    wall: "minecraft:sandstone_wall",
    slab: "minecraft:smooth_sandstone_slab",
    path: "minecraft:sandstone",
    rake: "minecraft:red_sand",
    hedge: "minecraft:oak_leaves[persistent=true]",
    grave_mound: "minecraft:coarse_dirt",
    wood: "birch",
    rocks: &[
        "minecraft:sandstone",
        "minecraft:smooth_sandstone",
        "minecraft:cut_sandstone",
        "minecraft:red_sandstone",
    ],
    graves: &[
        "minecraft:sandstone",
        "minecraft:cut_sandstone",
        "minecraft:smooth_sandstone",
        "minecraft:chiseled_sandstone",
    ],
    arid: true,
    cherry_blossom: false,
};

/// Japanese: a green garden town whose worked stone is deepslate, matching the
/// blackstone walls and deepslate roads. Keeps grass and dark-oak woodwork.
const JAPANESE: Theme = Theme {
    ground: "minecraft:grass_block",
    subsoil: "minecraft:dirt",
    pave: "minecraft:cobbled_deepslate",
    pave_border: "minecraft:deepslate_bricks",
    stone: "minecraft:deepslate_bricks",
    stone_accent: "minecraft:chiseled_deepslate",
    wall: "minecraft:deepslate_brick_wall",
    slab: "minecraft:deepslate_brick_slab",
    path: "minecraft:gravel",
    rake: "minecraft:gravel",
    hedge: "minecraft:oak_leaves[persistent=true]",
    grave_mound: "minecraft:podzol",
    wood: "dark_oak",
    rocks: &[
        "minecraft:cobbled_deepslate",
        "minecraft:deepslate",
        "minecraft:polished_blackstone",
        "minecraft:basalt",
    ],
    graves: &[
        "minecraft:deepslate_bricks",
        "minecraft:cobbled_deepslate",
        "minecraft:polished_deepslate",
        "minecraft:cracked_deepslate_bricks",
    ],
    arid: false,
    cherry_blossom: true,
};

impl Theme {
    /// The material theme matching a settlement's culture.
    pub fn for_culture(culture: Culture) -> Theme {
        match culture {
            Culture::Desert => DESERT,
            Culture::Japanese => JAPANESE,
            _ => MEDIEVAL,
        }
    }
}
