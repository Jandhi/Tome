//! Minecraft's own block models and textures, read from a client jar, so the
//! renderers can draw real block faces instead of flat colours.
//!
//! The jar is a zip: `assets/minecraft/blockstates/<id>.json` maps a block's
//! state to one or more models (with 90-degree rotations), each
//! `assets/minecraft/models/block/<name>.json` is a chain of parents ending
//! in `elements` (boxes in sixteenths with per-face textures and UVs), and
//! `assets/minecraft/textures/block/<name>.png` holds the 16x16 textures
//! (animated ones are vertical strips; the first frame is used). A block
//! resolves to a [`BlockGeometry`]: world-oriented elements with a texture
//! frame per face, plus average top/side colours for the flat views. Blocks
//! whose models have no elements (fluids) resolve to `None` and keep the
//! hand-written shape and colour; chests, beds, signs, pots and banners get
//! models built from their entity sheets (see [`super::entity`]).
//!
//! The jar is found through `TOME_MC_JAR`, else the newest release under the
//! CurseForge or `.minecraft` install; `TOME_NO_TEXTURES=1` disables all of
//! this. Nothing from the jar is written anywhere.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use resvg::tiny_skia::Pixmap;
use serde_json::Value;

use crate::util::HashMap;

pub type Rgba = [u8; 4];

/// A decoded block texture: one square frame.
pub struct Texture {
    pub size: u32,
    pub rgba: Vec<Rgba>,
    /// Mean colour of the texels with any alpha, with the mean alpha.
    pub avg: Rgba,
}

impl Texture {
    fn from_png(bytes: &[u8]) -> Option<Texture> {
        let pm = Pixmap::decode_png(bytes).ok()?;
        let size = pm.width().max(1);
        let frame_h = pm.height().min(size);
        let mut rgba = Vec::with_capacity((size * size) as usize);
        for y in 0..size {
            for x in 0..size {
                let c = if y < frame_h { pm.pixel(x, y).map(|p| p.demultiply()) } else { None };
                rgba.push(c.map_or([0, 0, 0, 0], |c| [c.red(), c.green(), c.blue(), c.alpha()]));
            }
        }
        Some(Texture::from_rgba(size, rgba))
    }

    /// A texture from `size x size` straight-alpha texels, row by row.
    pub fn from_rgba(size: u32, rgba: Vec<Rgba>) -> Texture {
        let (mut sum, mut n, mut alpha_sum) = ([0u64; 3], 0u64, 0u64);
        for c in &rgba {
            alpha_sum += c[3] as u64;
            if c[3] > 0 {
                for k in 0..3 {
                    sum[k] += c[k] as u64;
                }
                n += 1;
            }
        }
        let texels = (size * size) as u64;
        let avg = if n == 0 {
            [0, 0, 0, 0]
        } else {
            [(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8, (alpha_sum / texels) as u8]
        };
        Texture { size, rgba, avg }
    }

    /// Nearest texel at texture pixel coordinates (wrapping).
    pub fn sample(&self, u: f32, v: f32) -> Rgba {
        let s = self.size as i32;
        let x = (u.floor() as i32).rem_euclid(s);
        let y = (v.floor() as i32).rem_euclid(s);
        self.rgba[(y * s + x) as usize]
    }
}

/// A face direction (block-space normal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face6 {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Face6 {
    pub const ALL: [Face6; 6] = [Face6::Down, Face6::Up, Face6::North, Face6::South, Face6::West, Face6::East];

    fn parse(s: &str) -> Option<Face6> {
        Some(match s {
            "down" => Face6::Down,
            "up" => Face6::Up,
            "north" => Face6::North,
            "south" => Face6::South,
            "west" => Face6::West,
            "east" => Face6::East,
            _ => return None,
        })
    }

    /// Face entered by a ray along `axis` (0 = x, 1 = y, 2 = z) through the
    /// box's max plane (`through_max`) or min plane.
    pub fn entered(axis: usize, through_max: bool) -> Face6 {
        match (axis, through_max) {
            (0, true) => Face6::East,
            (0, false) => Face6::West,
            (1, true) => Face6::Up,
            (1, false) => Face6::Down,
            (_, true) => Face6::South,
            (_, false) => Face6::North,
        }
    }

