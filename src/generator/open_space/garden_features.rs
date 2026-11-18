//! Garden features: small set pieces a garden places whole (a well, a
//! scarecrow, a shrine, a stone lantern). Each is built in code here, but
//! takes its blocks from the house's palette by material role (`primary_stone`
//! as a wall, `primary_wood` as a fence, ...), so every feature suits every
//! culture: a well is stone brick in a medieval district and sandstone in a
//! desert one. Which features a culture's gardens get is chosen by name in
//! `data/gardens/<culture>.yaml` (`specials`); [`FEATURES`] lists the names.
//!
//! A feature is written facing north, toward its house: `x` runs east, `z`
//! south, `y` up from the ground (`-1` is the ground's top block, `0` the first
//! block above it). [`Piece`] turns it so its north faces the house, offsets
//! and sided block states (`facing`, `axis`, a vine's `north=true`) alike.

use crate::editor::Editor;
use crate::generator::materials::{Material, MaterialId, MaterialRole, Palette};
use crate::geometry::{Point2D, Point3D};
use crate::minecraft::{string_to_block, BlockForm};
use crate::noise::RNG;
use crate::util::HashMap;

use super::theme::Theme;

/// Every feature by name, with the clear square of bed it needs round its
/// centre (`1` is 3x3, `2` is 5x5).
pub const FEATURES: [(&str, i32); 9] = [
    ("well", 1),
    ("scarecrow", 1),
    ("arbour", 2),
    ("fountain", 2),
    ("pergola", 1),
    ("tsukubai", 1),
    ("shrine", 1),
    ("bamboo", 1),
    ("stone_lantern", 0),
];

/// The clear radius `name` needs, or `None` if there is no such feature.
pub fn radius(name: &str) -> Option<i32> {
    FEATURES.iter().find(|(n, _)| *n == name).map(|(_, r)| *r)
}

/// What a feature builds with: the house's palette (and the materials it
/// names) and the town's open-space theme.
pub struct Kit<'a> {
    pub palette: &'a Palette,
    pub materials: &'a HashMap<MaterialId, Material>,
    pub theme: &'a Theme,
}

impl Kit<'_> {
    /// The palette's block for the first of `choices` it has, else `fallback`.
    fn block(&self, choices: &[(MaterialRole, BlockForm)], fallback: &str, rng: &mut RNG) -> String {
        for (role, form) in choices {
            if let Some(id) = self.palette.get_block(*role, form, self.materials, rng) {
                return id.as_str().to_string();
            }
        }
        fallback.to_string()
    }

    fn wood(&self, form: BlockForm, fallback: &str, rng: &mut RNG) -> String {
        self.block(&[(MaterialRole::PrimaryWood, form)], fallback, rng)
    }

    fn stone(&self, form: BlockForm, rng: &mut RNG) -> String {
        let fallback = match form {
            BlockForm::Wall => self.theme.wall,
            BlockForm::Slab => self.theme.slab,
            _ => self.theme.stone,
        };
        self.block(&[(MaterialRole::PrimaryStone, form), (MaterialRole::SecondaryStone, form)], fallback, rng)
    }
}

