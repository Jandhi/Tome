//! Dense voxel grid built from a [`Scene`], plus tiny raster helpers shared by
//! every view. All views read the grid, never the scene's hash map, so lookups
//! are plain array indexing.

use std::sync::Arc;

use crate::util::HashMap;

use resvg::tiny_skia::Pixmap;

use super::assets::{self, BlockGeometry};
use super::colors;
use super::scene::Scene;
use super::shape::{self, Box16, Shape};
use crate::geometry::{Point3D, Rect3D};

pub type Rgba = [u8; 4];

/// Image background (sky / empty columns).
pub const BACKGROUND: Rgba = [236, 240, 244, 255];

/// A dense box of material indices in local coordinates `0..nx`, `0..ny`,
/// `0..nz`. Index 0 is empty; others index `materials` (deduplicated by
/// colours, [`Shape`] and model, so `water[level=0]` and `water[level=3]`
/// share one material).
///
/// When Minecraft's assets are available (see [`assets`]) a material also
/// carries the block's model: textured faces for the perspective views, and
/// texture-average top/side colours for the flat ones. The hand-written
/// colour table still decides which blocks are empty or translucent, and
/// supplies the colour of blocks the jar has no element model for.
pub struct VoxelGrid {
    /// Absolute world coordinate of local cell (0, 0, 0).
    pub origin: Point3D,
    pub nx: i32,
    pub ny: i32,
    pub nz: i32,
    /// Column-major: `(x * nz + z) * ny + y`, so a column is contiguous.
    cells: Vec<u16>,
    /// Top-face colour per material (the flat views' main colour).
    materials: Vec<Rgba>,
    /// Side-face colour per material (equals `materials` without textures).
    side_colors: Vec<Rgba>,
    shapes: Vec<Shape>,
    /// `shapes[m].boxes()`, cached.
    boxes: Vec<Vec<Box16>>,
    /// Textured model per material, when the assets have one.
    geometry: Vec<Option<Arc<BlockGeometry>>>,
}