    fn normal(self) -> V3 {
        match self {
            Face6::Down => [0.0, -1.0, 0.0],
            Face6::Up => [0.0, 1.0, 0.0],
            Face6::North => [0.0, 0.0, -1.0],
            Face6::South => [0.0, 0.0, 1.0],
            Face6::West => [-1.0, 0.0, 0.0],
            Face6::East => [1.0, 0.0, 0.0],
        }
    }

    fn from_normal(n: V3) -> Face6 {
        let mut best = Face6::Up;
        let mut best_dot = f32::NEG_INFINITY;
        for f in Face6::ALL {
            let d = dot(f.normal(), n);
            if d > best_dot {
                best_dot = d;
                best = f;
            }
        }
        best
    }

    pub fn is_vertical(self) -> bool {
        matches!(self, Face6::Up | Face6::Down)
    }
}

/// How a face maps its texture: the point with texture coords `(u0, v0)` and
/// the vectors to the `(u1, v0)` and `(u0, v1)` corners, in the element's
/// frame (sixteenths).
pub struct FaceTex {
    pub texture: Arc<Texture>,
    pub origin: V3,
    pub du: V3,
    pub dv: V3,
    /// `[u0, v0, u1, v1]` in texture pixels.
    pub uv: [f32; 4],
    /// Biome tint multiplier (grass, foliage), if the face is tinted.
    pub tint: Option<Rgba>,
    /// Whether the face takes directional shading (plants and fire don't).
    pub shade: bool,
}

impl FaceTex {
    /// Texel colour at a point `p` on the face (element frame, sixteenths).
    pub fn sample_at(&self, p: V3) -> Rgba {
        let rel = sub(p, self.origin);
        let a = dot(rel, self.du) / dot(self.du, self.du).max(1e-6);
        let b = dot(rel, self.dv) / dot(self.dv, self.dv).max(1e-6);
        let scale = self.texture.size as f32 / 16.0;
        let u = (self.uv[0] + a * (self.uv[2] - self.uv[0])) * scale;
        let v = (self.uv[1] + b * (self.uv[3] - self.uv[1])) * scale;
        let mut c = self.texture.sample(u, v);
        if let Some(t) = self.tint {
            for k in 0..3 {
                c[k] = ((c[k] as u32 * t[k] as u32) / 255) as u8;
            }
        }
        c
    }
}

/// Rotation + translation taking block-space sixteenths into an element's
/// own frame (for elements with a free rotation): `p' = m * p + t`.
pub struct LocalFrame {
    pub m: [[f32; 3]; 3],
    pub t: V3,
}

impl LocalFrame {
    pub fn point(&self, p: V3) -> V3 {
        add(mul_mat(&self.m, p), self.t)
    }
    pub fn dir(&self, d: V3) -> V3 {
        mul_mat(&self.m, d)
    }
}

/// One box of a block model, world-oriented unless `local` is set.
pub struct Element {
    /// Box corners in sixteenths (element frame).
    pub from: V3,
    pub to: V3,
    pub faces: Vec<(Face6, FaceTex)>,
    pub local: Option<LocalFrame>,
}

/// A block's resolved model.
pub struct BlockGeometry {
    pub elements: Vec<Element>,
    /// Average colour of the upward faces (sides when there are none); alpha
    /// is the textures' mean alpha.
    pub top: Rgba,
    /// Average colour of the side faces (top when there are none).
    pub side: Rgba,
}

impl BlockGeometry {
    /// World-space bounds of every element in sixteenths, rotated elements
    /// included: `(min, max)`. Beyond `0..16` the model reaches into a
    /// neighbouring cell (a sign board's top, a pot's neck).
    pub fn bounds(&self) -> (V3, V3) {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for e in &self.elements {
            for i in 0..8 {
                let c = [
                    if i & 1 == 0 { e.from[0] } else { e.to[0] },
                    if i & 2 == 0 { e.from[1] } else { e.to[1] },
                    if i & 4 == 0 { e.from[2] } else { e.to[2] },
                ];
                // A rotated element's corners are in its own frame: p' = m p + t.
                let w = match &e.local {
                    Some(f) => mul_mat(&transpose(&f.m), sub(c, f.t)),
                    None => c,
                };
                for k in 0..3 {
                    lo[k] = lo[k].min(w[k]);
                    hi[k] = hi[k].max(w[k]);
                }
            }
        }
        (lo, hi)
    }
}

