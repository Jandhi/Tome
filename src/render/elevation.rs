//! Orthographic side elevations. The viewer stands on one side of the grid and
//! looks across it; each pixel column shows the nearest block along the view
//! axis (composited through glass/water), darkened with distance so nearby
//! facades pop and far terrain recedes.
//!
//! A block with a textured model shows the texels of its faces turned toward
//! the viewer (nearest face first, box-filtered per pixel); cut-out texels and
//! uncovered parts of the cell let the ray carry on to the blocks behind.
//! A model with freely rotated elements gets the same rasters by casting the
//! view's rays through it ([`texel::peel`]).
//! A model reaching past its cell (a wall banner's flag, a pot's neck) is
//! drawn piece by piece by the cells it lies in: a cell it reaches into
//! merges that part's faces with its own block's, nearest first.
//!
//! With a [`Lit`] every ray is resolved per pixel: its first sample is lit by
//! the facing face's light from the cell in front of it (toward the viewer),
//! interpolated across the face, and the samples behind it by their own
//! cell; the depth fog applies as ever. Without one nothing changes.

use resvg::tiny_skia::Pixmap;

use super::assets::Face6;
use super::grid::{canvas, fill_square, shade, RayAcc, Rgba, VoxelGrid, BACKGROUND};
use super::lit::{axis_of, tint, FaceLight, Lit};
use super::shape::covers;
use super::texel;
use crate::util::HashMap;

