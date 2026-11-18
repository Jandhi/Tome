//! A culture's garden style, read from `data/gardens/<culture>.yaml`: every
//! choice a town house's garden makes that is only a block, a list or an odds
//! (boundary, ground, paths, props, plants, which layouts, pieces and rare
//! features the culture uses and how often). Blocks can be plain ids or
//! roles from the house's palette ([`BlockRef`]). The garden's shapes and placement rules
//! stay in `garden.rs`; a new culture is a new YAML file, as long as it uses
//! the layouts that exist. See `data/gardens/README.md`.

use serde_derive::Deserialize;

use crate::generator::buildings_v2::Culture;
use crate::geometry::Point2D;

use super::garden::{blotch, hash01, GardenLayout};
use super::garden_features::Kit;
use crate::generator::materials::MaterialRole;
use crate::minecraft::BlockForm;
use crate::noise::RNG;
use crate::util::HashMap;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GardenStyle {
    /// The layouts gardens are laid out as, with their weights (rolled in this
    /// order; one entry means no roll).
    pub layouts: Vec<WeightedLayout>,
    /// When a garden is walled and closed instead of fenced.
    pub walled: WalledRule,
    /// The fence round an open garden.
    pub fence: FenceStyle,
    /// The wall round a walled garden.
    pub wall: WallStyle,
    /// The garden path: one block, or two laid alternately (stepping stones).
    pub path: Vec<String>,
    /// Patches over the lawn, tried in order; the theme's ground where none
    /// matches.
    #[serde(default)]
    pub lawn: Vec<Patch>,
    /// A work yard's worn ground near the house, tried in order (the theme's
    /// ground where none matches); empty leaves the yard to `lawn`.
    #[serde(default)]
    pub yard_wear: Vec<Patch>,
    /// Props against the house wall (a work yard's first is always a woodpile).
    pub wall_props: Vec<String>,
    /// A work yard's scattered extras.
    pub yard_extras: Vec<String>,
    /// Tufts on the leftover lawn, by per-cell noise: the first whose `below`
    /// the noise is under. `$flower` picks from `flowers` by position.
    #[serde(default)]
    pub tufts: Vec<Tuft>,
    /// Kitchen garden crops.
    #[serde(default)]
    pub crops: Vec<String>,
    /// Flower bed and orchard flowers.
    #[serde(default)]
    pub flowers: Vec<String>,
    /// A courtyard's pots.
    #[serde(default)]
    pub pots: Vec<String>,
    /// Shade trees: whether layouts other than courtyard and zen plant them,
    /// their wood (`oak`, `spruce`, `birch`, `dark_oak`, `cherry`), and the
    /// block laid under the trunk (if any).
    pub trees: TreeStyle,
    /// Rare features (names from `garden_features::FEATURES`), picked
    /// evenly, and how often a garden gets one (percent).
    #[serde(default)]
    pub specials: Vec<String>,
    #[serde(default)]
    pub special_chance: i32,
    /// The pieces each layout sets (`zen: [stone_lantern, azalea_shrubs]`),
    /// in order: feature names (`garden_features::FEATURES`) or the groups in
    /// [`PIECE_GROUPS`]. A layout left out sets its usual pieces
    /// ([`default_pieces`]).
    #[serde(default)]
    pub pieces: HashMap<GardenLayout, Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeightedLayout {
    pub layout: GardenLayout,
    #[serde(default = "one")]
    pub weight: i32,
}

fn one() -> i32 {
    1
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalledRule {
    /// Every garden walled (no roll).
    #[serde(default)]
    pub always: bool,
    /// Houses with at least this footprint (frontage x depth) count as big.
    #[serde(default)]
    pub big_house_area: i32,
    /// Chance (percent) of a wall behind a big house, and behind any other.
    #[serde(default)]
    pub big: i32,
    #[serde(default)]
    pub other: i32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FenceStyle {
    /// Post and gate blocks (the gate gets its `facing`).
    pub post: BlockRef,
    pub gate: BlockRef,
    /// Chance (percent) of a lantern on one corner post; absent for never
    /// (and no roll).
    #[serde(default)]
    pub corner_lantern_chance: Option<i32>,
}

/// How a walled garden's wall is built (see `garden::courtyard_wall`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WallStyle {
    /// Bottom course and upper course, each `[block, weathered block]`.
    pub base: (BlockRef, BlockRef),
    pub body: (BlockRef, BlockRef),
    /// Coping along the top.
    pub coping: BlockRef,
    /// Pier blocks, and the cap on corner piers.
    pub pier: BlockRef,
    pub pier_cap: BlockRef,
    /// Climbing plant leaves over the coping, `[a, b]` mixed; absent for a bare wall.
    #[serde(default)]
    pub creeper: Option<(String, String)>,
}

/// A block: a plain id (`"minecraft:bamboo_fence"`), or a material role and
/// form taken from the house's palette (`{role: primary_wood, form: fence}`),
/// with an optional `fallback` id for a palette that has no such block.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum BlockRef {
    Id(String),
    Role {
        role: MaterialRole,
        #[serde(default)]
        form: Option<BlockForm>,
        #[serde(default)]
        fallback: Option<String>,
    },
}

impl BlockRef {
    /// The block id this names for a house with `kit`'s palette.
    pub fn resolve(&self, kit: &Kit<'_>, rng: &mut RNG) -> String {
        match self {
            BlockRef::Id(id) => id.clone(),
            BlockRef::Role { role, form, fallback } => {
                let form = form.clone().unwrap_or(BlockForm::Block);
                if let Some(id) = kit.palette.get_block(*role, &form, kit.materials, rng) {
                    return id.as_str().to_string();
                }
                if let Some(f) = fallback {
                    return f.clone();
                }
                match form {
                    BlockForm::Slab => kit.theme.slab.to_string(),
                    BlockForm::Wall => kit.theme.wall.to_string(),
                    BlockForm::Stairs => "minecraft:stone_brick_stairs".to_string(),
                    BlockForm::Fence => "minecraft:oak_fence".to_string(),
                    BlockForm::FenceGate => "minecraft:oak_fence_gate".to_string(),
                    _ => kit.theme.stone.to_string(),
                }
            }
        }
    }
}

/// Piece groups a layout can list besides features: `azalea_shrubs` (clipped
/// shrubs against the fence), `palms` (date palms at the back), `pots` (pots
/// and potted plants about the bed).
pub const PIECE_GROUPS: [&str; 3] = ["azalea_shrubs", "palms", "pots"];

/// The pieces a layout sets when its culture's file doesn't list them.
pub fn default_pieces(layout: GardenLayout) -> Vec<String> {
    let names: &[&str] = match layout {
        GardenLayout::Zen | GardenLayout::Pond => &["stone_lantern", "azalea_shrubs"],
        GardenLayout::Moss => &["bamboo", "stone_lantern", "azalea_shrubs"],
        GardenLayout::Courtyard => &["palms", "bench", "pots"],
        _ => &[],
    };
    names.iter().map(|s| s.to_string()).collect()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeStyle {
    pub shade_trees: bool,
    pub wood: String,
    #[serde(default)]
    pub soil: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoiseKind {
    /// Smooth value noise (`scale` cells between hashed corners).
    Blotch,
    /// Per-cell hash.
    Hash,
}

/// A noise patch: where `noise` (salted by the garden's own salt XOR `salt`)
/// is above `above`, lay `blocks[0]`, or with two blocks, `blocks[0]` where a
/// second per-cell hash (salt `mix_salt`) is above `mix_above` and `blocks[1]`
/// elsewhere.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    pub noise: NoiseKind,
    #[serde(default = "one")]
    pub scale: i32,
    #[serde(default)]
    pub salt: u32,
    pub above: f32,
    pub blocks: Vec<String>,
    #[serde(default)]
    pub mix_salt: u32,
    #[serde(default)]
    pub mix_above: f32,
}

impl Patch {
    /// This patch's block at `c`, if it covers it.
    pub fn at(&self, c: Point2D, salt: u32) -> Option<&str> {
        let n = match self.noise {
            NoiseKind::Blotch => blotch(c, self.scale, salt ^ self.salt),
            NoiseKind::Hash => hash01(c.x, c.y, salt ^ self.salt),
        };
        if n <= self.above || self.blocks.is_empty() {
            return None;
        }
        if self.blocks.len() >= 2 && hash01(c.x, c.y, salt ^ self.mix_salt) <= self.mix_above {
            return Some(&self.blocks[1]);
        }
        Some(&self.blocks[0])
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tuft {
    pub below: f32,
    pub block: String,
}

impl GardenStyle {
    /// The style for a culture, from `data/gardens/<culture>.yaml`.
    pub fn for_culture(culture: Culture) -> anyhow::Result<GardenStyle> {
        let name = match culture {
            Culture::Medieval => "medieval",
            Culture::Desert => "desert",
            Culture::Japanese => "japanese",
        };
        let path = format!("gardens/{name}.yaml");
        crate::data::load_yaml(&path).map_err(|e| anyhow::anyhow!("data/{path}: {e}"))
    }

    /// The first patch in `patches` that covers `c`.
    pub fn patch<'a>(patches: &'a [Patch], c: Point2D, salt: u32) -> Option<&'a str> {
        patches.iter().find_map(|p| p.at(c, salt))
    }

    /// Fail if a rare feature names no feature.
    pub fn check(&self) -> anyhow::Result<()> {
        let features: Vec<&str> = super::garden_features::FEATURES.iter().map(|(n, _)| *n).collect();
        for name in &self.specials {
            if super::garden_features::radius(name).is_none() {
                anyhow::bail!("garden style names an unknown feature {name:?}; known: {}", features.join(", "));
            }
        }
        for (layout, names) in &self.pieces {
            for name in names {
                if super::garden_features::radius(name).is_none() && !PIECE_GROUPS.contains(&name.as_str()) {
                    anyhow::bail!(
                        "garden style: {layout:?} lists an unknown piece {name:?}; features: {}; groups: {}",
                        features.join(", "),
                        PIECE_GROUPS.join(", "),
                    );
                }
            }
        }
        Ok(())
    }

    /// The pieces `layout` sets: the style's list, else the layout's usual ones.
    pub fn pieces_for(&self, layout: GardenLayout) -> Vec<String> {
        self.pieces.get(&layout).cloned().unwrap_or_else(|| default_pieces(layout))
    }

    /// A style from a file anywhere (for trying one out).
    pub fn from_path(path: &str) -> anyhow::Result<GardenStyle> {
        let file = std::fs::File::open(path).map_err(|e| anyhow::anyhow!("{path}: {e}"))?;
        serde_yaml::from_reader(file).map_err(|e| anyhow::anyhow!("{path}: {e}"))
    }

    /// The layouts in order, without weights.
    pub fn layout_list(&self) -> Vec<GardenLayout> {
        self.layouts.iter().map(|w| w.layout).collect()
    }
}
