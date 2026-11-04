//! Block models for entity-rendered blocks. The game draws chests, beds,
//! signs, decorated pots and banners with block-entity renderers, so the jar's block
//! models for them carry only a particle texture. This module rebuilds them
//! as ordinary block models (the jar's own JSON format) from the entity
//! texture sheets, so every renderer reads them like any other block.
//!
//! Entity sheets are unwrapped the way the game's `ModelPart` boxes are: a
//! box `w x h x d` texels at texture offset `(u, v)` (`texOffs(u, v)
//! .addBox(.., w, h, d)` in the game's model code) owns six rectangles, see
//! [`slots`]. A model here is a list of boxes in block space, each mapping
//! its world faces onto those rectangles ([`Box3`]), so a new entity block
//! is its boxes plus a line in [`entity_models`] saying how its block state
//! picks a model. Entity models are authored upside down in the game, which
//! is why side faces often read their rectangle bottom-up (`flip_v`).
//!
//! Parts the game draws outside the block (a sign board's top, a pot's neck)
//! stay outside it: the grid records which cells a model reaches into
//! (`VoxelGrid::overhangs_into`) and every view draws them there: the
//! perspective views and the flat ones (iso, top-down, elevations) alike
//! draw each part of a model with the cell it lies in.

use serde_json::{json, Map, Value};

/// The six rectangles of a `ModelPart` box, named as the game names them.
#[derive(Clone, Copy)]
enum Slot {
    Down,
    Up,
    West,
    North,
    East,
    South,
}

/// A rectangle on an entity sheet in texels: `[x0, y0, x1, y1]`.
type Rect = [f32; 4];

/// The six rectangles of a box `w x h x d` at texture offset `(u, v)`.
fn slots((u, v): (f32, f32), (w, h, d): (f32, f32, f32)) -> [Rect; 6] {
    [
        [u + d, v, u + d + w, v + d],                 // Down
        [u + d + w, v, u + d + 2.0 * w, v + d],       // Up
        [u, v + d, u + d, v + d + h],                 // West
        [u + d, v + d, u + d + w, v + d + h],         // North
        [u + d + w, v + d, u + 2.0 * d + w, v + d + h], // East
        [u + 2.0 * d + w, v + d, u + 2.0 * d + 2.0 * w, v + d + h], // South
    ]
}

/// How a world face reads its rectangle.
#[derive(Clone, Copy, Default)]
struct Xf {
    flip_u: bool,
    flip_v: bool,
    /// Face texture rotation (0, 90, 180, 270), as in block-model JSON.
    rot: i32,
}

const PLAIN: Xf = Xf { flip_u: false, flip_v: false, rot: 0 };
const FLIP_V: Xf = Xf { flip_u: false, flip_v: true, rot: 0 };

/// What a world face shows: a `ModelPart` slot, or a raw rectangle.
#[derive(Clone, Copy)]
enum Src {
    Slot(Slot, Xf),
    Rect(Rect, Xf),
}

/// One box of an entity block model.
struct Box3 {
    /// Block-space corners in sixteenths.
    from: [f32; 3],
    to: [f32; 3],
    /// Texture variable (`"t"` unless a model uses several sheets).
    tex: &'static str,
    /// The box's `texOffs` and texel size, for `Src::Slot` faces.
    uv_box: ((f32, f32), (f32, f32, f32)),
    /// World faces: `down`, `up`, `north`, `south`, `west`, `east`.
    faces: Vec<(&'static str, Src)>,
    /// Free rotation about the block's vertical centre line, in degrees.
    y_angle: f32,
}

impl Box3 {
    fn new(from: [f32; 3], to: [f32; 3], uv_box: ((f32, f32), (f32, f32, f32))) -> Self {
        Box3 { from, to, tex: "t", uv_box, faces: Vec::new(), y_angle: 0.0 }
    }
    fn face(mut self, world: &'static str, slot: Slot, xf: Xf) -> Self {
        self.faces.push((world, Src::Slot(slot, xf)));
        self
    }
    fn rect(mut self, world: &'static str, r: Rect, xf: Xf) -> Self {
        self.faces.push((world, Src::Rect(r, xf)));
        self
    }
    fn tex(mut self, tex: &'static str) -> Self {
        self.tex = tex;
        self
    }
    fn turned(mut self, deg: f32) -> Self {
        self.y_angle = deg;
        self
    }

