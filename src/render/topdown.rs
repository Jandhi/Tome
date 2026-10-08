//! Orthographic top-down view: north (-Z) up, east (+X) right.
//!
//! Each column shows its topmost block, composited through translucent blocks
//! (water gets bluer with depth, glass shows what's under it). Hill shading
//! compares the column's surface height with its north and west neighbours
//! (light from the north-west), and a gentle overall height gradient keeps
//! flat plateaus at different levels distinguishable.

use resvg::tiny_skia::Pixmap;

use super::grid::{canvas, fill_square, shade, RayAcc, VoxelGrid, BACKGROUND};

/// Render the grid top-down at `px` pixels per block with a 1-block margin.
/// Image size: `(nx + 2) * px` × `(nz + 2) * px`.
pub fn render_topdown(g: &VoxelGrid, px: u32) -> anyhow::Result<Pixmap> {
    let px = px.max(1) as usize;
    let (nx, nz) = (g.nx as usize, g.nz as usize);
    let mut pm = canvas((nx + 2) * px, (nz + 2) * px)?;

    // Pass 1: surface height and composited colour per column.
    let mut heights: Vec<Option<i32>> = vec![None; nx * nz];
    let mut colors = vec![BACKGROUND; nx * nz];
    for x in 0..g.nx {
        for z in 0..g.nz {
            let mut acc = RayAcc::default();
            let mut top = None;
            for y in (0..g.ny).rev() {
                let m = g.get(x, y, z);
                if m == 0 {
                    continue;
                }
                top.get_or_insert(y);
                if acc.push(g.color(m)) {
                    break;
                }
            }
            let i = x as usize * nz + z as usize;
            heights[i] = top;
            colors[i] = acc.finish(BACKGROUND);
        }
    }

    let (h_min, h_max) = heights
        .iter()
        .flatten()
        .fold((i32::MAX, i32::MIN), |(lo, hi), &h| (lo.min(h), hi.max(h)));
    let span = (h_max - h_min).max(1) as f32;

    // Pass 2: shade and paint.
    for x in 0..nx {
        for z in 0..nz {
            let i = x * nz + z;
            let Some(h) = heights[i] else { continue };
            let north = if z > 0 { heights[i - 1] } else { None }.unwrap_or(h);
            let west = if x > 0 { heights[i - nz] } else { None }.unwrap_or(h);
            let relief = (h - north).clamp(-4, 4) + (h - west).clamp(-4, 4);
            let gradient = 0.9 + 0.2 * (h - h_min) as f32 / span;
            let f = (1.0 + 0.07 * relief as f32) * gradient;
            fill_square(&mut pm, (x + 1) * px, (z + 1) * px, px, shade(colors[i], f));
        }
    }
    Ok(pm)
}