/// Facing faces of one cell, `(distance toward the viewer in sixteenths,
/// raster)`, nearest first.
type SideStamps = Vec<(f32, Vec<Rgba>)>;

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

    /// The image column of grid column `(x, z)` (inverse of [`frame`](Self::frame)'s map).
    fn column(self, g: &VoxelGrid, x: i32, z: i32) -> i32 {
        match self {
            ElevationSide::South => x,
            ElevationSide::North => g.nx - 1 - x,
            ElevationSide::East => g.nz - 1 - z,
            ElevationSide::West => z,
        }
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

/// Render one elevation at `px` pixels per block with a 1-block margin,
/// unlit. Image size: `(columns + 2) * px` × `(ny + 2) * px`.
///
/// Rays through flat-coloured full cubes only are resolved once per block;
/// when a ray meets a partial shape (slab, fence, pane, …) or a textured
/// block, the block's square is resolved per pixel against the shapes' boxes
/// and the models' face rasters, so a half slab shows half a block and
/// whatever lies behind it fills the rest.
pub fn render_elevation(g: &VoxelGrid, side: ElevationSide, px: u32) -> anyhow::Result<Pixmap> {
    render_elevation_with(g, side, px, None)
}

/// [`render_elevation`], lit by `lit` when given (see the module doc).
pub fn render_elevation_with(g: &VoxelGrid, side: ElevationSide, px: u32, lit: Option<&Lit>) -> anyhow::Result<Pixmap> {
    let px = px.max(1) as usize;
    let (cols, depth, map) = side.frame(g);
    let mut pm = canvas((cols as usize + 2) * px, (g.ny as usize + 2) * px)?;
    let bg = lit.map_or(BACKGROUND, |l| l.background());
    if lit.is_some() {
        let [r, g, b, a] = bg;
        pm.fill(resvg::tiny_skia::Color::from_rgba8(r, g, b, a));
    }
    // The facing face's normal axis; its light coordinates are the fractions
    // along the two other axes in ascending order (x, y for a south or
    // north face; y, z for an east or west one).
    let axis = axis_of(side.facing());
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
    let stamps: Vec<Option<SideStamps>> = models
        .iter()
        .map(|fm| fm.as_ref().map(|fm| texel::side_stamps(fm.geo, side.facing(), px, flip, [0; 3])))
        .collect();
    let stops_rays = |m: u16| match &models[m as usize] {
        Some(fm) => fm.solid,
        None => g.is_full(m) && g.is_opaque(m),
    };

    // Cells that textured models reach into from a neighbour: the faces
    // drawn there (the cell's own textured block's and the overhanging
    // parts), nearest first, and which rays (column * ny + y) pass through them.
    let mut layers: HashMap<[i32; 3], SideStamps> = HashMap::default();
    let mut over_rays: Vec<bool> = Vec::new();
    // The faces of one overhanging piece, per (material, model's cell minus
    // drawn cell), shared by every cell holding the same piece.
    let mut pieces: HashMap<(u16, [i32; 3]), SideStamps> = HashMap::default();
    for (cell, sources) in g.overhang_cells() {
        let [x, y, z] = cell;
        let mut faces: SideStamps = Vec::new();
        for s in sources {
            let sm = g.get(s[0], s[1], s[2]);
            if let Some(fm) = &models[sm as usize] {
                let off = [s[0] - x, s[1] - y, s[2] - z];
                let piece = pieces
                    .entry((sm, off))
                    .or_insert_with(|| texel::side_stamps(fm.geo, side.facing(), px, flip, off));
                faces.extend(piece.iter().cloned());
            }
        }
        if faces.is_empty() {
            continue;
        }
        if let Some(own) = &stamps[g.get(x, y, z) as usize] {
            faces.splice(0..0, own.iter().cloned());
        }
        faces.sort_by(|a, b| b.0.total_cmp(&a.0));
        layers.insert(cell, faces);
        over_rays.resize((cols * g.ny) as usize, false);
        over_rays[(side.column(g, x, z) * g.ny + y) as usize] = true;
    }

    // The blocks along a ray up to the first that stops it: depth, material,
    // and the merged faces of a cell models reach into.
    let mut cells: Vec<(i32, u16, Option<&SideStamps>)> = Vec::new();
    // Light of the facing face of `cells[idx]`, from the cell in front of it.
    let mut face_light: Vec<Option<FaceLight>> = Vec::new();
    let face_light_of = |l: &Lit, idx: usize, c: i32, y: i32, d: i32, cache: &mut Vec<Option<FaceLight>>| -> FaceLight {
        *cache[idx].get_or_insert_with(|| {
            let (x, z) = map(c, d - 1);
            l.face([x, y, z], axis)
        })
    };
    for c in 0..cols {
        for y in 0..g.ny {
            let row = (g.ny - 1 - y) as usize + 1;
            // The blocks along the ray up to the first that stops it.
            cells.clear();
            let mut per_pixel = false;
            if !over_rays.is_empty() && over_rays[(c * g.ny + y) as usize] {
                for d in 0..depth {
                    let (x, z) = map(c, d);
                    let m = g.get(x, y, z);
                    let merged = layers.get(&[x, y, z]);
                    if m == 0 && merged.is_none() {
                        continue;
                    }
                    cells.push((d, m, merged));
                    per_pixel = true;
                    if stops_rays(m) {
                        break;
                    }
                }
            } else {
                for d in 0..depth {
                    let (x, z) = map(c, d);
                    let m = g.get(x, y, z);
                    if m == 0 {
                        continue;
                    }
                    cells.push((d, m, None));
                    per_pixel |= stamps[m as usize].is_some() || !g.is_full(m);
                    if stops_rays(m) {
                        break;
                    }
                }
            }
            if cells.is_empty() {
                continue;
            }
            if !per_pixel && lit.is_none() {
                let mut acc = RayAcc::default();
                for &(d, m, _) in &cells {
                    if acc.push(fogged(m, d)) {
                        break;
                    }
                }
                fill_square(&mut pm, (c as usize + 1) * px, row * px, px, acc.finish(bg));
                continue;
            }
            face_light.clear();
            face_light.resize(cells.len(), None);

            for j in 0..px {
                let fy = 16.0 * (1.0 - (j as f32 + 0.5) / px as f32);
                for i in 0..px {
                    let f = (i as f32 + 0.5) / px as f32;
                    let lat = 16.0 * if flip { 1.0 - f } else { f };
                    let (fu, fv) = if axis == 2 { (lat / 16.0, fy / 16.0) } else { (fy / 16.0, lat / 16.0) };
                    let k = j * px + i;
                    let mut acc = RayAcc::default();
                    let mut hit = false;
                    // Light a sample of `cells[idx]`: the ray's first by the
                    // face in front, the rest by their own cell.
                    let light = |s: Rgba, idx: usize, d: i32, first: bool, cache: &mut Vec<Option<FaceLight>>| match lit {
                        Some(l) if first => tint(s, face_light_of(l, idx, c, y, d, cache).at(fu, fv)),
                        Some(l) => {
                            let (x, z) = map(c, d);
                            tint(s, l.cell([x, y, z]))
                        }
                        None => s,
                    };
                    'ray: for (idx, &(d, m, merged)) in cells.iter().enumerate() {
                        // A cell models reach into draws its own untextured
                        // block (water, say, filling the cell) first, then
                        // the merged faces.
                        let st = match merged {
                            Some(faces) => {
                                if m != 0 && stamps[m as usize].is_none() && covers(g.boxes(m), lat_is_x, lat, fy) {
                                    let s = light(fogged(m, d), idx, d, !hit, &mut face_light);
                                    hit = true;
                                    if acc.push(s) {
                                        break 'ray;
                                    }
                                }
                                Some(faces)
                            }
                            None => stamps[m as usize].as_ref(),
                        };
                        if let Some(st) = st {
                            for (_, raster) in st {
                                let t = raster[k];
                                if t[3] == 0 {
                                    continue;
                                }
                                let s = light(shade(t, fog(d)), idx, d, !hit, &mut face_light);
                                hit = true;
                                if acc.push(s) {
                                    break 'ray;
                                }
                            }
                        } else if covers(g.boxes(m), lat_is_x, lat, fy) {
                            let s = light(fogged(m, d), idx, d, !hit, &mut face_light);
                            hit = true;
                            if acc.push(s) {
                                break;
                            }
                        }
                    }
                    if hit {
                        fill_square(&mut pm, (c as usize + 1) * px + i, row * px + j, 1, acc.finish(bg));
                    }
                }
            }
        }
    }
    Ok(pm)
}
