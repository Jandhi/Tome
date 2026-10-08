//! 2:1 isometric (dimetric) view.
//!
//! The grid is first rotated so the camera always sits at the `+u, +v` corner
//! (u = screen-right axis, v = screen-left axis); then
//! `screen_x = (u - v) * tile_w / 2`, `screen_y = (u + v) * tile_h / 2 - y * block_h`
//! with `tile_h = block_h = tile_w / 2`. Blocks are painted back to front
//! ((u + v) ascending, then y ascending). Only the three camera-facing faces
//! (top, +u, +v) are drawn, and a face is skipped when its neighbour is opaque
//! or the same material (so water and glass don't show internal faces).
//!
//! Each face shape is rasterised once with tiny_skia into a pixel mask, then
//! stamped per block, which keeps 500k-block scenes fast.

use resvg::tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};

use super::grid::{blend, canvas, shade, VoxelGrid};

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
}

/// Pixel offsets of each face relative to the block's anchor (the projected
/// top-back corner).
struct FaceMasks {
    top: Vec<(i32, i32)>,
    right: Vec<(i32, i32)>,
    left: Vec<(i32, i32)>,
}

fn face_masks(tw: i32) -> anyhow::Result<FaceMasks> {
    let (hw, th, bh) = (tw as f32 / 2.0, tw as f32 / 2.0, tw as f32 / 2.0);
    let w = tw as f32;
    let polys: [[(f32, f32); 4]; 3] = [
        [(hw, 0.0), (w, th / 2.0), (hw, th), (0.0, th / 2.0)],
        [(w, th / 2.0), (hw, th), (hw, th + bh), (w, th / 2.0 + bh)],
        [(0.0, th / 2.0), (hw, th), (hw, th + bh), (0.0, th / 2.0 + bh)],
    ];
    let mut out: Vec<Vec<(i32, i32)>> = Vec::new();
    for poly in polys {
        let mut pm = Pixmap::new(tw as u32, (th + bh) as u32).ok_or_else(|| anyhow::anyhow!("bad tile size"))?;
        let mut pb = PathBuilder::new();
        pb.move_to(poly[0].0, poly[0].1);
        for &(x, y) in &poly[1..] {
            pb.line_to(x, y);
        }
        pb.close();
        let path = pb.finish().ok_or_else(|| anyhow::anyhow!("bad face path"))?;
        let mut paint = Paint::default();
        paint.set_color_rgba8(255, 255, 255, 255);
        paint.anti_alias = false;
        pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        let mut mask = Vec::new();
        for (i, p) in pm.pixels().iter().enumerate() {
            if p.alpha() > 0 {
                mask.push((i as i32 % tw - tw / 2, i as i32 / tw));
            }
        }
        out.push(mask);
    }
    let left = out.pop().unwrap_or_default();
    let right = out.pop().unwrap_or_default();
    let top = out.pop().unwrap_or_default();
    Ok(FaceMasks { top, right, left })
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
    let masks = face_masks(tw)?;
    let (w, h) = image_size(g, cam, tile_w);
    let mut pm = canvas(w, h)?;
    let (iw, ih) = (w as i32, h as i32);
    let data = pm.data_mut();

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

    let mut stamp = |mask: &[(i32, i32)], ax: i32, ay: i32, c: [u8; 4]| {
        for &(dx, dy) in mask {
            let (px, py) = (ax + dx, ay + dy);
            if px >= 0 && py >= 0 && px < iw && py < ih {
                blend(data, ((py * iw + px) * 4) as usize, c);
            }
        }
    };

    for s in 0..(nu + nv - 1) {
        for y in 0..g.ny {
            for u in (s - (nv - 1)).max(0)..=s.min(nu - 1) {
                let v = s - u;
                let m = at(u, y, v);
                if m == 0 {
                    continue;
                }
                let hides = |n: u16| n != 0 && (n == m || g.is_opaque(n));
                let (top, right, left) = (!hides(at(u, y + 1, v)), !hides(at(u + 1, y, v)), !hides(at(u, y, v + 1)));
                if !(top || right || left) {
                    continue;
                }
                let c = g.color(m);
                let ax = (u - v) * tw / 2 + off_x;
                let ay = (u + v) * th / 2 - y * bh + off_y;
                if top {
                    stamp(&masks.top, ax, ay, column_alpha(c, &at, u, y, v, m));
                }
                if right {
                    stamp(&masks.right, ax, ay, shade(c, 0.8));
                }
                if left {
                    stamp(&masks.left, ax, ay, shade(c, 0.64));
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
