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

use resvg::tiny_skia::Pixmap;

use super::grid::{canvas, fill_square, shade, RayAcc, VoxelGrid, BACKGROUND};
use super::shape::top_at;

/// Brightness at the bottom of a cell relative to its top.
const LOW_SURFACE: f32 = 0.8;

/// Render the grid top-down at `px` pixels per block with a 1-block margin.
/// Image size: `(nx + 2) * px` × `(nz + 2) * px`.
pub fn render_topdown(g: &VoxelGrid, px: u32) -> anyhow::Result<Pixmap> {
    let px = px.max(1) as usize;
    let (nx, nz) = (g.nx as usize, g.nz as usize);
    let mut pm = canvas((nx + 2) * px, (nz + 2) * px)?;

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
            let c = shade(colors[i], f);
            let (x0, y0) = ((x + 1) * px, (z + 1) * px);

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