    /// The box as a block-model element; uvs in sixteenths of a sheet that
    /// is `sheet` texels wide.
    fn element(&self, sheet: f32) -> Value {
        let s = 16.0 / sheet;
        let rects = slots(self.uv_box.0, self.uv_box.1);
        let mut faces = Map::new();
        for &(world, src) in &self.faces {
            let (r, xf) = match src {
                Src::Slot(slot, xf) => (rects[slot as usize], xf),
                Src::Rect(r, xf) => (r, xf),
            };
            let (mut u0, mut v0, mut u1, mut v1) = (r[0] * s, r[1] * s, r[2] * s, r[3] * s);
            if xf.flip_u {
                std::mem::swap(&mut u0, &mut u1);
            }
            if xf.flip_v {
                std::mem::swap(&mut v0, &mut v1);
            }
            let mut f = json!({ "texture": format!("#{}", self.tex), "uv": [u0, v0, u1, v1] });
            if xf.rot != 0 {
                f["rotation"] = json!(xf.rot);
            }
            faces.insert(world.to_string(), f);
        }
        let mut e = json!({ "from": self.from, "to": self.to, "faces": faces });
        if self.y_angle != 0.0 {
            e["rotation"] = json!({ "origin": [8, 8, 8], "axis": "y", "angle": self.y_angle });
        }
        e
    }
}

/// A block model from boxes on sheets `textures` (`(variable, texture key)`),
/// all `sheet` texels wide.
fn model(textures: &[(&str, &str)], sheet: f32, boxes: &[Box3]) -> Value {
    let tex: Map<String, Value> = textures.iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
    json!({ "textures": tex, "elements": boxes.iter().map(|b| b.element(sheet)).collect::<Vec<_>>() })
}

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// A single chest, latch to the north, from a 64-texel chest sheet: body
/// 14x10x14 at (0, 19), lid 14x5x14 at (0, 0), latch 2x4x1 at (0, 0). The
/// game draws the chest turned half round, so the latch side reads `South`
/// and the sides read bottom-up.
fn chest(tex: &str) -> Value {
    let sides = |b: Box3| {
        b.face("north", Slot::South, FLIP_V)
            .face("south", Slot::North, FLIP_V)
            .face("west", Slot::West, FLIP_V)
            .face("east", Slot::East, FLIP_V)
    };
    let body = sides(Box3::new([1.0, 0.0, 1.0], [15.0, 10.0, 15.0], ((0.0, 19.0), (14.0, 10.0, 14.0)))).face("down", Slot::Down, PLAIN);
    let lid = sides(Box3::new([1.0, 9.0, 1.0], [15.0, 14.0, 15.0], ((0.0, 0.0), (14.0, 5.0, 14.0)))).face("up", Slot::Up, PLAIN);
    let latch = Box3::new([7.0, 7.0, 0.0], [9.0, 11.0, 1.0], ((0.0, 0.0), (2.0, 4.0, 1.0)))
        .face("north", Slot::North, FLIP_V)
        .face("up", Slot::Down, PLAIN)
        .face("down", Slot::Up, PLAIN)
        .face("west", Slot::West, FLIP_V)
        .face("east", Slot::East, FLIP_V);
    model(&[("t", tex)], 64.0, &[body, lid, latch])
}

/// One half of a bed, head toward north, from a 64-texel bed sheet. Each half
/// is a 16x16x6 mattress (head at (0, 0), foot at (0, 22)) laid flat on 3x3x3
/// legs at the outer corners: the mattress's `North` rectangle is its top,
/// `South` its underside, `Down`/`Up` the head and foot ends, and the long
/// sides read `West`/`East` turned a quarter. Every side and end strip is
/// wool or sheet over a wooden frame (its two outer texels), as in game.
fn bed_half(tex: &str, head: bool) -> Value {
    let (v, end, end_face, leg_z) = if head {
        (0.0, Slot::Down, "north", 0.0)
    } else {
        (22.0, Slot::Up, "south", 13.0)
    };
    let mattress = Box3::new([0.0, 3.0, 0.0], [16.0, 9.0, 16.0], ((0.0, v), (16.0, 16.0, 6.0)))
        .face("up", Slot::North, PLAIN)
        .face("down", Slot::South, PLAIN)
        // End strips too keep the frame on their outer rows: read bottom-up.
        .face(end_face, end, FLIP_V)
        // The side strips run foot-to-head down the sheet and carry the
        // wooden frame on their outer edge (2 of their 6 texels): turned onto
        // the long sides, flip them lengthwise and across, so the white
        // sheet sits under the pillow and the frame along the bottom.
        .face("west", Slot::West, Xf { rot: 90, flip_u: true, flip_v: true })
        .face("east", Slot::East, Xf { rot: 270, flip_u: true, flip_v: true });
    let leg = |x: f32| {
        let r = [53.0, 3.0, 56.0, 6.0];
        let mut b = Box3::new([x, 0.0, leg_z], [x + 3.0, 3.0, leg_z + 3.0], ((50.0, 0.0), (3.0, 3.0, 3.0)));
        for f in ["north", "south", "west", "east"] {
            b = b.rect(f, r, PLAIN);
        }
        b
    };
    model(&[("t", tex)], 64.0, &[mattress, leg(0.0), leg(13.0)])
}

/// The sign board (24x12x2 at (0, 0)) and post (2x14x2 at (0, 14)) of a
/// 64-texel sign sheet, at the game's 2/3 scale: a 16x8 board. Standing:
/// the board on top of its 9.33-high post, reaching 1.33 above the block;
/// on a wall: the board alone, against the block's north edge.
fn sign(tex: &str, wall: bool, y_angle: f32) -> Value {
    let board_faces = |b: Box3| {
        b.face("north", Slot::North, PLAIN)
            .face("south", Slot::South, PLAIN)
            .face("west", Slot::West, PLAIN)
            .face("east", Slot::East, PLAIN)
            .face("up", Slot::Up, PLAIN)
            .face("down", Slot::Down, PLAIN)
    };
    let board_box = ((0.0, 0.0), (24.0, 12.0, 2.0));
    let boxes = if wall {
        vec![board_faces(Box3::new([0.0, 4.33, 14.67], [16.0, 12.33, 16.0], board_box))]
    } else {
        let post = Box3::new([7.33, 0.0, 7.33], [8.67, 9.33, 8.67], ((0.0, 14.0), (2.0, 14.0, 2.0)))
            .face("north", Slot::North, PLAIN)
            .face("south", Slot::South, PLAIN)
            .face("west", Slot::West, PLAIN)
            .face("east", Slot::East, PLAIN)
            .turned(y_angle);
        vec![board_faces(Box3::new([0.0, 9.33, 7.33], [16.0, 17.33, 8.67], board_box)).turned(y_angle), post]
    };
    model(&[("t", tex)], 64.0, &boxes)
}

/// A hanging sign from a 64-texel hanging sheet: the 14x10x2 board at
/// (0, 12) hung on two chains (the chain strip at (0, 6)); on a wall, a bar
/// along the top carries it.
fn hanging_sign(tex: &str, wall: bool, y_angle: f32) -> Value {
    let board = Box3::new([1.0, 0.0, 7.0], [15.0, 10.0, 9.0], ((0.0, 12.0), (14.0, 10.0, 2.0)))
        .face("north", Slot::North, PLAIN)
        .face("south", Slot::South, PLAIN)
        .face("west", Slot::West, PLAIN)
        .face("east", Slot::East, PLAIN)
        .face("up", Slot::Up, PLAIN)
        .face("down", Slot::Down, PLAIN)
        .turned(y_angle);
    let chain = |x: f32| {
        let r = [0.0, 6.0, 3.0, 12.0];
        Box3::new([x, 10.0, 7.5], [x + 1.0, if wall { 14.0 } else { 16.0 }, 8.5], ((0.0, 0.0), (0.0, 0.0, 0.0)))
            .rect("north", r, PLAIN)
            .rect("south", r, PLAIN)
            .rect("west", r, PLAIN)
            .rect("east", r, PLAIN)
            .turned(y_angle)
    };
    let mut boxes = vec![board, chain(3.0), chain(12.0)];
    if wall {
        let r = [4.0, 12.0, 20.0, 14.0];
        let mut bar = Box3::new([0.0, 14.0, 6.0], [16.0, 16.0, 10.0], ((0.0, 0.0), (0.0, 0.0, 0.0)));
        for f in ["north", "south", "west", "east", "up", "down"] {
            bar = bar.rect(f, r, PLAIN);
        }
        boxes.push(bar);
    }
    model(&[("t", tex)], 64.0, &boxes)
}

/// A plain decorated pot: the 16-texel side sheet's panel (x 1..15) on the
/// four sides of a 14-wide body, the 32-texel base sheet for the body's top
/// and bottom (14x0x14 at (0, 13)) and the neck above the block: a 6x1x6
/// collar (0, 5) under an 8x3x8 lip (0, 0), reaching 20/16 high.
fn decorated_pot(side: &str, base: &str) -> Value {
    let panel = [1.0, 0.0, 15.0, 16.0];
    // The side sheet is 16 texels wide, the base 32: scale the panel up.
    let panel = [panel[0] * 2.0, panel[1] * 2.0, panel[2] * 2.0, panel[3] * 2.0];
    // The game offsets this flat box by -14 texels, so its rectangles start at x 0.
    let body = Box3::new([1.0, 0.0, 1.0], [15.0, 16.0, 15.0], ((-14.0, 13.0), (14.0, 0.0, 14.0)))
        .face("up", Slot::Down, PLAIN)
        .face("down", Slot::Up, PLAIN);
    let mut panels = Box3::new([1.0, 0.0, 1.0], [15.0, 16.0, 15.0], ((0.0, 0.0), (0.0, 0.0, 0.0))).tex("side");
    for f in ["north", "south", "west", "east"] {
        panels = panels.rect(f, panel, PLAIN);
    }
    let neck_sides = |b: Box3| {
        b.face("north", Slot::North, FLIP_V).face("south", Slot::South, FLIP_V).face("west", Slot::West, FLIP_V).face("east", Slot::East, FLIP_V)
    };
    let collar = neck_sides(Box3::new([5.0, 16.0, 5.0], [11.0, 17.0, 11.0], ((0.0, 5.0), (6.0, 1.0, 6.0)))).tex("base");
    let lip = neck_sides(Box3::new([4.0, 17.0, 4.0], [12.0, 20.0, 12.0], ((0.0, 0.0), (8.0, 3.0, 8.0))))
        .face("up", Slot::Up, PLAIN)
        .tex("base");
    model(&[("t", base), ("base", base), ("side", side)], 32.0, &[body.tex("base"), panels, collar, lip])
}

/// A banner facing south, from the 64-texel `banner_base` sheet: the flag
/// (20x40x1 at (0, 0)) hung from a crossbar (20x2x2 at (0, 42)), standing on
/// a pole (2x42x2 at (44, 0)) or against the north wall. `BannerRenderer`
/// draws these at 2/3 scale turned half round about x (y and z negated) from
/// the block centre, or for a wall banner from (8, -7.67, 1), so the flag
/// hangs 13 below a wall banner's block and a standing banner is 29.33 tall.
/// The half turn puts each rectangle on the opposite world face only for the
/// `North`/`South` and `Down`/`Up` pairs, all read as laid out but the
/// bottom. The flag takes texture `flag` (see [`super::banner`]), the
/// wood `pole`.
fn banner(wall: bool, y_angle: f32) -> Value {
    const S: f32 = 2.0 / 3.0;
    let o = if wall { [8.0, -7.0 - 2.0 / 3.0, 1.0] } else { [8.0, 8.0, 8.0] };
    // A ModelPart box at (x, y, z) sized (w, h, d), in block sixteenths.
    let part = |at: [f32; 3], size: [f32; 3], uv: (f32, f32)| {
        let from = [o[0] + S * at[0], o[1] - S * (at[1] + size[1]), o[2] - S * (at[2] + size[2])];
        let to = [o[0] + S * (at[0] + size[0]), o[1] - S * at[1], o[2] - S * at[2]];
        Box3::new(from, to, (uv, (size[0], size[1], size[2])))
            .face("south", Slot::North, PLAIN)
            .face("north", Slot::South, PLAIN)
            .face("west", Slot::West, PLAIN)
            .face("east", Slot::East, PLAIN)
            .face("up", Slot::Down, PLAIN)
            .face("down", Slot::Up, FLIP_V)
            .turned(y_angle)
    };
    let mut boxes = vec![
        part([-10.0, -32.0, -2.0], [20.0, 40.0, 1.0], (0.0, 0.0)).tex("flag"),
        part([-10.0, -32.0, -1.0], [20.0, 2.0, 2.0], (0.0, 42.0)).tex("pole"),
    ];
    if !wall {
        boxes.push(part([-1.0, -30.0, -1.0], [2.0, 42.0, 2.0], (44.0, 0.0)).tex("pole"));
    }
    model(&[("pole", "entity/banner_base"), ("flag", "tome_banner:white")], 64.0, &boxes)
}

// ---------------------------------------------------------------------------
// Block states
// ---------------------------------------------------------------------------

/// The four horizontal facings with their quarter turns from north.
const FACINGS: [(&str, i32); 4] = [("north", 0), ("east", 90), ("south", 180), ("west", 270)];

/// A standing sign's `rotation` (0..15, sixteenths of a turn clockwise from
/// south) as a quarter turn of the south-facing model plus a remainder of
/// -1..=2 sixteenths: `(quarter turn degrees, remainder)`. The remainder
/// becomes a free element turn; free turns are counter-clockwise, so the
/// callers negate it.
fn sixteenth(r: i32) -> (i32, i32) {
    let q = (r + 1) / 4;
    let rem = r - 4 * q;
    ((q % 4) * 90, rem)
}

/// Models to register (`block/<name>` keys) and the block states that pick
/// them, for every entity block whose texture is in `has`. Families:
/// chests (`facing`), beds (`facing`, `part`), standing, wall, hanging and
/// wall hanging signs per wood, the decorated pot, and standing and wall
/// banners per colour.
pub(super) fn entity_models<'a>(
    textures: impl Iterator<Item = &'a str>,
) -> (Vec<(String, Value)>, Vec<(String, Value)>) {
    let textures: Vec<&str> = textures.collect();
    let has = |k: &str| textures.contains(&k);
    let mut models = Vec::new();
    let mut states = Vec::new();
    let facing_states = |key: &str| -> Value {
        let v: Map<String, Value> =
            FACINGS.iter().map(|(f, y)| (format!("facing={f}"), json!({ "model": key, "y": y }))).collect();
        json!({ "variants": v })
    };

    for (id, sheet) in [("chest", "normal"), ("trapped_chest", "trapped"), ("ender_chest", "ender")] {
        let tex = format!("entity/chest/{sheet}");
        if has(&tex) {
            let key = format!("block/tome_entity_{id}");
            models.push((key.clone(), chest(&tex)));
            states.push((id.to_string(), facing_states(&key)));
        }
    }

    for tex in textures.iter().filter(|k| k.starts_with("entity/bed/")) {
        let colour = &tex["entity/bed/".len()..];
        let id = format!("{colour}_bed");
        let mut v = Map::new();
        for (part, head) in [("head", true), ("foot", false)] {
            let key = format!("block/tome_entity_{id}_{part}");
            models.push((key.clone(), bed_half(tex, head)));
            for (f, y) in FACINGS {
                v.insert(format!("facing={f},part={part}"), json!({ "model": key, "y": y }));
            }
        }
        states.push((id, json!({ "variants": v })));
    }

    for tex in textures.iter().filter(|k| k.starts_with("entity/signs/") && !k.starts_with("entity/signs/hanging/")) {
        let wood = &tex["entity/signs/".len()..];
        let wall_key = format!("block/tome_entity_{wood}_wall_sign");
        models.push((wall_key.clone(), sign(tex, true, 0.0)));
        states.push((format!("{wood}_wall_sign"), facing_states(&wall_key)));
        let mut v = Map::new();
        for r in 0..16 {
            let (y, rem) = sixteenth(r);
            let key = format!("block/tome_entity_{wood}_sign_{}", rem + 1);
            if !models.iter().any(|(k, _)| *k == key) {
                models.push((key.clone(), sign(tex, false, -(rem as f32) * 22.5)));
            }
            v.insert(format!("rotation={r}"), json!({ "model": key, "y": y }));
        }
        states.push((format!("{wood}_sign"), json!({ "variants": v })));
    }

    for tex in textures.iter().filter(|k| k.starts_with("entity/signs/hanging/")) {
        let wood = &tex["entity/signs/hanging/".len()..];
        let wall_key = format!("block/tome_entity_{wood}_wall_hanging_sign");
        models.push((wall_key.clone(), hanging_sign(tex, true, 0.0)));
        states.push((format!("{wood}_wall_hanging_sign"), facing_states(&wall_key)));
        let mut v = Map::new();
        for r in 0..16 {
            let (y, rem) = sixteenth(r);
            let key = format!("block/tome_entity_{wood}_hanging_sign_{}", rem + 1);
            if !models.iter().any(|(k, _)| *k == key) {
                models.push((key.clone(), hanging_sign(tex, false, -(rem as f32) * 22.5)));
            }
            for attached in ["true", "false"] {
                v.insert(format!("attached={attached},rotation={r}"), json!({ "model": key, "y": y }));
            }
        }
        states.push((format!("{wood}_hanging_sign"), json!({ "variants": v })));
    }

    // Banners: one set of models for every colour; the flag texture is
    // bound per block string (colour and patterns), see `super::banner`.
    if has("entity/banner_base") && has("entity/banner/base") {
        let wall_key = "block/tome_entity_wall_banner".to_string();
        models.push((wall_key.clone(), banner(true, 0.0)));
        // The models face south: facing south is no turn.
        let wall: Map<String, Value> = FACINGS
            .iter()
            .map(|(f, y)| (format!("facing={f}"), json!({ "model": wall_key, "y": (y + 180) % 360 })))
            .collect();
        let mut standing = Map::new();
        for r in 0..16 {
            let (y, rem) = sixteenth(r);
            let key = format!("block/tome_entity_banner_{}", rem + 1);
            if !models.iter().any(|(k, _)| *k == key) {
                models.push((key.clone(), banner(false, -(rem as f32) * 22.5)));
            }
            standing.insert(format!("rotation={r}"), json!({ "model": key, "y": y }));
        }
        for colour in super::banner::colours() {
            states.push((format!("{colour}_wall_banner"), json!({ "variants": wall })));
            states.push((format!("{colour}_banner"), json!({ "variants": standing })));
        }
    }

    let (side, base) = ("entity/decorated_pot/decorated_pot_side", "entity/decorated_pot/decorated_pot_base");
    if has(side) && has(base) {
        let key = "block/tome_entity_decorated_pot".to_string();
        models.push((key.clone(), decorated_pot(side, base)));
        states.push(("decorated_pot".to_string(), json!({ "variants": { "": { "model": key } } })));
    }
    (models, states)
}
