//! Per-block geometry for the renderer: which part of its cell a block fills.
//!
//! Shapes are derived from the block id and state (`type=bottom` slabs,
//! `facing`/`half`/`shape` stairs, `open` doors, …) and expanded into a few
//! axis-aligned boxes in sixteenths of a block, in **world** orientation
//! (`x0, y0, z0, x1, y1, z1`; +X east, +Y up, +Z south). The views turn those
//! boxes into face masks (iso), sub-block coverage (elevations) or surface
//! height shading (top-down).

/// An axis-aligned box inside a cell, in sixteenths: `[x0, y0, z0, x1, y1, z1]`.
pub type Box16 = [u8; 6];

pub const FULL_BOX: Box16 = [0, 0, 0, 16, 16, 16];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    /// Fills the whole cell.
    Full,
    /// A single box: slabs, posts (fences, walls, torches, plants), thin
    /// panels (doors, trapdoors, ladders, carpets), small cubes (lanterns).
    Cuboid(Box16),
    /// Half slab (lower, or upper when `top`) plus a step on the other half
    /// covering the quarters set in `quarters` (bits NW=1, NE=2, SW=4, SE=8).
    Stairs { top: bool, quarters: u8 },
    /// Thin centred panes (glass panes, iron bars) running along X and/or Z.
    Pane { along_x: bool, along_z: bool },
    /// Quarter-width fence post with two rails toward each connected side
    /// (bits `N | E | S | W`).
    Fence { sides: u8 },
    /// Wall: optional half-width post plus low (14/16) or tall arms toward
    /// the connected sides.
    Wall { low: u8, tall: u8, post: bool },
}

// Stair quarters.
const NW: u8 = 1;
const NE: u8 = 2;
const SW: u8 = 4;
const SE: u8 = 8;

// Connected sides (fences, walls, panes).
pub const N: u8 = 1;
pub const E: u8 = 2;
pub const S: u8 = 4;
pub const W: u8 = 8;
const SIDES: [(&str, u8); 4] = [("north", N), ("east", E), ("south", S), ("west", W)];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    North,
    East,
    South,
    West,
}

impl Dir {
    fn parse(s: &str) -> Option<Dir> {
        Some(match s {
            "north" => Dir::North,
            "east" => Dir::East,
            "south" => Dir::South,
            "west" => Dir::West,
            _ => return None,
        })
    }

    fn cw(self) -> Dir {
        match self {
            Dir::North => Dir::East,
            Dir::East => Dir::South,
            Dir::South => Dir::West,
            Dir::West => Dir::North,
        }
    }

    fn ccw(self) -> Dir {
        self.cw().cw().cw()
    }

    fn opposite(self) -> Dir {
        self.cw().cw()
    }

    fn is_x_axis(self) -> bool {
        matches!(self, Dir::East | Dir::West)
    }

    /// The cell one step from `p` in this direction (+Z south, +X east).
    pub fn offset(self, p: crate::geometry::Point3D) -> crate::geometry::Point3D {
        let (dx, dz) = match self {
            Dir::North => (0, -1),
            Dir::South => (0, 1),
            Dir::East => (1, 0),
            Dir::West => (-1, 0),
        };
        crate::geometry::Point3D::new(p.x + dx, p.y, p.z + dz)
    }

    /// The two quarters on this side of the cell.
    fn half(self) -> u8 {
        match self {
            Dir::North => NW | NE,
            Dir::East => NE | SE,
            Dir::South => SW | SE,
            Dir::West => NW | SW,
        }
    }
}

pub fn is_stairs(block: &str) -> bool {
    bare_id(block).ends_with("_stairs")
}

/// `(facing, half == top)` of a stair block string, `None` for anything else.
pub fn stair_info(block: &str) -> Option<(Dir, bool)> {
    if !is_stairs(block) {
        return None;
    }
    let facing = state(block, "facing").and_then(Dir::parse).unwrap_or(Dir::North);
    Some((facing, state(block, "half") == Some("top")))
}

