//! Block id → RGBA colour for the software renderer.
//!
//! Colours are approximate texture averages (close to Minecraft's map colours
//! but less saturated). Lookup order: exact table, wood families, dye-coloured
//! families, shape suffixes (`_stairs`, `_slab`, …) that inherit their base
//! material, a few keyword heuristics, and finally a deterministic muted
//! grey-brown hash so unknown blocks stay visible. Unknown ids are recorded
//! and can be read back with [`unknown_ids`].
//!
//! Alpha below 255 marks translucent blocks (water, glass, ice); alpha 0
//! marks blocks the renderer should treat as empty (barrier, light, …).

use std::collections::BTreeSet;
use std::sync::Mutex;

type Rgba = [u8; 4];

const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
    [r, g, b, 255]
}

static UNKNOWN: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Strip a `minecraft:` namespace and any `[state]` suffix.
fn base_name(id: &str) -> &str {
    let id = id.split('[').next().unwrap_or(id);
    id.strip_prefix("minecraft:").unwrap_or(id)
}

pub fn is_air(id: &str) -> bool {
    matches!(base_name(id), "air" | "cave_air" | "void_air")
}

pub fn is_water(id: &str) -> bool {
    matches!(base_name(id), "water" | "bubble_column")
}

/// Every id that fell through to the hash fallback so far (process-wide),
/// without namespace or state.
pub fn unknown_ids() -> Vec<String> {
    UNKNOWN.lock().map(|s| s.iter().cloned().collect()).unwrap_or_default()
}

/// Colour for any block id (`minecraft:` prefix and `[state]` optional).
pub fn block_color(id: &str) -> Rgba {
    let name = base_name(id);
    if let Some(c) = lookup(name) {
        return c;
    }
    if let Ok(mut set) = UNKNOWN.lock() {
        set.insert(name.to_string());
    }
    hash_color(name)
}

fn hash_color(name: &str) -> Rgba {
    // FNV-1a: deterministic across runs and platforms.
    let mut h: u32 = 0x811c9dc5;
    for b in name.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    rgb(110 + (h % 40) as u8, 95 + ((h >> 8) % 35) as u8, 80 + ((h >> 16) % 30) as u8)
}

fn lookup(name: &str) -> Option<Rgba> {
    if is_air(name) {
        return Some([0, 0, 0, 0]);
    }
    if is_water(name) {
        return Some(WATER);
    }
    if let Some(c) = exact(name) {
        return Some(c);
    }
    for prefix in ["waxed_", "infested_", "petrified_"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            return lookup(rest);
        }
    }
    if name.starts_with("potted_") {
        return exact("flower_pot");
    }
    if let Some(c) = wood(name).or_else(|| dyed(name)) {
        return Some(c);
    }
    if name.contains("copper") {
        return Some(copper(name));
    }
    for suffix in [
        "_stairs", "_slab", "_wall", "_fence_gate", "_fence", "_door", "_trapdoor", "_pressure_plate", "_button",
        "_pane", "_carpet",
    ] {
        if let Some(base) = name.strip_suffix(suffix) {
            for cand in [base.to_string(), format!("{base}s"), format!("{base}_block"), format!("{base}_planks")] {
                if let Some(c) = exact(&cand) {
                    return Some(c);
                }
            }
        }
    }
    keyword(name)
}

/// Translucent so stacked water reads deeper (bluer) the more blocks a ray crosses.
const WATER: Rgba = [52, 90, 200, 110];
const PLANT_GREEN: Rgba = rgb(90, 135, 50);

fn keyword(name: &str) -> Option<Rgba> {
    if name.ends_with("_ore") {
        return Some(if name.starts_with("deepslate") {
            rgb(80, 80, 82)
        } else if name.starts_with("nether") {
            rgb(110, 45, 42)
        } else {
            rgb(125, 125, 125)
        });
    }
    if name.ends_with("_leaves") || name.ends_with("_sapling") || name.ends_with("_tulip") || name.ends_with("_vines") {
        return Some(PLANT_GREEN);
    }
    if name.ends_with("_log") || name.ends_with("_wood") {
        return Some(rgb(109, 85, 50));
    }
    if name.ends_with("_planks") || name.ends_with("_shelf") || name.ends_with("_sign") {
        return Some(rgb(162, 130, 78));
    }
    if name.contains("coral") {
        return Some(rgb(200, 90, 120));
    }
    if name.ends_with("_mushroom") {
        return Some(rgb(140, 100, 80));
    }
    if name.ends_with("_head") || name.ends_with("_skull") {
        return Some(rgb(150, 150, 150));
    }
    None
}