/// Build feature `name` centred on `c` (ground height `h`, the first air above
/// it), turned so its north faces `toward_house`. Returns false for an
/// unknown name.
pub async fn build(editor: &Editor, name: &str, c: Point2D, h: i32, toward_house: Point2D, kit: &Kit<'_>, rng: &mut RNG) -> bool {
    let p = Piece { editor, c, h, turns: turns_from_north(toward_house) };
    use BlockForm as F;
    match name {
        "well" => {
            // A ring of stone wall round a water hole, two wooden posts and a
            // little gable roof with a chain hanging under it.
            let ring = kit.block(&[(MaterialRole::SecondaryStone, F::Wall), (MaterialRole::PrimaryStone, F::Wall)], kit.theme.wall, rng);
            let rubble = kit.block(&[(MaterialRole::SecondaryStone, F::Block)], kit.theme.stone, rng);
            let post = kit.wood(F::Fence, "minecraft:oak_fence", rng);
            let planks = kit.wood(F::Block, "minecraft:oak_planks", rng);
            let stairs = kit.wood(F::Stairs, "minecraft:oak_stairs", rng);
            p.force(0, 0, -3, &rubble).await;
            p.force(0, 0, -2, "minecraft:water").await;
            p.force(0, 0, -1, "minecraft:water").await;
            for (x, z) in RING {
                p.force(x, z, -1, &rubble).await;
                p.put(x, z, 0, &ring).await;
            }
            for x in [-1, 1] {
                p.force(x, 0, 0, &rubble).await;
                p.put(x, 0, 1, &post).await;
                p.put(x, 0, 2, &post).await;
            }
            for x in -1..=1 {
                p.put(x, 0, 3, &planks).await;
                p.put(x, 1, 3, &format!("{stairs}[facing=north]")).await;
                p.put(x, -1, 3, &format!("{stairs}[facing=south]")).await;
            }
            p.put(0, 0, 2, "minecraft:chain").await;
        }
        "scarecrow" => {
            // A scarecrow looking at the house, in a little patch of wheat.
            let post = kit.wood(F::Fence, "minecraft:oak_fence", rng);
            for (x, z) in RING {
                if rng.percent(70) {
                    p.force(x, z, -1, "minecraft:farmland[moisture=7]").await;
                    p.put(x, z, 0, "minecraft:wheat[age=7]").await;
                }
            }
            p.put(0, 0, 0, &post).await;
            p.put(0, 0, 1, "minecraft:hay_block").await;
            p.put(0, 0, 2, "minecraft:carved_pumpkin[facing=north]").await;
            p.put(-1, 0, 1, &post).await;
            p.put(1, 0, 1, &post).await;
        }
        "arbour" => {
            // A two-seat bench under a flowering arch, looking at the house, a
            // rose bush behind each post.
            let post = kit.wood(F::Fence, "minecraft:oak_fence", rng);
            let seat = kit.wood(F::Stairs, "minecraft:oak_stairs", rng);
            for x in [0, 1] {
                p.put(x, 0, 0, &format!("{seat}[facing=south]")).await;
            }
            for x in [-1, 2] {
                p.put(x, 0, 0, &post).await;
                p.put(x, 0, 1, &post).await;
                p.put(x, 1, 0, "minecraft:rose_bush[half=lower]").await;
                p.put(x, 1, 1, "minecraft:rose_bush[half=upper]").await;
            }
            for x in -1..=2 {
                p.put(x, 0, 2, "minecraft:flowering_azalea_leaves[persistent=true]").await;
            }
        }
        "fountain" => {
            // A sunken stone basin with a lantern on a post in the middle (no
            // spout: a water source above ground would flood the garden).
            let edge = kit.stone(F::Block, rng);
            let centre = kit.block(&[(MaterialRole::PrimaryStone, F::Chiseled), (MaterialRole::PrimaryStone, F::Block)], kit.theme.stone_accent, rng);
            let pillar = kit.stone(F::Wall, rng);
            for x in -2i32..=2 {
                for z in -2i32..=2 {
                    if x.abs() == 2 || z.abs() == 2 {
                        p.force(x, z, -1, &edge).await;
                    } else if (x, z) != (0, 0) {
                        p.force(x, z, -2, &edge).await;
                        p.force(x, z, -1, "minecraft:water").await;
                    }
                }
            }
            p.force(0, 0, -2, &edge).await;
            p.force(0, 0, -1, &centre).await;
            p.put(0, 0, 0, &pillar).await;
            p.put(0, 0, 1, "minecraft:lantern").await;
        }
        "pergola" => {
            // Four posts under a slatted roof hung with vines, a pot in the
            // shade, on a paved floor.
            let post = kit.wood(F::Fence, "minecraft:oak_fence", rng);
            let slab = kit.wood(F::Slab, "minecraft:oak_slab", rng);
            let floor = kit.stone(F::Block, rng);
            for x in -1..=1 {
                for z in -1..=1 {
                    p.force(x, z, -1, &floor).await;
                }
            }
            for (x, z) in [(-1, -1), (1, -1), (-1, 1), (1, 1)] {
                for y in 0..=2 {
                    p.put(x, z, y, &post).await;
                }
            }
            for x in -1..=1 {
                for z in -1..=1 {
                    p.put(x, z, 3, &slab).await;
                }
            }
            p.put(0, 0, 0, "minecraft:decorated_pot").await;
            for (x, z) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                if rng.percent(70) {
                    p.put(x, z, 2, "minecraft:vine[up=true]").await;
                }
            }
        }
        "tsukubai" => {
            // A stone water basin brimming with water, a bamboo spout behind
            // it, a rock or a fern either side, on moss.
            for x in -1..=1 {
                for z in -1..=1 {
                    p.force(x, z, -1, "minecraft:moss_block").await;
                }
            }
            p.put(0, 0, 0, "minecraft:water_cauldron[level=3]").await;
            p.put(0, 1, 0, "minecraft:bamboo[age=1,leaves=none,stage=0]").await;
            p.put(0, 1, 1, "minecraft:bamboo[age=1,leaves=none,stage=0]").await;
            for x in [-1, 1] {
                if rng.percent(60) {
                    let rock = *rng.choose(kit.theme.rocks);
                    p.put(x, 0, 0, rock).await;
                } else {
                    p.put(x, 0, 0, "minecraft:fern").await;
                }
            }
        }
        "shrine" => {
            // A little shrine on a stone plinth facing the house: a wooden box
            // under a roof-material stair, a candle offered in front, moss behind.
            let plinth = kit.stone(F::Block, rng);
            let body = kit.block(&[(MaterialRole::WoodPillar, F::Block), (MaterialRole::PrimaryWood, F::Block)], "minecraft:stripped_dark_oak_wood", rng);
            let roof = kit.block(&[(MaterialRole::PrimaryRoof, F::Stairs), (MaterialRole::PrimaryWood, F::Stairs)], "minecraft:dark_oak_stairs", rng);
            p.put(0, 0, 0, &plinth).await;
            p.put(0, 0, 1, &body).await;
            p.put(0, 0, 2, &format!("{roof}[facing=south]")).await;
            p.put(0, -1, 0, "minecraft:white_candle[candles=2,lit=true]").await;
            p.force(0, 1, -1, "minecraft:moss_block").await;
        }
        "bamboo" => {
            // Three to five bamboo stalks of four to seven blocks.
            let around: Vec<Point2D> = (-1..=1).flat_map(|dx| (-1..=1).map(move |dz| c + Point2D::new(dx, dz))).collect();
            let height_at = |q: Point2D| editor.world().get_ocean_floor_height_at(q);
            super::garden::bamboo_clump(editor, c, &around, &height_at, rng).await;
        }
        "stone_lantern" => {
            // A lantern on a stone post (a toro).
            let post = kit.stone(F::Wall, rng);
            p.put(0, 0, 0, &post).await;
            p.put(0, 0, 1, "minecraft:lantern").await;
        }
        _ => {
            log::warn!("unknown garden feature {name}");
            return false;
        }
    }
    true
}

