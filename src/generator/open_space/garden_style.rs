//! A culture's garden style, read from `data/gardens/<culture>.yaml`: every
//! choice a town house's garden makes that is only a block, a list or an odds
//! (boundary, ground, paths, props, plants, which layouts the culture uses
//! and how often). The garden's shapes and placement rules
//! stay in `garden.rs`; a new culture is a new YAML file, as long as it uses
//! the layouts that exist. See `data/gardens/README.md`.

use serde_derive::Deserialize;

use crate::generator::buildings_v2::Culture;
use crate::geometry::Point2D;

use super::garden::{blotch, hash01, GardenLayout};

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
    pub post: String,
    pub gate: String,
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
    pub base: (String, String),
    pub body: (String, String),
    /// Coping along the top.
    pub coping: String,
    /// Pier blocks, and the cap on corner piers.
    pub pier: String,
    pub pier_cap: String,
    /// Climbing plant leaves over the coping, `[a, b]` mixed; absent for a bare wall.
    #[serde(default)]
    pub creeper: Option<(String, String)>,
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

    /// The layouts in order, without weights.
    pub fn layout_list(&self) -> Vec<GardenLayout> {
        self.layouts.iter().map(|w| w.layout).collect()
    }
}