pub type V3 = [f32; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn mul_mat(m: &[[f32; 3]; 3], v: V3) -> V3 {
    [dot(m[0], v), dot(m[1], v), dot(m[2], v)]
}
fn mat_mul(a: &[[f32; 3]; 3], b: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}
fn transpose(m: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = m[j][i];
        }
    }
    out
}
const IDENTITY: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
const CENTRE: V3 = [8.0, 8.0, 8.0];

/// Rotation about the block centre by `quarter_turns` x 90 degrees around
/// the Y axis (clockwise seen from above: north goes to east) or the X axis.
fn quarter_matrix(axis_y: bool, quarter_turns: i32) -> [[f32; 3]; 3] {
    let step = if axis_y {
        // (x, y, z) -> (-z, y, x)
        [[0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]]
    } else {
        // (x, y, z) -> (x, z, -y): the game's x = 90 turns a model's top
        // to the north (a north-facing barrel is `x: 90` alone).
        [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]]
    };
    let mut m = IDENTITY;
    for _ in 0..quarter_turns.rem_euclid(4) {
        m = mat_mul(&step, &m);
    }
    m
}

fn rotate_point(m: &[[f32; 3]; 3], p: V3) -> V3 {
    add(mul_mat(m, sub(p, CENTRE)), CENTRE)
}

fn axis_matrix(axis: usize, angle_deg: f32) -> [[f32; 3]; 3] {
    let (s, c) = angle_deg.to_radians().sin_cos();
    match axis {
        0 => [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]],
        1 => [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]],
        _ => [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
    }
}

/// Texture frame of `face` on the axis-aligned box `from..to`, following
/// Minecraft's conventions (u runs to the viewer's right looking at the face
/// from outside, v runs down; tops run x then z, bottoms x then -z).
fn face_frame(face: Face6, from: V3, to: V3) -> (V3, V3, V3) {
    let (f, t) = (from, to);
    let (sx, sy, sz) = (t[0] - f[0], t[1] - f[1], t[2] - f[2]);
    match face {
        Face6::Up => ([f[0], t[1], f[2]], [sx, 0.0, 0.0], [0.0, 0.0, sz]),
        Face6::Down => ([f[0], f[1], t[2]], [sx, 0.0, 0.0], [0.0, 0.0, -sz]),
        Face6::North => ([t[0], t[1], f[2]], [-sx, 0.0, 0.0], [0.0, -sy, 0.0]),
        Face6::South => ([f[0], t[1], t[2]], [sx, 0.0, 0.0], [0.0, -sy, 0.0]),
        Face6::West => ([f[0], t[1], f[2]], [0.0, 0.0, sz], [0.0, -sy, 0.0]),
        Face6::East => ([t[0], t[1], t[2]], [0.0, 0.0, -sz], [0.0, -sy, 0.0]),
    }
}

/// Default UVs: the face's own extent projected onto the texture.
fn default_uv(face: Face6, from: V3, to: V3) -> [f32; 4] {
    let (f, t) = (from, to);
    match face {
        Face6::Up => [f[0], f[2], t[0], t[2]],
        Face6::Down => [f[0], 16.0 - t[2], t[0], 16.0 - f[2]],
        Face6::North => [16.0 - t[0], 16.0 - t[1], 16.0 - f[0], 16.0 - f[1]],
        Face6::South => [f[0], 16.0 - t[1], t[0], 16.0 - f[1]],
        Face6::West => [f[2], 16.0 - t[1], t[2], 16.0 - f[1]],
        Face6::East => [16.0 - t[2], 16.0 - t[1], 16.0 - f[2], 16.0 - f[1]],
    }
}

/// Rotate a face's texture clockwise by `deg` (0/90/180/270).
fn rotate_frame(deg: i64, (o, du, dv): (V3, V3, V3)) -> (V3, V3, V3) {
    let neg = |v: V3| [-v[0], -v[1], -v[2]];
    match deg.rem_euclid(360) {
        90 => (add(o, du), dv, neg(du)),
        180 => (add(add(o, du), dv), neg(du), neg(dv)),
        270 => (add(o, dv), neg(dv), du),
        _ => (o, du, dv),
    }
}

