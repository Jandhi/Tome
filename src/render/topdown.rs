//! Orthographic top-down view: north (-Z) up, east (+X) right.
//!
//! Each column shows its topmost block, composited through translucent blocks
//! (water gets bluer with depth, glass shows what's under it). Hill shading
//! compares the column's surface height with its north and west neighbours
//! (light from the north-west), and a gentle overall height gradient keeps
//! flat plateaus at different levels distinguishable.
//!
//! Blocks keep their full footprint, but when the top block is a slab, stairs
//! or another shape covering the whole cell at varying heights, each pixel is
//! darkened by how far below the cell top its surface sits, so slab roofs read
//! darker than full blocks and stairs show their step.
//!
//! When the top block has a textured model, each pixel instead looks down its
//! own ray: the highest upward face of the model under it supplies the texel
//! (box-filtered to the pixel), cut-out texels and uncovered footprint fall
//! through to the blocks below, and the height shading uses the face hit.
//! A model with freely rotated elements gets its rasters by casting rays
//! down through it ([`texel::peel`]); one showing nothing from above (a
//! flower's crossed planes) keeps the flat-colour square.
//! A model reaching past its cell (a pot's neck, a standing banner's top) is
//! drawn piece by piece by the cells it lies in: a cell it reaches into
//! merges that part's upward faces with its own block's, highest first, and
//! a column holding such a cell goes down the per-pixel path.
//!
//! With a [`Lit`] every column goes down its per-pixel ray: the first sample
//! is lit by the top face's light from the cell above the surface block
//! (smooth light interpolated across the cell), samples behind it (water
//! under its surface, what shows through glass) by their own cell, and the
//! hill shading applies on top as ever. Without one nothing changes.

use resvg::tiny_skia::Pixmap;

use super::grid::{canvas, fill_square, shade, RayAcc, Rgba, VoxelGrid, BACKGROUND};
use super::lit::{tint, FaceLight, Lit};
use super::shape::top_at;
use super::texel;
use crate::util::HashMap;

/// Upward faces of one cell, `(height in sixteenths, raster)`, highest first.
type TopStamps = Vec<(f32, Vec<Rgba>)>;

/// Brightness at the bottom of a cell relative to its top.
const LOW_SURFACE: f32 = 0.8;

/// Render the grid top-down at `px` pixels per block with a 1-block margin,
/// unlit. Image size: `(nx + 2) * px` × `(nz + 2) * px`.
pub fn render_topdown(g: &VoxelGrid, px: u32) -> anyhow::Result<Pixmap> {
    render_topdown_with(g, px, None)
}