impl VoxelGrid {
    /// Rasterise `scene` inside `region` (default: the scene bounds). The y
    /// range is shrunk to the blocks actually present. `None` when nothing
    /// visible falls inside.
    pub fn from_scene(scene: &Scene, region: Option<Rect3D>) -> Option<VoxelGrid> {
        let crop = region.unwrap_or(scene.bounds);
        if crop.size.x <= 0 || crop.size.y <= 0 || crop.size.z <= 0 {
            return None;
        }

        let assets = assets::assets();
        let mut materials: Vec<Rgba> = vec![[0, 0, 0, 0]];
        let mut side_colors: Vec<Rgba> = vec![[0, 0, 0, 0]];
        let mut shapes: Vec<Shape> = vec![Shape::Full];
        let mut geometry: Vec<Option<Arc<BlockGeometry>>> = vec![None];
        // Key: colours, shape and the model's identity (models are cached per
        // block string, so the pointer is stable). 0 = no model.
        let mut by_key: HashMap<(Rgba, Rgba, Shape, usize), u16> = HashMap::default();
        // Intern a full block string. The hand-written colour decides whether
        // the block shows at all and how translucent it is; the jar's model,
        // when it has one, supplies the faces and the colours. A block the
        // table doesn't know but the jar does takes its alpha from the
        // texture too, so new translucent blocks still render as such; only
        // blocks neither knows get the hashed fallback colour.
        let mut intern = |block: &str| -> u16 {
            let known = colors::known_color(block);
            if known.is_some_and(|c| c[3] == 0) {
                return 0;
            }
            let shape = Shape::from_block(block);
            let geo = assets.and_then(|a| a.geometry(block));
            let (top, side) = match (&geo, known) {
                (Some(g), Some(hand)) => ([g.top[0], g.top[1], g.top[2], hand[3]], [g.side[0], g.side[1], g.side[2], hand[3]]),
                (Some(g), None) => (g.top, [g.side[0], g.side[1], g.side[2], g.top[3]]),
                (None, _) => {
                    let hand = colors::block_color(block);
                    (hand, hand)
                }
            };
            if top[3] == 0 {
                return 0;
            }
            let key = (top, side, shape, geo.as_ref().map_or(0, |g| Arc::as_ptr(g) as usize));
            *by_key.entry(key).or_insert_with(|| {
                materials.push(top);
                side_colors.push(side);
                shapes.push(shape);
                geometry.push(geo);
                (materials.len() - 1) as u16
            })
        };
        let palette_mat: Vec<u16> = scene.palette.iter().map(|s| intern(s)).collect();
        // Stateless fences / walls / panes link to their neighbours per cell.
        let needs_links: Vec<bool> = scene.palette.iter().map(|s| shape::needs_connections(s)).collect();
        let links_to = |q: Point3D| {
            scene.blocks.get(&q).is_some_and(|&i| {
                let s = &scene.palette[i as usize];
                palette_mat[i as usize] != 0 && (shape::connects(s) || Shape::from_block(s).is_full())
            })
        };
        // Stairs get their corner shape from the neighbours, as the server does.
        let is_stairs: Vec<bool> = scene.palette.iter().map(|s| shape::is_stairs(s)).collect();
        let stair_at = |q: Point3D| scene.blocks.get(&q).and_then(|&i| shape::stair_info(&scene.palette[i as usize]));
        let mut cell_mat = |p: &Point3D, i: u32| -> u16 {
            let m = palette_mat[i as usize];
            if m == 0 {
                return m;
            }
            let s = &scene.palette[i as usize];
            if is_stairs[i as usize] {
                return match shape::infer_stairs_shape(s, |d| stair_at(d.offset(*p))) {
                    Some(sh) if shape::state(s, "shape") != Some(sh) => intern(&shape::with_state(s, "shape", sh)),
                    _ => m,
                };
            }
            if !needs_links[i as usize] {
                return m;
            }
            let sides = [(0, -1, shape::N), (1, 0, shape::E), (0, 1, shape::S), (-1, 0, shape::W)]
                .iter()
                .filter(|&&(dx, dz, _)| links_to(Point3D::new(p.x + dx, p.y, p.z + dz)))
                .fold(0, |acc, &(_, _, bit)| acc | bit);
            intern(&shape::connected_block(s, sides))
        };

        let visible = |p: &Point3D, i: u32| crop.contains(*p) && palette_mat[i as usize] != 0;
        let (mut y_min, mut y_max) = (i32::MAX, i32::MIN);
        for (p, &i) in &scene.blocks {
            if visible(p, i) {
                y_min = y_min.min(p.y);
                y_max = y_max.max(p.y);
            }
        }
        if y_min > y_max {
            return None;
        }

        let (nx, ny, nz) = (crop.size.x, y_max - y_min + 1, crop.size.z);
        let origin = Point3D::new(crop.origin.x, y_min, crop.origin.z);
        let mut cells = vec![0; (nx as usize) * (ny as usize) * (nz as usize)];
        for (p, &i) in &scene.blocks {
            if visible(p, i) {
                let (x, y, z) = (p.x - origin.x, p.y - origin.y, p.z - origin.z);
                cells[((x * nz + z) * ny + y) as usize] = cell_mat(p, i);
            }
        }
        let boxes = shapes.iter().map(|s| s.boxes()).collect();
        Some(VoxelGrid { origin, nx, ny, nz, cells, materials, side_colors, shapes, boxes, geometry })
    }

    fn index(&self, x: i32, y: i32, z: i32) -> usize {
        ((x * self.nz + z) * self.ny + y) as usize
    }

    /// Material at local `(x, y, z)`; 0 (empty) outside the grid.
    pub fn get(&self, x: i32, y: i32, z: i32) -> u16 {
        if x < 0 || y < 0 || z < 0 || x >= self.nx || y >= self.ny || z >= self.nz {
            return 0;
        }
        self.cells[self.index(x, y, z)]
    }

    /// Top-face colour of material `m` (the flat views' main colour).
    pub fn color(&self, m: u16) -> Rgba {
        self.materials[m as usize]
    }

    /// Side-face colour of material `m`; the same as [`color`](Self::color)
    /// unless textures gave the block distinct top and side faces.
    pub fn side_color(&self, m: u16) -> Rgba {
        self.side_colors[m as usize]
    }