/// Biome tint for a tinted face of `id` (plains colours).
fn tint_for(id: &str) -> Rgba {
    const GRASS: Rgba = [145, 189, 89, 255];
    const FOLIAGE: Rgba = [119, 171, 47, 255];
    if id.ends_with("_leaves") {
        return match id {
            "spruce_leaves" => [97, 153, 97, 255],
            "birch_leaves" => [128, 167, 85, 255],
            _ => FOLIAGE,
        };
    }
    match id {
        "lily_pad" => [32, 128, 48, 255],
        "vine" | "mangrove_roots" => FOLIAGE,
        _ => GRASS,
    }
}

/// State values assumed when a block string leaves a property out.
const DEFAULTS: [(&str, &str); 24] = [
    ("facing", "north"),
    ("half", "bottom"),
    ("shape", "straight"),
    ("waterlogged", "false"),
    ("open", "false"),
    ("powered", "false"),
    ("lit", "true"),
    ("hanging", "false"),
    ("type", "bottom"),
    ("axis", "y"),
    ("snowy", "false"),
    ("part", "foot"),
    ("hinge", "left"),
    ("in_wall", "false"),
    ("signal_fire", "false"),
    ("layers", "1"),
    ("level", "0"),
    ("attached", "false"),
    ("face", "wall"),
    ("rotation", "0"),
    ("age", "0"),
    ("distance", "7"),
    ("persistent", "false"),
    ("up", "true"),
];

/// A model file: its own textures map and elements, plus its parent's name.
struct RawModel {
    parent: Option<String>,
    textures: BTreeMap<String, String>,
    elements: Option<Vec<Value>>,
}

/// A model picked by a blockstate, with its variant rotation.
pub(super) struct Pick {
    pub(super) model: String,
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) uvlock: bool,
}

/// Everything read from the jar.
pub struct Assets {
    pub jar: PathBuf,
    textures: HashMap<String, Arc<Texture>>,
    models: HashMap<String, RawModel>,
    blockstates: HashMap<String, Value>,
    cache: Mutex<HashMap<String, Option<Arc<BlockGeometry>>>>,
    /// Baked banner flags by [`super::banner::flag_key`].
    flags: Mutex<HashMap<String, Option<Arc<Texture>>>>,
}

static ASSETS: OnceLock<Option<Assets>> = OnceLock::new();

/// The loaded assets, if a jar was found and textures are not disabled.
/// Loaded once per process.
pub fn assets() -> Option<&'static Assets> {
    ASSETS
        .get_or_init(|| {
            if std::env::var_os("TOME_NO_TEXTURES").is_some() {
                log::info!("textures: disabled by TOME_NO_TEXTURES");
                return None;
            }
            let Some(jar) = find_jar() else {
                log::info!("textures: no Minecraft client jar found (set TOME_MC_JAR); using flat colours");
                return None;
            };
            let t = std::time::Instant::now();
            match Assets::load(&jar) {
                Ok(a) => {
                    log::info!(
                        "textures: {} block textures, {} models, {} blockstates from {} in {:.1?}",
                        a.textures.len(),
                        a.models.len(),
                        a.blockstates.len(),
                        jar.display(),
                        t.elapsed()
                    );
                    Some(a)
                }
                Err(e) => {
                    log::warn!("textures: could not read {}: {e:#}; using flat colours", jar.display());
                    None
                }
            }
        })
        .as_ref()
}

/// Locate a Minecraft client jar: `TOME_MC_JAR`, else the newest numbered
/// release under the CurseForge install or `.minecraft` (`TOME_MC_VERSION`
/// pins one).
pub fn find_jar() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("TOME_MC_JAR") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    for home in ["USERPROFILE", "HOME"].iter().filter_map(|k| std::env::var_os(k)) {
        roots.push(PathBuf::from(&home).join("curseforge").join("minecraft").join("Install").join("versions"));
        roots.push(PathBuf::from(&home).join(".minecraft").join("versions"));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        roots.push(PathBuf::from(appdata).join(".minecraft").join("versions"));
    }
    let wanted = std::env::var("TOME_MC_VERSION").ok();
    let version_key = |name: &str| -> Option<Vec<u32>> {
        name.split('.').map(|p| p.parse::<u32>().ok()).collect()
    };
    let mut best: Option<(Vec<u32>, PathBuf)> = None;
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if wanted.as_deref().is_some_and(|w| w != name) {
                continue;
            }
            let Some(key) = version_key(&name) else { continue };
            let jar = entry.path().join(format!("{name}.jar"));
            if jar.is_file() && best.as_ref().is_none_or(|(k, _)| key > *k) {
                best = Some((key, jar));
            }
        }
    }
    best.map(|(_, p)| p)
}

