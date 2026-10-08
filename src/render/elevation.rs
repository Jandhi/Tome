//! Orthographic side elevations. The viewer stands on one side of the grid and
//! looks across it; each pixel column shows the nearest block along the view
//! axis (composited through glass/water), darkened with distance so nearby
//! facades pop and far terrain recedes.

use resvg::tiny_skia::Pixmap;

use super::grid::{canvas, fill_square, shade, RayAcc, VoxelGrid, BACKGROUND};

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
}

/// Render one elevation at `px` pixels per block with a 1-block margin.
/// Image size: `(columns + 2) * px` × `(ny + 2) * px`.
pub fn render_elevation(g: &VoxelGrid, side: ElevationSide, px: u32) -> anyhow::Result<Pixmap> {
    let px = px.max(1) as usize;
    let (cols, depth, map) = side.frame(g);
    let mut pm = canvas((cols as usize + 2) * px, (g.ny as usize + 2) * px)?;
    let fog_span = (depth - 1).max(1) as f32;

    for c in 0..cols {
        for y in 0..g.ny {
            let mut acc = RayAcc::default();
            let mut hit = false;
            for d in 0..depth {
                let (x, z) = map(c, d);
                let m = g.get(x, y, z);
                if m == 0 {
                    continue;
                }
                hit = true;
                if acc.push(shade(g.color(m), 1.0 - 0.5 * d as f32 / fog_span)) {
                    break;
                }
            }
            if hit {
                let row = (g.ny - 1 - y) as usize + 1;
                fill_square(&mut pm, (c as usize + 1) * px, row * px, px, acc.finish(BACKGROUND));
            }
        }
    }
    Ok(pm)
}