fn copper(name: &str) -> Rgba {
    if name.contains("oxidized") {
        rgb(82, 162, 132)
    } else if name.contains("weathered") {
        rgb(108, 153, 110)
    } else if name.contains("exposed") {
        rgb(161, 125, 103)
    } else {
        rgb(192, 107, 79)
    }
}

/// (name, planks, log/stem bark, stripped, leaves)
const WOODS: [(&str, Rgba, Rgba, Rgba, Rgba); 13] = [
    ("oak", rgb(162, 130, 78), rgb(109, 85, 50), rgb(177, 144, 86), rgb(72, 116, 40)),
    ("spruce", rgb(115, 85, 49), rgb(58, 37, 16), rgb(115, 89, 52), rgb(58, 92, 58)),
    ("birch", rgb(192, 175, 121), rgb(216, 215, 210), rgb(196, 176, 118), rgb(98, 128, 64)),
    ("jungle", rgb(160, 115, 81), rgb(85, 67, 25), rgb(171, 132, 84), rgb(56, 128, 30)),
    ("acacia", rgb(168, 90, 50), rgb(103, 96, 86), rgb(174, 92, 59), rgb(82, 118, 32)),
    ("dark_oak", rgb(66, 43, 20), rgb(60, 46, 26), rgb(72, 56, 36), rgb(56, 100, 30)),
    ("pale_oak", rgb(227, 217, 214), rgb(88, 80, 77), rgb(240, 232, 231), rgb(160, 166, 155)),
    ("mangrove", rgb(117, 54, 48), rgb(84, 66, 36), rgb(119, 54, 47), rgb(68, 112, 38)),
    ("cherry", rgb(226, 178, 172), rgb(54, 33, 44), rgb(215, 145, 148), rgb(229, 170, 196)),
    ("bamboo", rgb(194, 173, 80), rgb(127, 144, 58), rgb(178, 158, 72), rgb(93, 144, 19)),
    ("crimson", rgb(101, 48, 70), rgb(92, 25, 29), rgb(137, 57, 90), rgb(114, 2, 2)),
    ("warped", rgb(43, 104, 99), rgb(58, 58, 77), rgb(57, 150, 147), rgb(22, 119, 121)),
    ("azalea", rgb(162, 130, 78), rgb(109, 85, 50), rgb(177, 144, 86), rgb(90, 115, 45)),
];

fn wood(name: &str) -> Option<Rgba> {
    let (stripped, name) = match name.strip_prefix("stripped_") {
        Some(rest) => (true, rest),
        None => (false, name),
    };
    for (w, planks, log, strip, leaves) in WOODS {
        let Some(rest) = name.strip_prefix(w).and_then(|r| r.strip_prefix('_')) else {
            continue;
        };
        return Some(match rest {
            "log" | "wood" | "stem" | "hyphae" | "block" => {
                if stripped {
                    strip
                } else {
                    log
                }
            }
            "leaves" | "sapling" | "propagule" => leaves,
            "mosaic" => rgb(190, 170, 78),
            _ => planks, // planks, stairs, slab, fence(_gate), door, trapdoor, sign, shelf, mosaic_*, …
        });
    }
    None
}

/// Dye order matches the 16 vanilla colours.
const DYES: [&str; 16] = [
    "white", "orange", "magenta", "light_blue", "yellow", "lime", "pink", "gray", "light_gray", "cyan", "purple",
    "blue", "brown", "green", "red", "black",
];
const WOOL: [Rgba; 16] = [
    rgb(234, 236, 237), rgb(241, 118, 20), rgb(190, 69, 180), rgb(58, 175, 217), rgb(249, 198, 40),
    rgb(112, 185, 26), rgb(238, 141, 172), rgb(63, 68, 72), rgb(142, 142, 135), rgb(21, 138, 145),
    rgb(122, 42, 173), rgb(53, 57, 157), rgb(114, 72, 41), rgb(85, 110, 28), rgb(161, 39, 35), rgb(21, 21, 26),
];
const CONCRETE: [Rgba; 16] = [
    rgb(207, 213, 214), rgb(224, 97, 1), rgb(169, 48, 159), rgb(36, 137, 199), rgb(241, 175, 21),
    rgb(94, 169, 24), rgb(214, 101, 143), rgb(55, 58, 62), rgb(125, 125, 115), rgb(21, 119, 136),
    rgb(100, 32, 156), rgb(45, 47, 143), rgb(96, 60, 32), rgb(73, 91, 36), rgb(142, 33, 33), rgb(8, 10, 15),
];
const TERRACOTTA: [Rgba; 16] = [
    rgb(210, 178, 161), rgb(162, 84, 38), rgb(150, 88, 109), rgb(113, 109, 138), rgb(186, 133, 35),
    rgb(104, 118, 53), rgb(162, 78, 79), rgb(58, 42, 36), rgb(135, 107, 98), rgb(87, 91, 91), rgb(118, 70, 86),
    rgb(74, 60, 91), rgb(77, 51, 36), rgb(76, 83, 42), rgb(143, 61, 47), rgb(37, 23, 17),
];