fn strip_ns(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

impl Assets {
    pub fn load(jar: &Path) -> anyhow::Result<Assets> {
        let file = std::fs::File::open(jar)?;
        let mut zip = zip::ZipArchive::new(file)?;
        let mut textures = HashMap::default();
        let mut models = HashMap::default();
        let mut blockstates = HashMap::default();
        let mut buf = Vec::new();
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let name = entry.name().to_string();
            let Some(rest) = name.strip_prefix("assets/minecraft/") else { continue };
            let (kind, key) = if let Some(k) = rest.strip_prefix("textures/block/").and_then(|k| k.strip_suffix(".png")) {
                ("tex", format!("block/{k}"))
            } else if let Some(k) = ENTITY_TEXTURE_DIRS
                .iter()
                .find_map(|d| rest.strip_prefix("textures/entity/").filter(|k| k.starts_with(d)))
                .and_then(|k| k.strip_suffix(".png"))
            {
                ("tex", format!("entity/{k}"))
            } else if let Some(k) = rest.strip_prefix("models/block/").and_then(|k| k.strip_suffix(".json")) {
                ("model", format!("block/{k}"))
            } else if let Some(k) = rest.strip_prefix("blockstates/").and_then(|k| k.strip_suffix(".json")) {
                ("state", k.to_string())
            } else {
                continue;
            };
            buf.clear();
            entry.read_to_end(&mut buf)?;
            match kind {
                "tex" => {
                    if let Some(t) = Texture::from_png(&buf) {
                        textures.insert(key, Arc::new(t));
                    }
                }
                "model" => {
                    let v: Value = serde_json::from_slice(&buf)?;
                    let textures_map = v
                        .get("textures")
                        .and_then(Value::as_object)
                        .map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
                        .unwrap_or_default();
                    models.insert(
                        key,
                        RawModel {
                            parent: v.get("parent").and_then(Value::as_str).map(|s| strip_ns(s).to_string()),
                            textures: textures_map,
                            elements: v.get("elements").and_then(Value::as_array).cloned(),
                        },
                    );
                }
                _ => {
                    blockstates.insert(key, serde_json::from_slice(&buf)?);
                }
            }
        }
        add_entity_models(&textures, &mut models, &mut blockstates);
        Ok(Assets { jar: jar.to_path_buf(), textures, models, blockstates, cache: Mutex::default(), flags: Mutex::default() })
    }

    /// The resolved geometry of a block string (`minecraft:` prefix and state
    /// optional), cached; `None` when the block has no element model.
    pub fn geometry(&self, block: &str) -> Option<Arc<BlockGeometry>> {
        if let Some(hit) = self.cache.lock().unwrap().get(block) {
            return hit.clone();
        }
        let built = self.build_geometry(block).map(Arc::new);
        self.cache.lock().unwrap().insert(block.to_string(), built.clone());
        built
    }

    fn build_geometry(&self, block: &str) -> Option<BlockGeometry> {
        let (id, props) = parse_block(block);
        let state = self.blockstates.get(id)?;
        let props = self.fill_defaults(state, props);
        let picks = pick_models(state, &props);
        // A banner's flag shows its own design, baked on demand.
        let flag = super::banner::flag_key(id, &props).map(|k| ("flag", k));
        let mut elements = Vec::new();
        for pick in picks {
            self.add_model(id, &pick, flag.as_ref(), &mut elements);
        }
        if elements.is_empty() {
            return None;
        }
        let (top, side) = average_colours(&elements);
        Some(BlockGeometry { elements, top, side })
    }

    /// Fill in properties the blockstate file uses that `props` leaves out.
    fn fill_defaults(&self, state: &Value, mut props: BTreeMap<String, String>) -> BTreeMap<String, String> {
        let mut seen: BTreeMap<String, String> = BTreeMap::new();
        if let Some(variants) = state.get("variants").and_then(Value::as_object) {
            for key in variants.keys() {
                for (k, v) in key.split(',').filter_map(|kv| kv.split_once('=')) {
                    seen.entry(k.to_string()).or_insert_with(|| v.to_string());
                }
            }
        }
        if let Some(parts) = state.get("multipart").and_then(Value::as_array) {
            for part in parts {
                if let Some(when) = part.get("when") {
                    collect_when_keys(when, &mut seen);
                }
            }
        }
        for (k, first) in seen {
            if !props.contains_key(&k) {
                let v = DEFAULTS.iter().find(|(dk, _)| *dk == k).map(|(_, dv)| dv.to_string()).unwrap_or(first);
                props.insert(k, v);
            }
        }
        props
    }

    /// Resolve `pick`'s model chain and append its elements, rotated.
    /// `texture` binds one texture variable ahead of the models' own.
    fn add_model(&self, id: &str, pick: &Pick, texture: Option<&(&str, String)>, out: &mut Vec<Element>) {
        let mut textures: BTreeMap<String, String> = BTreeMap::new();
        if let Some((var, key)) = texture {
            textures.insert(var.to_string(), key.clone());
        }
        let mut elements: Option<&Vec<Value>> = None;
        let mut name = Some(pick.model.clone());
        for _ in 0..16 {
            let Some(n) = name.take() else { break };
            let Some(model) = self.models.get(&n) else { break };
            for (k, v) in &model.textures {
                textures.entry(k.clone()).or_insert_with(|| v.clone());
            }
            if elements.is_none() {
                elements = model.elements.as_ref();
            }
            name = model.parent.clone();
        }
        let Some(elements) = elements else { return };
        let lookup = |tex_ref: &str| -> Option<Arc<Texture>> {
            let mut r = tex_ref.to_string();
            for _ in 0..16 {
                if let Some(key) = r.strip_prefix('#') {
                    r = textures.get(key)?.clone();
                } else if r.starts_with(super::banner::FLAG_KEY) {
                    return self.flag(&r);
                } else {
                    return self.textures.get(strip_ns(&r)).cloned();
                }
            }
            None
        };
        let variant = mat_mul(&quarter_matrix(true, pick.y / 90), &quarter_matrix(false, pick.x / 90));
        for el in elements {
            if let Some(e) = build_element(id, el, &variant, pick.uvlock, &lookup) {
                out.push(e);
            }
        }
    }
}