/// Stair shape derived from the neighbours, exactly as Minecraft's
/// `StairsBlock::getStairsShape` does on placement. The generator places
/// stairs without a `shape` state (or with `straight`), and the server works
/// the real one out from block updates, so a hip-roof corner that is an
/// `outer_right` in game is just `facing=east` in our scene. `at(dir)` gives
/// the neighbour's `(facing, top)` when it is a stair.
pub fn infer_stairs(block: &str, at: impl Fn(Dir) -> Option<(Dir, bool)>) -> Option<Shape> {
    let (facing, top) = stair_info(block)?;
    let same_half = |d: Dir| at(d).filter(|&(_, t)| t == top);
    // True unless the stair in direction `d` has our exact orientation.
    let different = |d: Dir| at(d).map_or(true, |(f, t)| f != facing || t != top);
    let quarters = if let Some((f2, _)) = same_half(facing)
        .filter(|&(f2, _)| f2.is_x_axis() != facing.is_x_axis() && different(f2.opposite()))
    {
        // Outer corner: only the quarter shared by our high side and theirs.
        if f2 == facing.ccw() { facing.half() & facing.ccw().half() } else { facing.half() & facing.cw().half() }
    } else if let Some((f3, _)) = same_half(facing.opposite())
        .filter(|&(f3, _)| f3.is_x_axis() != facing.is_x_axis() && different(f3))
    {
        // Inner corner: our high half plus the neighbour's.
        if f3 == facing.ccw() { facing.half() | facing.ccw().half() } else { facing.half() | facing.cw().half() }
    } else {
        facing.half()
    };
    Some(Shape::Stairs { top, quarters })
}

/// Value of `key` in an `id[k=v,...]` block string.
fn state<'a>(block: &'a str, key: &str) -> Option<&'a str> {
    let inner = block.split_once('[')?.1.trim_end_matches(']');
    inner.split(',').find_map(|kv| kv.split_once('=').filter(|(k, _)| k.trim() == key).map(|(_, v)| v.trim()))
}

/// A `t`-thick full-width panel against side `d`, spanning `y0..y1`.
fn panel(d: Dir, t: u8, y0: u8, y1: u8) -> Shape {
    Shape::Cuboid(match d {
        Dir::North => [0, y0, 0, 16, y1, t],
        Dir::South => [0, y0, 16 - t, 16, y1, 16],
        Dir::West => [0, y0, 0, t, y1, 16],
        Dir::East => [16 - t, y0, 0, 16, y1, 16],
    })
}

/// A centred square post of width `w` spanning `y0..y1`.
fn post(w: u8, y0: u8, y1: u8) -> Shape {
    let a = (16 - w) / 2;
    Shape::Cuboid([a, y0, a, a + w, y1, a + w])
}

fn floor(h: u8) -> Shape {
    if h >= 16 {
        Shape::Full
    } else {
        Shape::Cuboid([0, 0, 0, 16, h.max(1), 16])
    }
}

const FLOWERS: [&str; 22] = [
    "dandelion", "poppy", "blue_orchid", "allium", "azure_bluet", "oxeye_daisy", "cornflower", "lily_of_the_valley",
    "torchflower", "wither_rose", "open_eyeblossom", "closed_eyeblossom", "red_tulip", "orange_tulip", "white_tulip",
    "pink_tulip", "dead_bush", "red_mushroom", "brown_mushroom", "crimson_fungus", "warped_fungus", "cactus_flower",
];
const LOW_PLANTS: [&str; 10] = [
    "short_grass", "grass", "fern", "bush", "short_dry_grass", "firefly_bush", "sweet_berry_bush", "seagrass",
    "crimson_roots", "warped_roots",
];
const TALL_PLANTS: [&str; 9] =
    ["tall_grass", "large_fern", "lilac", "rose_bush", "peony", "sunflower", "pitcher_plant", "tall_dry_grass", "tall_seagrass"];
const FLAT: [&str; 10] = [
    "rail", "powered_rail", "detector_rail", "activator_rail", "redstone_wire", "tripwire", "lily_pad", "pink_petals",
    "leaf_litter", "wildflowers",
];

