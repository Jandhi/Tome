//! 2:1 isometric (dimetric) view.
//!
//! The grid is first rotated so the camera always sits at the `+u, +v` corner
//! (u = screen-right axis, v = screen-left axis); then
//! `screen_x = (u - v) * tile_w / 2`, `screen_y = (u + v) * tile_h / 2 - y * block_h`
//! with `tile_h = block_h = tile_w / 2`. Blocks are painted back to front
//! ((u + v) ascending, then y ascending). Only the three camera-facing faces
//! (top, +u, +v) of each of a block's shape boxes are drawn, and a face on the
//! cell boundary is skipped when its neighbour is a full opaque cube or the
//! same translucent material (so water and glass don't show internal faces).
//! Partial shapes (slabs, stairs, posts, panes) never hide their neighbours.
//!
//! Each shape's faces are rasterised once per camera with tiny_skia into a
//! tagged pixel mask, then stamped per block, which keeps 500k-block scenes
//! fast.

use resvg::tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};

use super::grid::{blend, canvas, shade, VoxelGrid};
use super::shape::{Box16, Shape};

/// Which corner the camera looks from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // SouthWest / NorthEast are offered to callers; render_all uses SE + NW.
pub enum IsoCamera {
    SouthEast,
    SouthWest,
    NorthEast,
    NorthWest,
}

impl IsoCamera {
    /// Short tag used in file names: `se`, `sw`, `ne`, `nw`.
    pub fn tag(self) -> &'static str {
        match self {
            IsoCamera::SouthEast => "se",
            IsoCamera::SouthWest => "sw",
            IsoCamera::NorthEast => "ne",
            IsoCamera::NorthWest => "nw",
        }
    }

    /// Size of the rotated grid `(nu, nv)` and the map `(u, v) -> (x, z)`.
    fn frame(self, g: &VoxelGrid) -> (i32, i32, impl Fn(i32, i32) -> (i32, i32)) {
        let (nx, nz) = (g.nx, g.nz);
        let (nu, nv) = match self {
            IsoCamera::SouthEast | IsoCamera::NorthWest => (nx, nz),
            IsoCamera::NorthEast | IsoCamera::SouthWest => (nz, nx),
        };
        let map = move |u: i32, v: i32| match self {
            IsoCamera::SouthEast => (u, v),
            IsoCamera::NorthWest => (nx - 1 - u, nz - 1 - v),
            IsoCamera::NorthEast => (v, nz - 1 - u),
            IsoCamera::SouthWest => (nx - 1 - v, u),
        };
        (nu, nv, map)
    }

    /// Rotate a world-oriented box into this camera's frame: `(u0, u1, v0, v1)`
    /// in sixteenths, consistent with [`frame`](Self::frame).
    fn box_uv(self, b: &Box16) -> (u8, u8, u8, u8) {
        let (x0, x1, z0, z1) = (b[0], b[3], b[2], b[5]);
        match self {
            IsoCamera::SouthEast => (x0, x1, z0, z1),
            IsoCamera::NorthWest => (16 - x1, 16 - x0, 16 - z1, 16 - z0),
            IsoCamera::NorthEast => (16 - z1, 16 - z0, x0, x1),
            IsoCamera::SouthWest => (z0, z1, 16 - x1, 16 - x0),
        }
    }
}

/// Face of a box as seen by the camera (index into the per-block colours).
const TOP: u8 = 0;
const RIGHT: u8 = 1; // +u
const LEFT: u8 = 2; // +v
/// Flag on a mask pixel: the face lies on the cell boundary, so a neighbour
/// can hide it.
const BOUNDARY: u8 = 4;

/// Pixel offsets of a shape's visible faces relative to the block's anchor
/// (the projected top-back corner of the cell), each tagged with its face
/// (`TOP` / `RIGHT` / `LEFT`, plus `BOUNDARY`). Boxes are painted back to front
/// within the cell, so every pixel belongs to the face visible there.
type ShapeMask = Vec<(i32, i32, u8)>;