/// The eight cells round the centre.
const RING: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

/// A feature being built: its centre, ground height and turn.
struct Piece<'a> {
    editor: &'a Editor,
    c: Point2D,
    h: i32,
    turns: u8,
}

impl Piece<'_> {
    /// Place `id` at `(x, z)` from the centre, `y` above the ground, unless
    /// something is there already.
    async fn put(&self, x: i32, z: i32, y: i32, id: &str) {
        self.place(x, z, y, id, false).await;
    }

    /// Place `id` over whatever is there (ground included).
    async fn force(&self, x: i32, z: i32, y: i32, id: &str) {
        self.place(x, z, y, id, true).await;
    }

    async fn place(&self, x: i32, z: i32, y: i32, id: &str, force: bool) {
        let q = self.c + rotate(Point2D::new(x, z), self.turns);
        let Some(block) = string_to_block(&rotate_states(id, self.turns)) else {
            log::warn!("garden feature: bad block {id}");
            return;
        };
        let at = Point3D::new(q.x, self.h + y, q.y);
        if force {
            self.editor.place_block_forced(&block, at).await;
        } else {
            self.editor.place_block(&block, at).await;
        }
    }
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

    #[test]
    fn every_culture_style_names_real_features() {
        for c in ["medieval", "desert", "japanese"] {
            let style: super::super::garden_style::GardenStyle =
                crate::data::load_yaml(&format!("gardens/{c}.yaml")).expect("style loads");
            style.check().expect("features exist");
        }
    }
}