impl Shape {
    /// Shape of a full block string (`minecraft:` prefix and state optional).
    pub fn from_block(block: &str) -> Shape {
        let id = bare_id(block);
        let st = |k: &str| state(block, k);
        let facing = st("facing").and_then(Dir::parse);

        if id.ends_with("_slab") {
            return match st("type") {
                Some("top") => Shape::Cuboid([0, 8, 0, 16, 16, 16]),
                Some("double") => Shape::Full,
                _ => Shape::Cuboid([0, 0, 0, 16, 8, 16]),
            };
        }
        if id.ends_with("_stairs") {
            let f = facing.unwrap_or(Dir::North);
            let quarters = match st("shape") {
                Some("inner_left") => f.half() | f.ccw().half(),
                Some("inner_right") => f.half() | f.cw().half(),
                Some("outer_left") => f.half() & f.ccw().half(),
                Some("outer_right") => f.half() & f.cw().half(),
                _ => f.half(),
            };
            return Shape::Stairs { top: st("half") == Some("top"), quarters };
        }
        if id.ends_with("_fence_gate") {
            return match facing {
                Some(Dir::East | Dir::West) => Shape::Cuboid([7, 3, 0, 9, 15, 16]),
                _ => Shape::Cuboid([0, 3, 7, 16, 15, 9]),
            };
        }
        // Connection bits for fences, walls and panes from `north=…` etc.
        let sides = |want: &dyn Fn(&str) -> bool| {
            SIDES.iter().filter(|(k, _)| st(k).is_some_and(want)).fold(0, |acc, (_, bit)| acc | bit)
        };
        if id.ends_with("_fence") {
            return Shape::Fence { sides: sides(&|v| v == "true") };
        }
        if id.ends_with("_wall") {
            let (low, tall) = (sides(&|v| v == "low"), sides(&|v| v == "tall"));
            // Like Minecraft: a straight run without `up` has no post.
            let post = st("up") != Some("false") || (low | tall).count_ones() == 0;
            return Shape::Wall { low, tall, post };
        }
        if id.ends_with("_pane") || id == "iron_bars" {
            let s = sides(&|v| v == "true");
            // No connections: Minecraft draws a cross-shaped post.
            let (x, z) = if s == 0 { (true, true) } else { (s & (E | W) != 0, s & (N | S) != 0) };
            return Shape::Pane { along_x: x, along_z: z };
        }
        if id.ends_with("_door") {
            let f = facing.unwrap_or(Dir::North);
            let side = if st("open") == Some("true") {
                if st("hinge") == Some("right") {
                    f.cw()
                } else {
                    f.ccw()
                }
            } else {
                f.opposite()
            };
            return panel(side, 3, 0, 16);
        }
        if id.ends_with("_trapdoor") {
            if st("open") == Some("true") {
                return panel(facing.unwrap_or(Dir::North).opposite(), 3, 0, 16);
            }
            return if st("half") == Some("top") {
                Shape::Cuboid([0, 13, 0, 16, 16, 16])
            } else {
                Shape::Cuboid([0, 0, 0, 16, 3, 16])
            };
        }
        if id == "ladder" {
            return panel(facing.unwrap_or(Dir::North).opposite(), 1, 0, 16);
        }
        if id.ends_with("_wall_sign") {
            return panel(facing.unwrap_or(Dir::North).opposite(), 2, 4, 12);
        }
        if id.ends_with("_wall_banner") {
            return panel(facing.unwrap_or(Dir::North).opposite(), 1, 0, 16);
        }
        if id.ends_with("_hanging_sign") {
            return post(10, 0, 10);
        }
        if id.ends_with("_sign") {
            return post(8, 6, 16);
        }
        if id.ends_with("_carpet") || id.ends_with("_pressure_plate") || FLAT.contains(&id) {
            return floor(1);
        }
        if id == "snow" {
            let layers: u8 = st("layers").and_then(|v| v.parse().ok()).unwrap_or(1);
            return floor(layers.saturating_mul(2));
        }
        if id.ends_with("torch") {
            return post(2, 0, 10);
        }
        if id.ends_with("lantern") && id != "sea_lantern" && id != "jack_o_lantern" {
            return if st("hanging") == Some("true") { post(6, 1, 8) } else { post(6, 0, 7) };
        }
        if id.ends_with("candle") {
            return post(2, 0, 6);
        }
        if id.ends_with("_button") || id == "lever" || id == "tripwire_hook" {
            return post(4, 6, 10);
        }
        if id == "end_rod" || id == "lightning_rod" || id.ends_with("chain") || id == "bamboo" || id == "brewing_stand" {
            return post(4, 0, 16);
        }
        if id == "flower_pot" || id.starts_with("potted_") {
            return post(6, 0, 6);
        }
        if id.ends_with("_bed") {
            return Shape::Cuboid([0, 0, 0, 16, 9, 16]);
        }
        if matches!(id, "chest" | "trapped_chest" | "ender_chest") {
            return Shape::Cuboid([1, 0, 1, 15, 14, 15]);
        }
        if matches!(id, "campfire" | "soul_campfire") {
            return floor(7);
        }
        if id == "stonecutter" {
            return floor(9);
        }
        if id == "enchanting_table" {
            return floor(12);
        }
        if FLOWERS.contains(&id) || id.ends_with("_sapling") || id.ends_with("_tulip") || id.ends_with("_propagule") {
            return post(6, 0, 10);
        }
        if LOW_PLANTS.contains(&id) {
            return post(12, 0, 9);
        }
        if TALL_PLANTS.contains(&id) {
            return if st("half") == Some("upper") { post(12, 0, 12) } else { post(12, 0, 16) };
        }
        if id == "sugar_cane" {
            return post(10, 0, 16);
        }
        if matches!(id, "wheat" | "carrots" | "potatoes" | "beetroots") {
            let max = if id == "beetroots" { 3 } else { 7 };
            let age: u8 = st("age").and_then(|v| v.parse().ok()).unwrap_or(max);
            return Shape::Cuboid([1, 0, 1, 15, 2 + 14 * age.min(max) / max, 15]);
        }
        if id == "vine" {
            for (k, d) in [("north", Dir::North), ("south", Dir::South), ("west", Dir::West), ("east", Dir::East)] {
                if st(k) == Some("true") {
                    return panel(d, 1, 0, 16);
                }
            }
            return Shape::Cuboid([0, 15, 0, 16, 16, 16]);
        }
        Shape::Full
    }