/// A box in the camera frame, sixteenths: `(u0, u1, v0, v1, y0, y1)`.
type UvBox = (u8, u8, u8, u8, u8, u8);

/// Back-to-front order for the non-overlapping boxes of one cell. A box lying
/// wholly on the low side of another along u, v or y can't occlude it (the
/// camera looks from +u, +v, +y), so it is painted first.
fn paint_order(mut boxes: Vec<UvBox>) -> Vec<UvBox> {
    let behind = |a: &UvBox, b: &UvBox| a.1 <= b.0 || a.3 <= b.2 || a.5 <= b.4;
    let must_precede = |a: &UvBox, b: &UvBox| behind(a, b) && !behind(b, a);
    let mut out = Vec::with_capacity(boxes.len());
    while !boxes.is_empty() {
        let next = (0..boxes.len())
            .find(|&i| (0..boxes.len()).all(|j| j == i || !must_precede(&boxes[j], &boxes[i])))
            .unwrap_or(0);
        out.push(boxes.remove(next));
    }
    out
}

fn shape_mask(shape: Shape, cam: IsoCamera, tw: i32) -> anyhow::Result<ShapeMask> {
    let (th, bh) = (tw / 2, tw / 2);
    let (pw, ph) = (tw, th + bh);
    // Cell point (u, v, y) in sixteenths -> local pixmap coords (anchor at x = tw/2).
    let proj = |u: u8, v: u8, y: u8| -> (f32, f32) {
        let (u, v, y) = (u as f32 / 16.0, v as f32 / 16.0, y as f32 / 16.0);
        ((u - v) * tw as f32 / 2.0 + tw as f32 / 2.0, (u + v) * th as f32 / 2.0 + (1.0 - y) * bh as f32)
    };

    let boxes: Vec<UvBox> = shape
        .boxes()
        .iter()
        .map(|b| {
            let (u0, u1, v0, v1) = cam.box_uv(b);
            (u0, u1, v0, v1, b[1], b[4])
        })
        .collect();

    let mut label: Vec<Option<u8>> = vec![None; (pw * ph) as usize];
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = false;
    for (u0, u1, v0, v1, y0, y1) in paint_order(boxes) {
        let faces: [([(u8, u8, u8); 4], u8, bool); 3] = [
            ([(u0, v0, y1), (u1, v0, y1), (u1, v1, y1), (u0, v1, y1)], TOP, y1 == 16),
            ([(u1, v0, y1), (u1, v1, y1), (u1, v1, y0), (u1, v0, y0)], RIGHT, u1 == 16),
            ([(u0, v1, y1), (u1, v1, y1), (u1, v1, y0), (u0, v1, y0)], LEFT, v1 == 16),
        ];
        for (quad, face, boundary) in faces {
            let mut pb = PathBuilder::new();
            let (x, y) = proj(quad[0].0, quad[0].1, quad[0].2);
            pb.move_to(x, y);
            for &(u, v, yy) in &quad[1..] {
                let (x, y) = proj(u, v, yy);
                pb.line_to(x, y);
            }
            pb.close();
            // Degenerate (zero-area) faces have no path: nothing to draw.
            let Some(path) = pb.finish() else { continue };
            let mut pm = Pixmap::new(pw as u32, ph as u32).ok_or_else(|| anyhow::anyhow!("bad tile size"))?;
            pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
            let tag = face | if boundary { BOUNDARY } else { 0 };
            for (i, p) in pm.pixels().iter().enumerate() {
                if p.alpha() > 0 {
                    label[i] = Some(tag);
                }
            }
        }
    }
    Ok(label
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.map(|t| (i as i32 % pw - tw / 2, i as i32 / pw, t)))
        .collect())
}

/// Snap a requested tile width to a multiple of 4 (so every projected anchor
/// lands on a whole pixel), minimum 4.
pub fn tile_width(tile_w: u32) -> i32 {
    ((tile_w.max(4) / 4) * 4) as i32
}

