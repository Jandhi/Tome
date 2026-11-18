//! Garden features: small set pieces a garden places whole (a well, a
//! scarecrow, a shrine, a stone lantern), each described in
//! `data/gardens/features/*.yaml` as blocks at offsets, the way furniture is.
//!
//! A feature is written facing north, toward the house it belongs to: `x` runs
//! east, `z` south, and `y` up from the ground (`-1` is the ground's top
//! block, `0` the first block above it). When it is built it is turned so its
//! north faces the house; offsets turn with it, and so do block states that
//! name a side (`facing`, `axis`, a vine's or fence's `north=true`, ...).
//! See `data/gardens/README.md` for the format.

use std::collections::BTreeMap;

use serde_derive::Deserialize;

use crate::editor::Editor;
use crate::geometry::{Point2D, Point3D};
use crate::minecraft::string_to_block;
use crate::noise::RNG;
use crate::util::HashMap;

use super::theme::Theme;

/// Every garden feature, by name.
#[derive(Debug, Clone, Default)]
pub struct GardenFeatures {
    pub features: HashMap<String, GardenFeature>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GardenFeature {
    /// The square of clear bed it needs round its centre: `1` is 3x3, `2` 5x5.
    #[serde(default = "one")]
    pub radius: i32,
    /// Its blocks, in the order they are placed.
    #[serde(default)]
    pub parts: Vec<Part>,
    /// A shape built by code instead of (or as well as) `parts`.
    #[serde(default)]
    pub builtin: Option<Builtin>,
}

fn one() -> i32 {
    1
}

/// Shapes too irregular to list block by block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Builtin {
    /// Three to five bamboo stalks of four to seven blocks in the 3x3 round the centre.
    BambooClump,
}

/// Blocks stacked on one or more cells. Each cell rolls `chance` (percent) on
/// its own; `one_of` picks one of several stacks (by `weight`) per cell
/// instead of `stack`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    /// `[x, z]` offsets from the centre.
    #[serde(default)]
    pub at: Option<[i32; 2]>,
    #[serde(default)]
    pub cells: Vec<[i32; 2]>,
    #[serde(default)]
    pub chance: Option<i32>,
    /// Height above ground -> block.
    #[serde(default)]
    pub stack: BTreeMap<i32, BlockSpec>,
    #[serde(default)]
    pub one_of: Vec<Choice>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    #[serde(default = "one")]
    pub weight: i32,
    pub stack: BTreeMap<i32, BlockSpec>,
}

/// A block id (`"minecraft:lantern"`, `"$wood_fence"`), or one placed over
/// whatever is there (`{block: ..., force: true}`; ground and anything
/// already placed are only replaced when forced).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum BlockSpec {
    Id(String),
    Full {
        block: String,
        #[serde(default)]
        force: bool,
    },
}

impl BlockSpec {
    fn parts(&self) -> (&str, bool) {
        match self {
            BlockSpec::Id(s) => (s, false),
            BlockSpec::Full { block, force } => (block, *force),
        }
    }
}

impl GardenFeatures {
    /// Every `data/gardens/features/*.yaml`, merged.
    pub fn load() -> anyhow::Result<GardenFeatures> {
        let features: HashMap<String, GardenFeature> = crate::data::load_yaml_dir("gardens/features")
            .map_err(|e| anyhow::anyhow!("data/gardens/features: {e}"))?;
        // Check every block now, so a typo fails at load, not mid-town.
        for (name, f) in &features {
            for part in &f.parts {
                let stacks = std::iter::once(&part.stack).chain(part.one_of.iter().map(|c| &c.stack));
                for stack in stacks {
                    for spec in stack.values() {
                        let (id, _) = spec.parts();
                        let probe = resolve(id, &Theme::for_culture(crate::generator::buildings_v2::Culture::Medieval), &mut RNG::new(0));
                        if string_to_block(&probe).is_none() {
                            anyhow::bail!("garden feature {name}: bad block {id:?}");
                        }
                    }
                }
                if part.at.is_none() && part.cells.is_empty() {
                    anyhow::bail!("garden feature {name}: a part names no cell (`at` or `cells`)");
                }
            }
        }
        Ok(GardenFeatures { features })
    }

    pub fn get(&self, name: &str) -> Option<&GardenFeature> {
        self.features.get(name)
    }