    pub fn is_full(self) -> bool {
        self == Shape::Full
    }

    /// The boxes making up this shape, in world orientation, non-overlapping.
    pub fn boxes(self) -> Vec<Box16> {
        match self {
            Shape::Full => vec![FULL_BOX],
            Shape::Cuboid(b) => vec![b],
            Shape::Stairs { top, quarters } => {
                let (slab, step) = if top { ((8, 16), (0, 8)) } else { ((0, 8), (8, 16)) };
                let mut out = vec![[0, slab.0, 0, 16, slab.1, 16]];
                for (west, east, z0, z1) in [(NW, NE, 0, 8), (SW, SE, 8, 16)] {
                    let (w, e) = (quarters & west != 0, quarters & east != 0);
                    let x = match (w, e) {
                        (true, true) => Some((0, 16)),
                        (true, false) => Some((0, 8)),
                        (false, true) => Some((8, 16)),
                        (false, false) => None,
                    };
                    if let Some((x0, x1)) = x {
                        out.push([x0, step.0, z0, x1, step.1, z1]);
                    }
                }
                out
            }
            Shape::Pane { along_x, along_z } => match (along_x, along_z) {
                (true, true) => vec![[0, 0, 7, 16, 16, 9], [7, 0, 0, 9, 16, 7], [7, 0, 9, 9, 16, 16]],
                (true, false) => vec![[0, 0, 7, 16, 16, 9]],
                (false, true) => vec![[7, 0, 0, 9, 16, 16]],
                (false, false) => vec![[7, 0, 7, 9, 16, 9]],
            },
            Shape::Fence { sides } => {
                let mut out = vec![[6, 0, 6, 10, 16, 10]];
                for (y0, y1) in [(6, 9), (12, 15)] {
                    arms(&mut out, sides, 4, 2, y0, y1);
                }
                out
            }
            Shape::Wall { low, tall, post } => {
                let (pw, h) = if post { (8, 16) } else { (6, if tall != 0 { 16 } else { 14 }) };
                let a = (16 - pw) / 2;
                let mut out = vec![[a, 0, a, a + pw, h, a + pw]];
                arms(&mut out, low, pw, 6, 0, 14);
                arms(&mut out, tall, pw, 6, 0, 16);
                out
            }
        }
    }
}