/// Image size for a grid at this tile width.
pub fn image_size(g: &VoxelGrid, cam: IsoCamera, tile_w: u32) -> (usize, usize) {
    let tw = tile_width(tile_w);
    let (nu, nv, _) = cam.frame(g);
    let margin = tw;
    let w = (nu + nv) * tw / 2 + 2 * margin;
    let h = (nu + nv) * tw / 4 + g.ny * tw / 2 + 2 * margin;
    (w as usize, h as usize)
}

pub fn render_iso(g: &VoxelGrid, cam: IsoCamera, tile_w: u32) -> anyhow::Result<Pixmap> {
    let tw = tile_width(tile_w);
    let (th, bh) = (tw / 2, tw / 2);
    let (w, h) = image_size(g, cam, tile_w);
    let mut pm = canvas(w, h)?;
    let (iw, ih) = (w as i32, h as i32);
    let data = pm.data_mut();

    // One mask per distinct shape, shared by every material with that shape.
    let mut shapes: Vec<Shape> = Vec::new();
    let mut masks: Vec<ShapeMask> = Vec::new();
    let mut mat_mask: Vec<usize> = Vec::with_capacity(g.material_count());
    for m in 0..g.material_count() {
        let shape = g.shape(m as u16);
        let i = match shapes.iter().position(|&s| s == shape) {
            Some(i) => i,
            None => {
                shapes.push(shape);
                masks.push(shape_mask(shape, cam, tw)?);
                shapes.len() - 1
            }
        };
        mat_mask.push(i);
    }

    let (nu, nv, map) = cam.frame(g);
    let at = |u: i32, y: i32, v: i32| {
        if u < 0 || v < 0 || u >= nu || v >= nv {
            return 0;
        }
        let (x, z) = map(u, v);
        g.get(x, y, z)
    };
    let margin = tw;
    let off_x = nv * tw / 2 + margin;
    let off_y = (g.ny - 1) * bh + margin;

    for s in 0..(nu + nv - 1) {
        for y in 0..g.ny {
            for u in (s - (nv - 1)).max(0)..=s.min(nu - 1) {
                let v = s - u;
                let m = at(u, y, v);
                if m == 0 {
                    continue;
                }
                // hidden[face]: that boundary face is covered by its neighbour.
                let hidden = [g.hides(at(u, y + 1, v), m), g.hides(at(u + 1, y, v), m), g.hides(at(u, y, v + 1), m)];
                // Every view ray into the cell leaves through its top, +u or +v
                // side, so with all three covered nothing of this block shows.
                if hidden.iter().all(|&h| h) {
                    continue;
                }
                let c = g.color(m);
                let colors = [column_alpha(c, &at, u, y, v, m), shade(c, 0.8), shade(c, 0.64)];
                let ax = (u - v) * tw / 2 + off_x;
                let ay = (u + v) * th / 2 - y * bh + off_y;
                for &(dx, dy, tag) in &masks[mat_mask[m as usize]] {
                    let face = (tag & 3) as usize;
                    if tag & BOUNDARY != 0 && hidden[face] {
                        continue;
                    }
                    let (px, py) = (ax + dx, ay + dy);
                    if px >= 0 && py >= 0 && px < iw && py < ih {
                        blend(data, ((py * iw + px) * 4) as usize, colors[face]);
                    }
                }
            }
        }
    }
    Ok(pm)
}

/// A translucent top face only gets drawn once per column of the same
/// material (inner faces are culled), so give it the opacity of the whole
/// stack below it: deep water reads bluer than a puddle.
fn column_alpha(c: [u8; 4], at: &impl Fn(i32, i32, i32) -> u16, u: i32, y: i32, v: i32, m: u16) -> [u8; 4] {
    if c[3] == 255 {
        return c;
    }
    let depth = (1..6).take_while(|d| at(u, y - d, v) == m).count() as i32 + 1;
    let clear = (1.0 - c[3] as f32 / 255.0).powi(depth);
    [c[0], c[1], c[2], ((1.0 - clear) * 255.0).round() as u8]
}