fn dyed(name: &str) -> Option<Rgba> {
    for (i, dye) in DYES.iter().enumerate() {
        let Some(rest) = name.strip_prefix(dye).and_then(|r| r.strip_prefix('_')) else {
            continue;
        };
        let c = match rest {
            "wool" | "carpet" | "bed" | "banner" | "wall_banner" | "shulker_box" | "candle" | "candle_cake"
            | "harness" => WOOL[i],
            "concrete" | "glazed_terracotta" => CONCRETE[i],
            "concrete_powder" => lighten(CONCRETE[i], 0.25),
            "terracotta" => TERRACOTTA[i],
            "stained_glass" | "stained_glass_pane" => {
                let [r, g, b, _] = WOOL[i];
                [r, g, b, 150]
            }
            _ => continue, // e.g. red_sandstone, red_mushroom_block: not a dye family
        };
        return Some(c);
    }
    None
}

fn lighten(c: Rgba, t: f32) -> Rgba {
    let l = |v: u8| (v as f32 + (255.0 - v as f32) * t) as u8;
    [l(c[0]), l(c[1]), l(c[2]), c[3]]
}

fn exact(name: &str) -> Option<Rgba> {
    Some(match name {
        // invisible / technical
        "barrier" | "light" | "structure_void" | "moving_piston" => [0, 0, 0, 0],

        // stone family
        "stone" => rgb(125, 125, 125),
        "cobblestone" => rgb(122, 122, 122),
        "mossy_cobblestone" => rgb(110, 118, 95),
        "stone_bricks" => rgb(122, 121, 122),
        "mossy_stone_bricks" => rgb(115, 121, 105),
        "cracked_stone_bricks" => rgb(118, 117, 118),
        "chiseled_stone_bricks" => rgb(120, 119, 120),
        "smooth_stone" => rgb(158, 158, 158),
        "andesite" => rgb(136, 136, 137),
        "polished_andesite" => rgb(132, 135, 134),
        "diorite" => rgb(189, 188, 189),
        "polished_diorite" => rgb(193, 193, 195),
        "granite" => rgb(149, 103, 86),
        "polished_granite" => rgb(154, 106, 89),
        "deepslate" => rgb(80, 80, 82),
        "cobbled_deepslate" => rgb(77, 77, 80),
        "polished_deepslate" => rgb(72, 72, 73),
        "deepslate_bricks" => rgb(70, 70, 71),
        "cracked_deepslate_bricks" => rgb(65, 65, 65),
        "deepslate_tiles" | "chiseled_deepslate" => rgb(54, 54, 55),
        "cracked_deepslate_tiles" => rgb(52, 52, 52),
        "reinforced_deepslate" => rgb(80, 82, 78),
        "tuff" | "chiseled_tuff" => rgb(108, 109, 103),
        "polished_tuff" => rgb(98, 104, 99),
        "tuff_bricks" | "chiseled_tuff_bricks" => rgb(98, 102, 96),
        "calcite" => rgb(223, 224, 221),
        "dripstone_block" | "pointed_dripstone" => rgb(134, 107, 92),
        "blackstone" => rgb(42, 36, 41),
        "polished_blackstone" => rgb(53, 49, 57),
        "polished_blackstone_bricks" => rgb(48, 43, 50),
        "cracked_polished_blackstone_bricks" => rgb(44, 38, 44),
        "chiseled_polished_blackstone" => rgb(53, 48, 56),
        "gilded_blackstone" => rgb(56, 43, 38),
        "bricks" => rgb(150, 97, 83),
        "mud_bricks" => rgb(137, 103, 79),
        "packed_mud" => rgb(142, 106, 80),
        "nether_bricks" => rgb(44, 21, 26),
        "red_nether_bricks" => rgb(69, 7, 9),
        "cracked_nether_bricks" => rgb(40, 20, 23),
        "chiseled_nether_bricks" => rgb(47, 23, 28),
        "sandstone" | "chiseled_sandstone" => rgb(216, 203, 155),
        "cut_sandstone" => rgb(217, 206, 159),
        "smooth_sandstone" => rgb(223, 214, 170),
        "red_sandstone" | "chiseled_red_sandstone" => rgb(186, 99, 29),
        "cut_red_sandstone" => rgb(189, 101, 31),
        "smooth_red_sandstone" => rgb(181, 98, 31),
        "quartz_block" | "smooth_quartz" => rgb(236, 230, 223),
        "quartz_bricks" | "chiseled_quartz_block" => rgb(233, 228, 220),
        "quartz_pillar" => rgb(235, 230, 224),
        "prismarine" => rgb(99, 156, 151),
        "prismarine_bricks" => rgb(99, 171, 158),
        "dark_prismarine" => rgb(51, 91, 75),
        "sea_lantern" => rgb(172, 199, 190),
        "purpur_block" | "purpur_pillar" => rgb(170, 127, 170),
        "end_stone" => rgb(219, 222, 158),
        "end_stone_bricks" => rgb(218, 224, 162),
        "netherrack" => rgb(97, 38, 38),
        "basalt" => rgb(80, 81, 86),
        "polished_basalt" => rgb(99, 98, 100),
        "smooth_basalt" => rgb(72, 72, 78),
        "obsidian" => rgb(15, 10, 24),
        "crying_obsidian" => rgb(32, 10, 60),
        "glowstone" => rgb(171, 131, 84),
        "magma_block" => rgb(142, 63, 31),
        "soul_sand" => rgb(81, 62, 50),
        "soul_soil" => rgb(75, 57, 46),
        "crimson_nylium" => rgb(130, 31, 31),
        "warped_nylium" => rgb(43, 114, 101),
        "nether_wart_block" => rgb(114, 2, 2),
        "warped_wart_block" => rgb(22, 119, 121),
        "shroomlight" => rgb(240, 146, 70),
        "crimson_roots" | "crimson_fungus" | "nether_sprouts" => rgb(126, 8, 41),
        "warped_roots" | "warped_fungus" => rgb(20, 138, 124),
        "iron_block" => rgb(220, 220, 220),
        "gold_block" => rgb(246, 208, 61),
        "diamond_block" => rgb(98, 237, 228),
        "emerald_block" => rgb(42, 203, 87),
        "lapis_block" => rgb(30, 67, 140),
        "redstone_block" => rgb(175, 24, 5),
        "coal_block" => rgb(16, 15, 15),
        "amethyst_block" | "budding_amethyst" => rgb(133, 97, 191),
        "raw_iron_block" => rgb(166, 135, 107),
        "raw_gold_block" => rgb(221, 169, 46),
        "terracotta" => rgb(152, 94, 68),
        "bone_block" => rgb(229, 225, 207),
        "resin_block" | "resin_bricks" | "chiseled_resin_bricks" => rgb(206, 100, 31),

        // soil / ground
        "grass_block" => rgb(96, 140, 56),
        "dirt" => rgb(134, 96, 67),
        "coarse_dirt" => rgb(119, 85, 59),
        "rooted_dirt" => rgb(144, 103, 76),
        "podzol" => rgb(92, 63, 24),
        "mud" => rgb(60, 57, 60),
        "mycelium" => rgb(111, 99, 105),
        "dirt_path" | "grass_path" => rgb(148, 121, 65),
        "farmland" => rgb(81, 44, 15),
        "gravel" => rgb(131, 127, 126),
        "sand" | "suspicious_sand" => rgb(219, 207, 163),
        "red_sand" => rgb(190, 102, 33),
        "suspicious_gravel" => rgb(128, 124, 122),
        "clay" => rgb(160, 166, 179),
        "moss_block" | "moss_carpet" => rgb(89, 109, 45),
        "pale_moss_block" | "pale_moss_carpet" => rgb(106, 112, 104),
        "mangrove_roots" => rgb(75, 60, 40),
        "muddy_mangrove_roots" => rgb(68, 58, 48),

        // fluids, snow, ice
        "lava" => rgb(207, 92, 20),
        "snow" | "snow_block" | "powder_snow" => rgb(249, 254, 254),
        "ice" | "frosted_ice" => [145, 183, 253, 200],
        "packed_ice" => rgb(141, 180, 250),
        "blue_ice" => rgb(116, 167, 253),

        // glass
        "glass" | "glass_pane" => [200, 220, 230, 80],
        "tinted_glass" => [44, 38, 46, 200],

        // farm & plants
        "hay_block" => rgb(166, 136, 38),
        "pumpkin" | "carved_pumpkin" => rgb(198, 118, 24),
        "jack_o_lantern" => rgb(214, 152, 52),
        "melon" => rgb(111, 145, 30),
        "pumpkin_stem" | "melon_stem" | "attached_pumpkin_stem" | "attached_melon_stem" => rgb(110, 140, 50),
        "wheat" => rgb(170, 160, 70),
        "carrots" => rgb(80, 140, 40),
        "potatoes" => rgb(80, 135, 45),
        "beetroots" => rgb(90, 125, 45),
        "sweet_berry_bush" => rgb(60, 100, 40),
        "short_grass" | "grass" | "tall_grass" | "fern" | "large_fern" | "bush" | "short_dry_grass"
        | "tall_dry_grass" => PLANT_GREEN,
        "dandelion" | "poppy" | "blue_orchid" | "allium" | "azure_bluet" | "oxeye_daisy" | "cornflower"
        | "lily_of_the_valley" | "sunflower" | "lilac" | "rose_bush" | "peony" | "torchflower" | "pink_petals"
        | "wither_rose" | "pitcher_plant" | "open_eyeblossom" | "closed_eyeblossom" | "wildflowers"
        | "spore_blossom" | "firefly_bush" => rgb(95, 140, 55),
        "leaf_litter" => rgb(140, 100, 50),
        "dead_bush" => rgb(107, 79, 40),
        "vine" | "glow_lichen" => rgb(60, 100, 30),
        "lily_pad" => rgb(32, 128, 48),
        "sugar_cane" => rgb(148, 192, 101),
        "cactus" => rgb(85, 127, 43),
        "bamboo" | "bamboo_sapling" => rgb(93, 144, 19),
        "kelp" | "kelp_plant" | "seagrass" | "tall_seagrass" => rgb(50, 110, 30),
        "azalea" | "flowering_azalea" => rgb(95, 120, 45),
        "flowering_azalea_leaves" => rgb(100, 112, 60),
        "mushroom_stem" => rgb(203, 196, 185),
        "brown_mushroom_block" => rgb(149, 111, 81),
        "red_mushroom_block" => rgb(200, 46, 45),
        "dried_kelp_block" => rgb(50, 58, 36),

        // lights
        "lantern" => rgb(220, 170, 90),
        "soul_lantern" | "soul_torch" | "soul_wall_torch" => rgb(100, 180, 190),
        "torch" | "wall_torch" => rgb(230, 190, 90),
        "campfire" => rgb(160, 100, 50),
        "soul_campfire" => rgb(90, 140, 150),
        "candle" => rgb(230, 200, 150),
        "redstone_lamp" => rgb(150, 90, 50),
        "end_rod" => rgb(230, 225, 215),

        // furniture / utility
        "chest" | "trapped_chest" => rgb(162, 115, 40),
        "ender_chest" => rgb(40, 50, 50),
        "barrel" => rgb(134, 100, 58),
        "crafting_table" => rgb(129, 96, 58),
        "furnace" | "dispenser" | "dropper" | "observer" => rgb(110, 110, 110),
        "smoker" => rgb(107, 95, 82),
        "blast_furnace" => rgb(80, 80, 82),
        "bookshelf" => rgb(117, 94, 59),
        "chiseled_bookshelf" => rgb(130, 105, 70),
        "anvil" | "chipped_anvil" | "damaged_anvil" => rgb(70, 70, 70),
        "cauldron" | "water_cauldron" | "lava_cauldron" | "powder_snow_cauldron" => rgb(70, 70, 75),
        "composter" => rgb(115, 80, 45),
        "lectern" => rgb(170, 135, 85),
        "loom" => rgb(142, 119, 91),
        "cartography_table" => rgb(103, 87, 67),
        "fletching_table" => rgb(197, 180, 133),
        "smithing_table" => rgb(57, 58, 70),
        "stonecutter" => rgb(123, 118, 111),
        "grindstone" => rgb(142, 142, 142),
        "bell" => rgb(252, 229, 96),
        "brewing_stand" => rgb(120, 100, 80),
        "enchanting_table" => rgb(50, 40, 60),
        "flower_pot" | "decorated_pot" => rgb(124, 68, 53),
        "scaffolding" => rgb(170, 134, 74),
        "ladder" => rgb(125, 98, 58),
        "iron_bars" => [136, 139, 135, 200],
        "chain" | "iron_chain" => rgb(60, 65, 80),
        "iron_door" => rgb(194, 193, 193),
        "iron_trapdoor" => rgb(202, 202, 202),
        "cobweb" => [228, 233, 234, 120],
        "rail" | "powered_rail" | "detector_rail" | "activator_rail" => rgb(120, 110, 95),
        "hopper" => rgb(64, 64, 67),
        "target" => rgb(226, 170, 157),
        "note_block" => rgb(88, 58, 40),
        "jukebox" => rgb(94, 64, 47),
        "beehive" => rgb(180, 146, 90),
        "bee_nest" => rgb(200, 160, 70),
        "tnt" => rgb(180, 60, 50),
        "sponge" | "wet_sponge" => rgb(195, 192, 74),
        "slime_block" => rgb(112, 192, 91),
        "honey_block" => rgb(251, 185, 52),
        "lodestone" => rgb(147, 149, 152),
        "respawn_anchor" => rgb(60, 30, 90),
        "command_block" => rgb(180, 136, 108),
        "chain_command_block" => rgb(131, 161, 147),
        "repeating_command_block" => rgb(129, 111, 176),
        "crafter" => rgb(110, 98, 92),
        "piston" | "sticky_piston" | "piston_head" => rgb(153, 127, 85),
        "heavy_weighted_pressure_plate" => rgb(220, 220, 220),
        "light_weighted_pressure_plate" => rgb(246, 208, 61),
        "tripwire" => [200, 200, 200, 120],
        "tripwire_hook" => rgb(140, 130, 120),
        "redstone_wire" => rgb(150, 20, 10),
        "redstone_torch" | "redstone_wall_torch" => rgb(190, 40, 20),
        "lever" => rgb(110, 100, 90),
        "armor_stand" => rgb(140, 110, 70),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heuristics_inherit_base_material() {
        let planks = block_color("minecraft:spruce_planks");
        assert_eq!(block_color("minecraft:spruce_stairs[facing=north]"), planks);
        assert_eq!(block_color("spruce_trapdoor"), planks);
        assert_eq!(block_color("minecraft:stone_brick_stairs"), block_color("stone_bricks"));
        assert_eq!(block_color("polished_blackstone_brick_wall"), block_color("polished_blackstone_bricks"));
        assert_eq!(block_color("quartz_slab"), block_color("quartz_block"));
        assert_eq!(block_color("red_nether_brick_stairs"), block_color("red_nether_bricks"));
        assert_eq!(block_color("light_blue_wool"), WOOL[3]);
        assert_eq!(block_color("blue_wool"), WOOL[11]);
        assert_eq!(block_color("red_bed[part=head]"), WOOL[14]);
        assert_eq!(block_color("stripped_oak_log"), rgb(177, 144, 86));
        assert_eq!(block_color("dark_oak_planks"), rgb(66, 43, 20));
        assert_eq!(block_color("waxed_oxidized_cut_copper_stairs"), rgb(82, 162, 132));
        assert_eq!(block_color("red_sandstone_slab"), block_color("red_sandstone"));
        assert!(block_color("blue_stained_glass_pane")[3] < 255);
        assert!(is_water("minecraft:water[level=0]"));
        assert!(is_air("minecraft:cave_air"));
        assert!(lookup("oak_shelf").is_some());
        assert!(lookup("bamboo_mosaic_slab").is_some());
        assert!(lookup("mossy_stone_brick_wall").is_some());
    }

    #[test]
    fn unknown_ids_fall_back_and_are_recorded() {
        let c = block_color("minecraft:totally_made_up_block");
        assert_eq!(c, block_color("totally_made_up_block"));
        assert_eq!(c[3], 255);
        assert!(unknown_ids().iter().any(|s| s == "totally_made_up_block"));
    }
}