impl Assets {
    /// The baked banner flag texture for `key`, cached.
    fn flag(&self, key: &str) -> Option<Arc<Texture>> {
        if let Some(hit) = self.flags.lock().unwrap().get(key) {
            return hit.clone();
        }
        let built = super::banner::flag_texture(&self.textures, key).map(Arc::new);
        self.flags.lock().unwrap().insert(key.to_string(), built.clone());
        built
    }
}

/// `(id, properties)` of a block string.
pub(super) fn parse_block(block: &str) -> (&str, BTreeMap<String, String>) {
    let (id, inner) = match block.split_once('[') {
        Some((id, rest)) => (id, rest.trim_end_matches(']')),
        None => (block, ""),
    };
    let props = inner
        .split(',')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    (strip_ns(id), props)
}

fn collect_when_keys(when: &Value, seen: &mut BTreeMap<String, String>) {
    let Some(obj) = when.as_object() else { return };
    for (k, v) in obj {
        if k == "OR" || k == "AND" {
            for sub in v.as_array().into_iter().flatten() {
                collect_when_keys(sub, seen);
            }
        } else if let Some(s) = v.as_str() {
            let first = s.split('|').next().unwrap_or(s);
            seen.entry(k.clone()).or_insert_with(|| first.to_string());
        }
    }
}

pub(super) fn when_matches(when: &Value, props: &BTreeMap<String, String>) -> bool {
    let Some(obj) = when.as_object() else { return true };
    obj.iter().all(|(k, v)| match k.as_str() {
        "OR" => v.as_array().is_some_and(|alts| alts.iter().any(|w| when_matches(w, props))),
        "AND" => v.as_array().is_some_and(|alts| alts.iter().all(|w| when_matches(w, props))),
        _ => {
            let Some(want) = v.as_str() else { return false };
            props.get(k).is_some_and(|have| want.split('|').any(|w| w == have))
        }
    })
}