    /// Build `name` centred on `c` (ground height `h`, the first air above
    /// it), turned so its north faces `toward_house`. Nothing happens for an
    /// unknown name.
    pub async fn build(&self, editor: &Editor, name: &str, c: Point2D, h: i32, toward_house: Point2D, theme: &Theme, rng: &mut RNG) {
        let Some(f) = self.get(name) else {
            log::warn!("unknown garden feature {name}");
            return;
        };
        let turns = turns_from_north(toward_house);
        for part in &f.parts {
            let cells = part.at.iter().chain(part.cells.iter());
            for &[x, z] in cells {
                if part.chance.is_some_and(|p| !rng.percent(p)) {
                    continue;
                }
                let stack = if part.one_of.is_empty() {
                    &part.stack
                } else {
                    let total: i32 = part.one_of.iter().map(|o| o.weight).sum();
                    let mut roll = rng.rand_i32_range(0, total.max(1));
                    let mut pick = &part.one_of[part.one_of.len() - 1].stack;
                    for o in &part.one_of {
                        if roll < o.weight {
                            pick = &o.stack;
                            break;
                        }
                        roll -= o.weight;
                    }
                    pick
                };
                let p = c + rotate(Point2D::new(x, z), turns);
                for (&y, spec) in stack {
                    let (id, force) = spec.parts();
                    let id = rotate_states(&resolve(id, theme, rng), turns);
                    let Some(block) = string_to_block(&id) else { continue };
                    let at = Point3D::new(p.x, h + y, p.y);
                    if force {
                        editor.place_block_forced(&block, at).await;
                    } else {
                        editor.place_block(&block, at).await;
                    }
                }
            }
        }
        if f.builtin == Some(Builtin::BambooClump) {
            let around: Vec<Point2D> = (-1..=1).flat_map(|dx| (-1..=1).map(move |dz| c + Point2D::new(dx, dz))).collect();
            let height_at = |p: Point2D| editor.world().get_ocean_floor_height_at(p);
            super::garden::bamboo_clump(editor, c, &around, &height_at, rng).await;
        }
    }
}

/// Fill a block id's theme placeholders: `$wood` (the theme's wood family, as
/// in `$wood_fence`), `$stone`, `$wall`, `$slab` (the theme's full ids) and
/// `$rock` (one of the theme's rocks, picked per block).
fn resolve(id: &str, theme: &Theme, rng: &mut RNG) -> String {
    if !id.contains('$') {
        return id.to_string();
    }
    let mut s = id.to_string();
    if s.contains("$rock") {
        let rock = *rng.choose(theme.rocks);
        s = s.replace("$rock", rock);
    }
    s.replace("$stone", theme.stone)
        .replace("$wall", theme.wall)
        .replace("$slab", theme.slab)
        .replace("$wood", &format!("minecraft:{}", theme.wood))
        .replace("minecraft:minecraft:", "minecraft:")
}

/// Quarter turns clockwise (seen from above) that take north onto `dir`.
fn turns_from_north(dir: Point2D) -> u8 {
    match (dir.x, dir.y) {
        (1, 0) => 1,
        (0, 1) => 2,
        (-1, 0) => 3,
        _ => 0,
    }
}

/// `p` turned `turns` quarter turns clockwise about the origin.
fn rotate(p: Point2D, turns: u8) -> Point2D {
    match turns % 4 {
        1 => Point2D::new(-p.y, p.x),
        2 => Point2D::new(-p.x, -p.y),
        3 => Point2D::new(p.y, -p.x),
        _ => p,
    }
}

const SIDES: [&str; 4] = ["north", "east", "south", "west"];

fn turn_side(side: &str, turns: u8) -> Option<&'static str> {
    SIDES.iter().position(|s| *s == side).map(|i| SIDES[(i + turns as usize) % 4])
}

/// Turn the sided states of `id` (`facing`, `axis`, and side flags such as a
/// vine's `north=true`) by `turns` quarter turns clockwise.
fn rotate_states(id: &str, turns: u8) -> String {
    if turns % 4 == 0 {
        return id.to_string();
    }
    let Some(open) = id.find('[') else { return id.to_string() };
    let (name, rest) = id.split_at(open);
    let inner = rest.trim_start_matches('[').trim_end_matches(']');
    let states: Vec<String> = inner
        .split(',')
        .map(|kv| {
            let Some((k, v)) = kv.split_once('=') else { return kv.to_string() };
            let (k, v) = (k.trim(), v.trim());
            if k == "facing" {
                if let Some(t) = turn_side(v, turns) {
                    return format!("facing={t}");
                }
            } else if k == "axis" && turns % 2 == 1 && (v == "x" || v == "z") {
                return format!("axis={}", if v == "x" { "z" } else { "x" });
            } else if let Some(t) = turn_side(k, turns) {
                return format!("{t}={v}");
            }
            format!("{k}={v}")
        })
        .collect();
    format!("{name}[{}]", states.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_turns_offsets_and_states() {
        // North onto east: one clockwise quarter turn.
        assert_eq!(turns_from_north(Point2D::new(1, 0)), 1);
        assert_eq!(rotate(Point2D::new(0, -1), 1), Point2D::new(1, 0));
        assert_eq!(rotate(Point2D::new(1, 0), 1), Point2D::new(0, 1));
        assert_eq!(rotate_states("minecraft:oak_stairs[facing=south]", 1), "minecraft:oak_stairs[facing=west]");
        assert_eq!(rotate_states("minecraft:vine[north=true]", 2), "minecraft:vine[south=true]");
        assert_eq!(rotate_states("minecraft:oak_log[axis=x]", 3), "minecraft:oak_log[axis=z]");
        assert_eq!(rotate_states("minecraft:lantern", 1), "minecraft:lantern");
    }
}
