//! Dense voxel grid built from a [`Scene`], plus tiny raster helpers shared by
//! every view. All views read the grid, never the scene's hash map, so lookups
//! are plain array indexing.

use crate::util::HashMap;

use resvg::tiny_skia::Pixmap;

use super::colors;
use super::scene::Scene;
use crate::geometry::{Point3D, Rect3D};

pub type Rgba = [u8; 4];

/// Image background (sky / empty columns).
pub const BACKGROUND: Rgba = [236, 240, 244, 255];

/// A dense box of material indices in local coordinates `0..nx`, `0..ny`,
/// `0..nz`. Index 0 is empty; others index `materials` (deduplicated by colour,
/// so `water[level=0]` and `water[level=3]` share one material).
pub struct VoxelGrid {
    /// Absolute world coordinate of local cell (0, 0, 0).
    pub origin: Point3D,
    pub nx: i32,
    pub ny: i32,
    pub nz: i32,
    /// Column-major: `(x * nz + z) * ny + y`, so a column is contiguous.
    cells: Vec<u16>,
    materials: Vec<Rgba>,
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

        let mut materials: Vec<Rgba> = vec![[0, 0, 0, 0]];
        let mut by_color: HashMap<Rgba, u16> = HashMap::default();
        let palette_mat: Vec<u16> = scene
            .palette
            .iter()
            .map(|s| {
                let c = colors::block_color(Scene::strip_state(s));
                if c[3] == 0 {
                    return 0;
                }
                *by_color.entry(c).or_insert_with(|| {
                    materials.push(c);
                    (materials.len() - 1) as u16
                })
            })
            .collect();

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
        let mut grid =
            VoxelGrid { origin, nx, ny, nz, cells: vec![0; (nx as usize) * (ny as usize) * (nz as usize)], materials };
        for (p, &i) in &scene.blocks {
            if visible(p, i) {
                let idx = grid.index(p.x - origin.x, p.y - origin.y, p.z - origin.z);
                grid.cells[idx] = palette_mat[i as usize];
            }
        }
        Some(grid)
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

    pub fn color(&self, m: u16) -> Rgba {
        self.materials[m as usize]
    }

    pub fn is_opaque(&self, m: u16) -> bool {
        m != 0 && self.materials[m as usize][3] == 255
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