fn pick_from(apply: &Value) -> Option<Pick> {
    // A list means random alternatives; the first is as good as any.
    let v = if let Some(arr) = apply.as_array() { arr.first()? } else { apply };
    Some(Pick {
        model: strip_ns(v.get("model")?.as_str()?).to_string(),
        x: v.get("x").and_then(Value::as_i64).unwrap_or(0) as i32,
        y: v.get("y").and_then(Value::as_i64).unwrap_or(0) as i32,
        uvlock: v.get("uvlock").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// Models a blockstate selects for `props`.
pub(super) fn pick_models(state: &Value, props: &BTreeMap<String, String>) -> Vec<Pick> {
    let mut picks = Vec::new();
    if let Some(variants) = state.get("variants").and_then(Value::as_object) {
        let matches = |key: &str| {
            key.split(',')
                .filter(|kv| !kv.is_empty())
                .all(|kv| kv.split_once('=').is_some_and(|(k, v)| props.get(k).is_some_and(|have| have == v)))
        };
        if let Some((_, apply)) = variants.iter().find(|(key, _)| matches(key)) {
            picks.extend(pick_from(apply));
        }
    } else if let Some(parts) = state.get("multipart").and_then(Value::as_array) {
        for part in parts {
            let ok = part.get("when").map_or(true, |w| when_matches(w, props));
            if ok {
                if let Some(apply) = part.get("apply") {
                    picks.extend(pick_from(apply));
                }
            }
        }
    }
    picks
}

fn v3(v: &Value) -> Option<V3> {
    let a = v.as_array()?;
    Some([a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32])
}

/// Build one world-oriented element from its JSON.
fn build_element(
    id: &str,
    el: &Value,
    variant: &[[f32; 3]; 3],
    uvlock: bool,
    lookup: &dyn Fn(&str) -> Option<Arc<Texture>>,
) -> Option<Element> {
    let from = v3(el.get("from")?)?;
    let to = v3(el.get("to")?)?;
    let shade = el.get("shade").and_then(Value::as_bool).unwrap_or(true);
    let faces_json = el.get("faces").and_then(Value::as_object)?;

    // The element's own rotation (plants, levers): a free angle about one axis.
    let rot = el.get("rotation").and_then(|r| {
        let origin = v3(r.get("origin")?)?;
        let axis = match r.get("axis")?.as_str()? {
            "x" => 0,
            "y" => 1,
            _ => 2,
        };
        let angle = r.get("angle")?.as_f64()? as f32;
        let rescale = r.get("rescale").and_then(Value::as_bool).unwrap_or(false);
        Some((origin, axis, angle, rescale))
    });
    // Rescaled elements stretch so a 45-degree plane still spans the block.
    let (from, to) = match rot {
        Some((origin, axis, angle, true)) => {
            let s = 1.0 / angle.to_radians().cos().abs().max(1e-3);
            let stretch = |p: V3| {
                let mut q = p;
                for k in 0..3 {
                    if k != axis {
                        q[k] = origin[k] + (p[k] - origin[k]) * s;
                    }
                }
                q
            };
            (stretch(from), stretch(to))
        }
        _ => (from, to),
    };
    let is_identity = *variant == IDENTITY;

    // Faces in the element's own frame.
    let mut faces: Vec<(Face6, FaceTex)> = Vec::new();
    for (name, fv) in faces_json {
        let Some(face) = Face6::parse(name) else { continue };
        let Some(tex_ref) = fv.get("texture").and_then(Value::as_str) else { continue };
        let Some(texture) = lookup(tex_ref) else { continue };
        let uv = fv
            .get("uv")
            .and_then(Value::as_array)
            .and_then(|a| {
                let f = |i: usize| a.get(i).and_then(Value::as_f64).map(|x| x as f32);
                Some([f(0)?, f(1)?, f(2)?, f(3)?])
            })
            .unwrap_or_else(|| default_uv(face, from, to));
        let face_rot = fv.get("rotation").and_then(Value::as_i64).unwrap_or(0);
        let tint = fv.get("tintindex").map(|_| tint_for(id));
        let frame = rotate_frame(face_rot, face_frame(face, from, to));
        faces.push((face, FaceTex { texture, origin: frame.0, du: frame.1, dv: frame.2, uv, tint, shade }));
    }
    if faces.is_empty() {
        return None;
    }

    match rot {
        None if is_identity => Some(Element { from, to, faces, local: None }),
        None => {
            // Bake the 90-degree variant rotation into the geometry.
            let a = rotate_point(variant, from);
            let b = rotate_point(variant, to);
            let (from2, to2) = ([a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])], [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])]);
            let faces = faces
                .into_iter()
                .map(|(face, ft)| {
                    let new_face = Face6::from_normal(mul_mat(variant, face.normal()));
                    let (origin, du, dv) = if uvlock && new_face.is_vertical() {
                        // Locked UVs: the top texture keeps its world orientation.
                        let f = face_frame(new_face, from2, to2);
                        (f.0, f.1, f.2)
                    } else {
                        (rotate_point(variant, ft.origin), mul_mat(variant, ft.du), mul_mat(variant, ft.dv))
                    };
                    (new_face, FaceTex { origin, du, dv, ..ft })
                })
                .collect();
            Some(Element { from: from2, to: to2, faces, local: None })
        }
        Some((origin, axis, angle, _)) => {
            // Keep the box in its own frame; the ray is brought into it.
            // world = V(R(p - o) + o - c) + c, so local = R^-1(V^-1(p - c) + c - o) + o.
            let r_inv = transpose(&axis_matrix(axis, angle));
            let v_inv = transpose(variant);
            let m = mat_mul(&r_inv, &v_inv);
            let t = add(mul_mat(&r_inv, sub(add(mul_mat(&v_inv, [-CENTRE[0], -CENTRE[1], -CENTRE[2]]), CENTRE), origin)), origin);
            Some(Element { from, to, faces, local: Some(LocalFrame { m, t }) })
        }
    }
}