/// [`render_topdown`], lit by `lit` when given (see the module doc).
pub fn render_topdown_with(g: &VoxelGrid, px: u32, lit: Option<&Lit>) -> anyhow::Result<Pixmap> {
    let px = px.max(1) as usize;
    let (nx, nz) = (g.nx as usize, g.nz as usize);
    let mut pm = canvas((nx + 2) * px, (nz + 2) * px)?;
    let bg = lit.map_or(BACKGROUND, |l| l.background());
    if lit.is_some() {
        let [r, g, b, a] = bg;
        pm.fill(resvg::tiny_skia::Color::from_rgba8(r, g, b, a));
    }

    // Pass 1: surface height, top material and composited colour per column.
    let mut heights: Vec<Option<i32>> = vec![None; nx * nz];
    let mut tops: Vec<u16> = vec![0; nx * nz];
    let mut colors = vec![BACKGROUND; nx * nz];
    for x in 0..g.nx {
        for z in 0..g.nz {
            let i = x as usize * nz + z as usize;
            let mut acc = RayAcc::default();
            for y in (0..g.ny).rev() {
                let m = g.get(x, y, z);
                if m == 0 {
                    continue;
                }
                if heights[i].is_none() {
                    heights[i] = Some(y);
                    tops[i] = m;
                }
                if acc.push(g.color(m)) {
                    break;
                }
            }
            colors[i] = acc.finish(BACKGROUND);
        }
    }

    // Upward-face rasters of every textured material at this pixel size. A
    // rotated model showing nothing from above (a flower's crossed planes
    // stand edge-on) keeps its flat-colour square, so meadows stay on the map.
    let models = texel::flat_models(g);
    let stamps: Vec<Option<TopStamps>> = models
        .iter()
        .map(|fm| {
            let fm = fm.as_ref()?;
            let st = texel::top_stamps(fm.geo, px);
            (!st.is_empty() || !texel::is_rotated(fm.geo)).then_some(st)
        })
        .collect();

    // Cells that textured models reach into from a neighbour: the upward
    // faces drawn there (the cell's own textured block's and the overhanging
    // parts), and per column the highest such cell. Those columns take the
    // per-pixel path, looking down from that cell when it lies above the
    // column's blocks; the hill shading still follows the blocks.
    let mut layers: HashMap<[i32; 3], TopStamps> = HashMap::default();
    let mut over_cols: Vec<Option<i32>> = Vec::new();
    // The faces of one overhanging piece, per (material, model's cell minus
    // drawn cell), shared by every cell holding the same piece.
    let mut pieces: HashMap<(u16, [i32; 3]), TopStamps> = HashMap::default();
    for (cell, sources) in g.overhang_cells() {
        let [x, y, z] = cell;
        let mut faces: TopStamps = Vec::new();
        for s in sources {
            let sm = g.get(s[0], s[1], s[2]);
            if let (Some(fm), Some(_)) = (&models[sm as usize], &stamps[sm as usize]) {
                let off = [s[0] - x, s[1] - y, s[2] - z];
                let piece = pieces.entry((sm, off)).or_insert_with(|| texel::top_stamps_in(fm.geo, px, off));
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
        over_cols.resize(nx * nz, None);
        let i = x as usize * nz + z as usize;
        over_cols[i] = Some(over_cols[i].map_or(y, |t| t.max(y)));
        heights[i].get_or_insert(y);
    }
    let over_top = |i: usize| if over_cols.is_empty() { None } else { over_cols[i] };

    let (h_min, h_max) = heights
        .iter()
        .flatten()
        .fold((i32::MAX, i32::MIN), |(lo, hi), &h| (lo.min(h), hi.max(h)));
    let span = (h_max - h_min).max(1) as f32;

    // Pass 2: shade and paint.
    let mut sub = vec![0u8; px * px];
    for x in 0..nx {
        for z in 0..nz {
            let i = x * nz + z;
            let Some(h) = heights[i] else { continue };
            let north = if z > 0 { heights[i - 1] } else { None }.unwrap_or(h);
            let west = if x > 0 { heights[i - nz] } else { None }.unwrap_or(h);
            let relief = (h - north).clamp(-4, 4) + (h - west).clamp(-4, 4);
            let gradient = 0.9 + 0.2 * (h - h_min) as f32 / span;
            let f = (1.0 + 0.07 * relief as f32) * gradient;
            let (x0, y0) = ((x + 1) * px, (z + 1) * px);
            // One sub-pixel's ray down the column, from the highest cell a
            // model reaches into when that lies above the blocks.
            let column = |k: usize, light: Option<(&Lit, &FaceLight, f32, f32)>, bg: Rgba| {
                let (cx, cz) = (x as i32, z as i32);
                match over_top(i) {
                    Some(t) => texel_column::<true>(g, &stamps, &layers, cx, cz, h, t.max(h), k, px, light, bg),
                    None => texel_column::<false>(g, &stamps, &layers, cx, cz, h, h, k, px, light, bg),
                }
            };

            if let Some(l) = lit {
                // The surface block's top face is lit from the cell above it.
                let face = l.face([x as i32, h + 1, z as i32], 1);
                for k in 0..px * px {
                    let (fu, fv) = (((k % px) as f32 + 0.5) / px as f32, ((k / px) as f32 + 0.5) / px as f32);
                    let light = Some((l, &face, fu, fv));
                    let (c, top) = column(k, light, bg);
                    let t = LOW_SURFACE + (1.0 - LOW_SURFACE) * top / 16.0;
                    fill_square(&mut pm, x0 + k % px, y0 + k / px, 1, shade(shade(c, f), t));
                }
                continue;
            }
            if stamps[tops[i] as usize].is_some() || over_top(i).is_some() {
                for k in 0..px * px {
                    let (c, top) = column(k, None, BACKGROUND);
                    let t = LOW_SURFACE + (1.0 - LOW_SURFACE) * top / 16.0;
                    fill_square(&mut pm, x0 + k % px, y0 + k / px, 1, shade(shade(c, f), t));
                }
                continue;
            }
            let c = shade(colors[i], f);
            if surface_heights(g, tops[i], px, &mut sub) {
                for (k, &top) in sub.iter().enumerate() {
                    let t = LOW_SURFACE + (1.0 - LOW_SURFACE) * top as f32 / 16.0;
                    fill_square(&mut pm, x0 + k % px, y0 + k / px, 1, shade(c, t));
                }
            } else {
                fill_square(&mut pm, x0, y0, px, c);
            }
        }
    }
    Ok(pm)
}

/// Colour seen looking down sub-pixel `k` of column `(x, z)` from cell
/// `start`, its surface block being at `h` (`start` lies above it when a
/// model reaches over the surface): textured blocks contribute the texel of
/// their highest upward face under the pixel (cut-outs fall through), others
/// their flat colour. Also the height in sixteenths, within the surface
/// cell, of the first face hit (0 when it lies in a block below, 16 above). With `light`, the first
/// sample takes the surface face's light at the pixel's `(u, v)` and every
/// later one its own cell's. `layers` holds the merged faces of the cells
/// models reach into, read only when the column has any (`OVER`, a constant
/// so other columns run without the lookups): such a cell draws its own
/// untextured block first (water filling the cell, say), then those faces.
#[allow(clippy::too_many_arguments)]
fn texel_column<const OVER: bool>(
    g: &VoxelGrid,
    stamps: &[Option<TopStamps>],
    layers: &HashMap<[i32; 3], TopStamps>,
    x: i32,
    z: i32,
    h: i32,
    start: i32,
    k: usize,
    px: usize,
    light: Option<(&Lit, &FaceLight, f32, f32)>,
    bg: Rgba,
) -> (Rgba, f32) {
    let mut acc = RayAcc::default();
    let mut top: Option<f32> = None;
    let lit_sample = |c: Rgba, y: i32, first: bool| match light {
        Some((l, face, fu, fv)) => tint(c, if first { face.at(fu, fv) } else { l.cell([x, y, z]) }),
        None => c,
    };
    'column: for y in (0..=start).rev() {
        let m = g.get(x, y, z);
        let merged = if OVER { layers.get(&[x, y, z]) } else { None };
        if m == 0 && merged.is_none() {
            continue;
        }
        let base = (y - h) as f32 * 16.0;
        let st = match merged {
            Some(faces) => {
                if m != 0 && stamps[m as usize].is_none() {
                    let fx = 16.0 * ((k % px) as f32 + 0.5) / px as f32;
                    let fz = 16.0 * ((k / px) as f32 + 0.5) / px as f32;
                    if let Some(face_h) = top_at(g.boxes(m), fx, fz) {
                        let c = lit_sample(g.color(m), y, top.is_none());
                        top.get_or_insert((base + face_h as f32).clamp(0.0, 16.0));
                        if acc.push(c) {
                            break 'column;
                        }
                    }
                }
                Some(faces)
            }
            None => stamps[m as usize].as_ref(),
        };
        if let Some(st) = st {
            for (face_h, raster) in st {
                let c = raster[k];
                if c[3] == 0 {
                    continue;
                }
                let c = lit_sample(c, y, top.is_none());
                top.get_or_insert((base + face_h).clamp(0.0, 16.0));
                if acc.push(c) {
                    break 'column;
                }
            }
        } else {
            let fx = 16.0 * ((k % px) as f32 + 0.5) / px as f32;
            let fz = 16.0 * ((k / px) as f32 + 0.5) / px as f32;
            let face_h = top_at(g.boxes(m), fx, fz).unwrap_or(16) as f32;
            let c = lit_sample(g.color(m), y, top.is_none());
            top.get_or_insert((base + face_h).clamp(0.0, 16.0));
            if acc.push(c) {
                break 'column;
            }
        }
    }
    (acc.finish(bg), top.unwrap_or(16.0))
}

/// Fill `out` (`px * px`, row-major north to south) with the surface height in
/// sixteenths of material `m`'s shape under each pixel. `false` when the shape
/// is a full cube or leaves part of the footprint uncovered (posts, panes, …),
/// which keep the plain full-footprint colour.
fn surface_heights(g: &VoxelGrid, m: u16, px: usize, out: &mut [u8]) -> bool {
    if g.is_full(m) {
        return false;
    }
    let boxes = g.boxes(m);
    for (k, slot) in out.iter_mut().enumerate() {
        let fx = 16.0 * ((k % px) as f32 + 0.5) / px as f32;
        let fz = 16.0 * ((k / px) as f32 + 0.5) / px as f32;
        match top_at(boxes, fx, fz) {
            Some(t) => *slot = t,
            None => return false,
        }
    }
    true
}