    /// The block model behind material `m`, when Minecraft's assets supplied one.
    pub fn geometry(&self, m: u16) -> Option<&BlockGeometry> {
        self.geometry[m as usize].as_deref()
    }

    /// `true` when any material carries a textured model.
    pub fn has_textures(&self) -> bool {
        self.geometry.iter().any(Option::is_some)
    }

    pub fn is_opaque(&self, m: u16) -> bool {
        m != 0 && self.materials[m as usize][3] == 255
    }

    /// Number of materials, including the empty material 0.
    pub fn material_count(&self) -> usize {
        self.materials.len()
    }

    pub fn shape(&self, m: u16) -> Shape {
        self.shapes[m as usize]
    }

    /// The material's shape as boxes in sixteenths (world orientation).
    pub fn boxes(&self, m: u16) -> &[Box16] {
        &self.boxes[m as usize]
    }

    /// `true` when material `m` fills its whole cell.
    pub fn is_full(&self, m: u16) -> bool {
        m != 0 && self.shapes[m as usize].is_full()
    }

    /// Whether neighbour `n` hides the face of `m` that touches it: a full
    /// opaque cube hides anything; a translucent neighbour of the same
    /// material hides the shared face (no inner faces in water or glass).
    pub fn hides(&self, n: u16, m: u16) -> bool {
        n != 0 && ((self.is_full(n) && self.is_opaque(n)) || (n == m && (self.is_full(m) || !self.is_opaque(m))))
    }
}

/// A pixmap filled with [`BACKGROUND`].
pub fn canvas(w: usize, h: usize) -> anyhow::Result<Pixmap> {
    let mut pm = Pixmap::new(w as u32, h as u32).ok_or_else(|| anyhow::anyhow!("bad image size {w}x{h}"))?;
    let [r, g, b, a] = BACKGROUND;
    pm.fill(resvg::tiny_skia::Color::from_rgba8(r, g, b, a));
    Ok(pm)
}

/// Blend straight-alpha `c` over the premultiplied pixel at byte offset `i`.
pub fn blend(data: &mut [u8], i: usize, c: Rgba) {
    let a = c[3] as u32;
    if a == 255 {
        data[i..i + 4].copy_from_slice(&c);
        return;
    }
    for k in 0..3 {
        data[i + k] = ((c[k] as u32 * a + data[i + k] as u32 * (255 - a)) / 255) as u8;
    }
    data[i + 3] = (a + data[i + 3] as u32 * (255 - a) / 255) as u8;
}

/// Fill an axis-aligned square of side `size` at pixel `(x0, y0)` (opaque `c`).
pub fn fill_square(pm: &mut Pixmap, x0: usize, y0: usize, size: usize, c: Rgba) {
    let w = pm.width() as usize;
    let data = pm.data_mut();
    for y in y0..y0 + size {
        for x in x0..x0 + size {
            data[(y * w + x) * 4..][..4].copy_from_slice(&c);
        }
    }
}

/// Multiply the colour channels by `f` (alpha unchanged).
pub fn shade(c: Rgba, f: f32) -> Rgba {
    let s = |v: u8| (v as f32 * f).round().clamp(0.0, 255.0) as u8;
    [s(c[0]), s(c[1]), s(c[2]), c[3]]
}

/// Front-to-back compositing along a ray: push samples nearest-first until
/// [`push`](RayAcc::push) says the ray is saturated, then [`finish`](RayAcc::finish).
#[derive(Default)]
pub struct RayAcc {
    rgb: [f32; 3],
    a: f32,
}

impl RayAcc {
    /// Add a sample behind everything pushed so far. Returns `true` once the
    /// accumulated alpha is (nearly) opaque and further samples can't show.
    pub fn push(&mut self, c: Rgba) -> bool {
        let w = (1.0 - self.a) * c[3] as f32 / 255.0;
        for k in 0..3 {
            self.rgb[k] += w * c[k] as f32;
        }
        self.a += w;
        self.a >= 0.985
    }

    /// Opaque result over `bg`.
    pub fn finish(&self, bg: Rgba) -> Rgba {
        let px = |k: usize| (self.rgb[k] + (1.0 - self.a) * bg[k] as f32).round().clamp(0.0, 255.0) as u8;
        [px(0), px(1), px(2), 255]
    }
}