/// Mean colours of the upward and side faces: texture averages with tint,
/// each face weighted by how opaque its texture is (so a mostly transparent
/// overlay like the grass side fringe only tints the dirt behind it), with
/// the most opaque face's alpha.
fn average_colours(elements: &[Element]) -> (Rgba, Rgba) {
    let acc = |vertical_up: bool| -> Option<Rgba> {
        let (mut sum, mut weight, mut alpha) = ([0.0f32; 3], 0.0f32, 0u8);
        for e in elements {
            for (face, ft) in &e.faces {
                let pick = if vertical_up { *face == Face6::Up } else { !face.is_vertical() };
                if !pick {
                    continue;
                }
                let mut c = ft.texture.avg;
                if let Some(t) = ft.tint {
                    for k in 0..3 {
                        c[k] = ((c[k] as u32 * t[k] as u32) / 255) as u8;
                    }
                }
                let w = c[3] as f32 / 255.0;
                for k in 0..3 {
                    sum[k] += c[k] as f32 * w;
                }
                weight += w;
                alpha = alpha.max(c[3]);
            }
        }
        (weight > 0.0).then(|| [(sum[0] / weight) as u8, (sum[1] / weight) as u8, (sum[2] / weight) as u8, alpha])
    };
    let top = acc(true);
    let side = acc(false);
    let fallback = top.or(side).unwrap_or([0, 0, 0, 0]);
    (top.unwrap_or(fallback), side.unwrap_or(fallback))
}

/// Entity texture folders read from the jar, for the blocks the game draws
/// with a block-entity renderer instead of a block model (see [`super::entity`]).
/// `banner` takes both `banner/` (the pattern masks) and `banner_base.png`.
const ENTITY_TEXTURE_DIRS: [&str; 5] = ["chest/", "decorated_pot/", "bed/", "signs/", "banner"];

/// Register the models [`super::entity`] builds for entity-rendered blocks,
/// replacing the jar's particle-only models and block states for them.
fn add_entity_models(
    textures: &HashMap<String, Arc<Texture>>,
    models: &mut HashMap<String, RawModel>,
    blockstates: &mut HashMap<String, Value>,
) {
    let (new_models, states) = super::entity::entity_models(textures.keys().map(String::as_str));
    for (key, model) in new_models {
        let textures = model
            .get("textures")
            .and_then(Value::as_object)
            .map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
            .unwrap_or_default();
        models.insert(key, RawModel { parent: None, textures, elements: model.get("elements").and_then(Value::as_array).cloned() });
    }
    for (id, state) in states {
        blockstates.insert(id, state);
    }
}