fn bare_id(block: &str) -> &str {
    let id = block.split('[').next().unwrap_or(block);
    id.strip_prefix("minecraft:").unwrap_or(id)
}

/// Fences, walls, panes and iron bars: blocks that link up with their neighbours.
pub fn connects(block: &str) -> bool {
    let id = bare_id(block);
    id.ends_with("_fence") || id.ends_with("_wall") || id.ends_with("_pane") || id == "iron_bars"
}

/// A connecting block placed without any state (the generator often writes a
/// bare `glass_pane`): its shape has to be inferred from the neighbours.
pub fn needs_connections(block: &str) -> bool {
    !block.contains('[') && connects(block)
}

/// Shape of a stateless connecting block linked toward `sides` (`N | E | S | W`).
pub fn with_connections(block: &str, sides: u8) -> Shape {
    let id = bare_id(block);
    let (on, off) = if id.ends_with("_wall") { ("low", "none") } else { ("true", "false") };
    let state: Vec<String> =
        SIDES.iter().map(|(k, bit)| format!("{k}={}", if sides & bit != 0 { on } else { off })).collect();
    Shape::from_block(&format!("{id}[{}]", state.join(",")))
}

/// Push a centred `aw`-wide arm from a `pw`-wide centre post out to each side
/// in `sides`, spanning `y0..y1`.
fn arms(out: &mut Vec<Box16>, sides: u8, pw: u8, aw: u8, y0: u8, y1: u8) {
    let (a, b) = ((16 - pw) / 2, (16 - aw) / 2);
    for (bit, bx) in [
        (N, [b, y0, 0, b + aw, y1, a]),
        (S, [b, y0, a + pw, b + aw, y1, 16]),
        (W, [0, y0, b, a, y1, b + aw]),
        (E, [a + pw, y0, b, 16, y1, b + aw]),
    ] {
        if sides & bit != 0 {
            out.push(bx);
        }
    }
}

/// `true` when any box covers the point `(lat, y)` (sixteenths) seen along the
/// view axis; `lat` runs along X when `lat_is_x`, else along Z.
pub fn covers(boxes: &[Box16], lat_is_x: bool, lat: f32, y: f32) -> bool {
    boxes.iter().any(|b| {
        let (l0, l1) = if lat_is_x { (b[0], b[3]) } else { (b[2], b[5]) };
        lat >= l0 as f32 && lat < l1 as f32 && y >= b[1] as f32 && y < b[4] as f32
    })
}

/// Height (sixteenths) of the highest box surface over the point `(x, z)`, if any.
pub fn top_at(boxes: &[Box16], x: f32, z: f32) -> Option<u8> {
    boxes
        .iter()
        .filter(|b| x >= b[0] as f32 && x < b[3] as f32 && z >= b[2] as f32 && z < b[5] as f32)
        .map(|b| b[4])
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slabs_stairs_and_thin_blocks() {
        assert_eq!(Shape::from_block("minecraft:oak_slab[type=bottom,waterlogged=false]"), Shape::Cuboid([0, 0, 0, 16, 8, 16]));
        assert_eq!(Shape::from_block("minecraft:oak_slab[type=top]"), Shape::Cuboid([0, 8, 0, 16, 16, 16]));
        assert_eq!(Shape::from_block("minecraft:oak_slab[type=double]"), Shape::Full);
        assert_eq!(Shape::from_block("minecraft:stone"), Shape::Full);

        // facing=north: high side at the north.
        let s = Shape::from_block("minecraft:oak_stairs[facing=north,half=bottom,shape=straight]");
        assert_eq!(s.boxes(), vec![[0, 0, 0, 16, 8, 16], [0, 8, 0, 16, 16, 8]]);
        // facing=east outer_right: only the south-east quarter is raised.
        let s = Shape::from_block("minecraft:oak_stairs[facing=east,half=top,shape=outer_right]");
        assert_eq!(s.boxes(), vec![[0, 8, 0, 16, 16, 16], [8, 0, 8, 16, 8, 16]]);
        // facing=east inner_left: east half plus the north-west quarter.
        let s = Shape::from_block("minecraft:oak_stairs[facing=east,half=bottom,shape=inner_left]");
        assert_eq!(s, Shape::Stairs { top: false, quarters: NW | NE | SE });

        assert_eq!(Shape::from_block("minecraft:oak_door[facing=east,open=false]"), Shape::Cuboid([0, 0, 0, 3, 16, 16]));
        assert_eq!(Shape::from_block("minecraft:snow[layers=8]"), Shape::Full);
        assert_eq!(Shape::from_block("minecraft:snow[layers=2]"), Shape::Cuboid([0, 0, 0, 16, 4, 16]));
        assert_eq!(
            Shape::from_block("minecraft:glass_pane[east=true,north=false,south=false,west=true]"),
            Shape::Pane { along_x: true, along_z: false }
        );
        assert!(!Shape::from_block("minecraft:oak_fence").is_full());
        assert!(Shape::from_block("minecraft:sea_lantern").is_full());
    }
}

