//! Orthographic side elevations. The viewer stands on one side of the grid and
//! looks across it; each pixel column shows the nearest block along the view
//! axis (composited through glass/water), darkened with distance so nearby
//! facades pop and far terrain recedes.
//!
//! A block with a textured model shows the texels of its faces turned toward
//! the viewer (nearest face first, box-filtered per pixel); cut-out texels and
//! uncovered parts of the cell let the ray carry on to the blocks behind.

use resvg::tiny_skia::Pixmap;

use super::assets::Face6;
use super::grid::{canvas, fill_square, shade, RayAcc, Rgba, VoxelGrid, BACKGROUND};
use super::shape::covers;
use super::texel;

/// The side the viewer stands on. `South` looks north (east on the right).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElevationSide {
    South,
    North,
    East,
    West,
}

impl ElevationSide {
    pub const ALL: [ElevationSide; 4] =
        [ElevationSide::South, ElevationSide::North, ElevationSide::East, ElevationSide::West];

    pub fn name(self) -> &'static str {
        match self {
            ElevationSide::South => "south",
            ElevationSide::North => "north",
            ElevationSide::East => "east",
            ElevationSide::West => "west",
        }
    }

    /// What the image shows, for index files.
    pub fn describe(self) -> &'static str {
        match self {
            ElevationSide::South => "seen from the south looking north; west left, east right",
            ElevationSide::North => "seen from the north looking south; east left, west right",
            ElevationSide::East => "seen from the east looking west; south left, north right",
            ElevationSide::West => "seen from the west looking east; north left, south right",
        }
    }

    /// `(columns, depth)` and the map `(column, depth) -> (x, z)`; depth 0 is
    /// nearest the viewer.
    fn frame(self, g: &VoxelGrid) -> (i32, i32, impl Fn(i32, i32) -> (i32, i32)) {
        let (nx, nz) = (g.nx, g.nz);
        let (cols, depth) = match self {
            ElevationSide::South | ElevationSide::North => (nx, nz),
            ElevationSide::East | ElevationSide::West => (nz, nx),
        };
        let map = move |c: i32, d: i32| match self {
            ElevationSide::South => (c, nz - 1 - d),
            ElevationSide::North => (nx - 1 - c, d),
            ElevationSide::East => (nx - 1 - d, nz - 1 - c),
            ElevationSide::West => (d, c),
        };
        (cols, depth, map)
    }

    /// The block face turned toward the viewer.
    fn facing(self) -> Face6 {
        match self {
            ElevationSide::South => Face6::South,
            ElevationSide::North => Face6::North,
            ElevationSide::East => Face6::East,
            ElevationSide::West => Face6::West,
        }
    }
}

/// Render one elevation at `px` pixels per block with a 1-block margin.
/// Image size: `(columns + 2) * px` × `(ny + 2) * px`.
///
/// Rays through flat-coloured full cubes only are resolved once per block;
/// when a ray meets a partial shape (slab, fence, pane, …) or a textured
/// block, the block's square is resolved per pixel against the shapes' boxes
/// and the models' face rasters, so a half slab shows half a block and
/// whatever lies behind it fills the rest.
pub fn render_elevation(g: &VoxelGrid, side: ElevationSide, px: u32) -> anyhow::Result<Pixmap> {
    let px = px.max(1) as usize;
    let (cols, depth, map) = side.frame(g);
    let mut pm = canvas((cols as usize + 2) * px, (g.ny as usize + 2) * px)?;
    let fog_span = (depth - 1).max(1) as f32;
    let fog = |d: i32| 1.0 - 0.5 * d as f32 / fog_span;
    let fogged = |m: u16, d: i32| shade(g.color(m), fog(d));
    // Image-left-to-right position inside a cell -> cell-local X or Z.
    let (lat_is_x, flip) = match side {
        ElevationSide::South => (true, false),
        ElevationSide::North => (true, true),
        ElevationSide::East => (false, true),
        ElevationSide::West => (false, false),
    };

    // Facing-face rasters of every textured material at this pixel size, and
    // which materials stop every ray (opaque full cubes).
    let models = texel::flat_models(g);
    let stamps: Vec<Option<Vec<Vec<Rgba>>>> =
        models.iter().map(|fm| fm.as_ref().map(|fm| texel::side_stamps(fm.geo, side.facing(), px, flip))).collect();
    let stops_rays = |m: u16| match &models[m as usize] {
        Some(fm) => fm.solid,
        None => g.is_full(m) && g.is_opaque(m),
    };

    let mut cells: Vec<(i32, u16)> = Vec::new();
    for c in 0..cols {
        for y in 0..g.ny {
            let row = (g.ny - 1 - y) as usize + 1;
            // The blocks along the ray up to the first that stops it.
            cells.clear();
            let mut per_pixel = false;
            for d in 0..depth {
                let (x, z) = map(c, d);
                let m = g.get(x, y, z);
                if m == 0 {
                    continue;
                }
                cells.push((d, m));
                per_pixel |= stamps[m as usize].is_some() || !g.is_full(m);
                if stops_rays(m) {
                    break;
                }
            }
            if cells.is_empty() {
                continue;
            }
            if !per_pixel {
                let mut acc = RayAcc::default();
                for &(d, m) in &cells {
                    if acc.push(fogged(m, d)) {
                        break;
                    }
                }
                fill_square(&mut pm, (c as usize + 1) * px, row * px, px, acc.finish(BACKGROUND));
                continue;
            }

            for j in 0..px {
                let fy = 16.0 * (1.0 - (j as f32 + 0.5) / px as f32);
                for i in 0..px {
                    let f = (i as f32 + 0.5) / px as f32;
                    let lat = 16.0 * if flip { 1.0 - f } else { f };
                    let k = j * px + i;
                    let mut acc = RayAcc::default();
                    let mut hit = false;
                    'ray: for &(d, m) in &cells {
                        if let Some(st) = &stamps[m as usize] {
                            for raster in st {
                                let t = raster[k];
                                if t[3] == 0 {
                                    continue;
                                }
                                hit = true;
                                if acc.push(shade(t, fog(d))) {
                                    break 'ray;
                                }
                            }
                        } else if covers(g.boxes(m), lat_is_x, lat, fy) {
                            hit = true;
                            if acc.push(fogged(m, d)) {
                                break;
                            }
                        }
                    }
                    if hit {
                        fill_square(&mut pm, (c as usize + 1) * px + i, row * px + j, 1, acc.finish(BACKGROUND));
                    }
                }
            }
        }
    }
    Ok(pm)
}