#[cfg(test)]
mod stair_inference_tests {
    use super::*;

    fn lookup<'a>(map: &'a [(Dir, &'a str)]) -> impl Fn(Dir) -> Option<(Dir, bool)> + 'a {
        move |d| map.iter().find(|(k, _)| *k == d).and_then(|(_, b)| stair_info(b))
    }

    #[test]
    fn straight_when_alone_or_aligned() {
        let me = "minecraft:oak_stairs[facing=north,half=bottom]";
        assert_eq!(infer_stairs(me, lookup(&[])), Some(Shape::Stairs { top: false, quarters: NW | NE }));
        // Same orientation on both sides keeps it straight.
        let row = [(Dir::East, me), (Dir::West, me)];
        assert_eq!(infer_stairs(me, lookup(&row)), Some(Shape::Stairs { top: false, quarters: NW | NE }));
    }

    #[test]
    fn outer_corner_from_the_stair_in_front() {
        // Facing north; the stair in front (north) faces east and nothing matches us to the west:
        // Minecraft calls this outer_right, the north-east quarter only.
        let me = "minecraft:oak_stairs[facing=north,half=bottom]";
        let front = "minecraft:oak_stairs[facing=east,half=bottom]";
        assert_eq!(infer_stairs(me, lookup(&[(Dir::North, front)])), Some(Shape::Stairs { top: false, quarters: NE }));
        // Front faces west instead: outer_left, north-west quarter.
        let front_w = "minecraft:oak_stairs[facing=west,half=bottom]";
        assert_eq!(infer_stairs(me, lookup(&[(Dir::North, front_w)])), Some(Shape::Stairs { top: false, quarters: NW }));
        // A top-half stair in front does not pair with a bottom one.
        let front_top = "minecraft:oak_stairs[facing=east,half=top]";
        assert_eq!(infer_stairs(me, lookup(&[(Dir::North, front_top)])), Some(Shape::Stairs { top: false, quarters: NW | NE }));
    }

    #[test]
    fn inner_corner_from_the_stair_behind() {
        // Facing north; the stair behind (south) faces east and the cell east of us is not our twin:
        // inner_right = north half plus the south-east quarter.
        let me = "minecraft:oak_stairs[facing=north,half=bottom]";
        let back = "minecraft:oak_stairs[facing=east,half=bottom]";
        assert_eq!(infer_stairs(me, lookup(&[(Dir::South, back)])), Some(Shape::Stairs { top: false, quarters: NW | NE | SE }));
        // If our east neighbour is an identical stair, the corner is not formed.
        assert_eq!(
            infer_stairs(me, lookup(&[(Dir::South, back), (Dir::East, me)])),
            Some(Shape::Stairs { top: false, quarters: NW | NE })
        );
        // Behind faces west: inner_left = north half plus south-west.
        let back_w = "minecraft:oak_stairs[facing=west,half=bottom]";
        assert_eq!(infer_stairs(me, lookup(&[(Dir::South, back_w)])), Some(Shape::Stairs { top: false, quarters: NW | NE | SW }));
    }
}
