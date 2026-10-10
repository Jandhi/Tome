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
//! fast. A material with a block model gets its own mask built from the
//! model's boxes instead, with the texel under each pixel baked in: cut-out
//! texels are left out of the mask (so what is behind them shows), and a
//! model that is not a fully opaque cube never hides a neighbour's face.
//!
//! A model reaching past its cell (a wall banner's flag, a pot's neck) is
//! clipped to each cell it lies in and every piece is painted with that
//! cell, so it takes its place in the back-to-front order: a cell a model
//! reaches into paints the overhanging pieces first, then its own block.
//!
//! With a [`Lit`] the painted colour of every pixel is multiplied by the
//! light of its face: the face's outside cell is the neighbour above / +u /
//! +v for a boundary face and the block's own cell for an inner one, and
//! each mask pixel carries its face-local position so smooth light
//! interpolates across the face. Without one the arithmetic is untouched.

use resvg::tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};

use super::assets::{BlockGeometry, Element, Face6, V3};
use super::grid::{blend, canvas, shade, Rgba, VoxelGrid};
use super::lit::{axis_of, face_uv, tint, FaceLight, Lit};
use super::shape::{Box16, Shape};
use super::texel;
use crate::util::HashMap;

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

    /// [`box_uv`](Self::box_uv) for a model box `from..to`.
    fn box_uv_f(self, from: V3, to: V3) -> (f32, f32, f32, f32) {
        let (x0, x1, z0, z1) = (from[0], to[0], from[2], to[2]);
        match self {
            IsoCamera::SouthEast => (x0, x1, z0, z1),
            IsoCamera::NorthWest => (16.0 - x1, 16.0 - x0, 16.0 - z1, 16.0 - z0),
            IsoCamera::NorthEast => (16.0 - z1, 16.0 - z0, x0, x1),
            IsoCamera::SouthWest => (z0, z1, 16.0 - x1, 16.0 - x0),
        }
    }

    /// A camera-frame point `(u, v, y)` back in world sixteenths.
    fn world(self, u: f32, v: f32, y: f32) -> V3 {
        match self {
            IsoCamera::SouthEast => [u, y, v],
            IsoCamera::NorthWest => [16.0 - u, y, 16.0 - v],
            IsoCamera::NorthEast => [v, y, 16.0 - u],
            IsoCamera::SouthWest => [16.0 - v, y, u],
        }
    }

    /// The world faces the camera sees as a box's right (+u) and left (+v) sides.
    fn side_faces(self) -> (Face6, Face6) {
        match self {
            IsoCamera::SouthEast => (Face6::East, Face6::South),
            IsoCamera::NorthWest => (Face6::West, Face6::North),
            IsoCamera::NorthEast => (Face6::North, Face6::East),
            IsoCamera::SouthWest => (Face6::South, Face6::West),
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
/// Brightness of the top, right and left faces.
const FACE_SHADE: [f32; 3] = [1.0, 0.8, 0.64];
/// Tolerance for a model box edge lying on the cell boundary, and the margin
/// kept from a face's far edge when sampling (so the texel never wraps).
const EDGE: f32 = 1e-3;

/// Pixel offsets of a shape's visible faces relative to the block's anchor
/// (the projected top-back corner of the cell), each tagged with its face
/// (`TOP` / `RIGHT` / `LEFT`, plus `BOUNDARY`) and the face-local `(u, v)`
/// of the pixel's centre for lighting (see [`face_uv`]). Boxes are painted
/// back to front within the cell, so every pixel belongs to the face
/// visible there.
type ShapeMask = Vec<(i32, i32, u8, f32, f32)>;

/// A [`ShapeMask`] for a textured material, each pixel carrying the shaded
/// texel visible there.
type TexMask = Vec<(i32, i32, u8, Rgba, f32, f32)>;

/// A box in the camera frame, sixteenths: `(u0, u1, v0, v1, y0, y1)`.
type UvBox = (u8, u8, u8, u8, u8, u8);
type UvBoxF = (f32, f32, f32, f32, f32, f32);

/// Back-to-front order for the non-overlapping boxes of one cell. A box lying
/// wholly on the low side of another along u, v or y can't occlude it (the
/// camera looks from +u, +v, +y), so it is painted first; `behind(a, b)`
/// tells whether `a` lies on the low side of `b`. Boxes nothing separates
/// (coincident overlays) keep their order.
fn paint_order<B>(mut boxes: Vec<B>, behind: impl Fn(&B, &B) -> bool) -> Vec<B> {
    let must_precede = |a: &B, b: &B| behind(a, b) && !behind(b, a);
    let mut out = Vec::with_capacity(boxes.len());
    while !boxes.is_empty() {
        let next = (0..boxes.len())
            .find(|&i| (0..boxes.len()).all(|j| j == i || !must_precede(&boxes[j], &boxes[i])))
            .unwrap_or(0);
        out.push(boxes.remove(next));
    }
    out
}

/// Cell point `(u, v, y)` in sixteenths -> mask pixmap coords (anchor at x = tw/2).
fn proj(tw: i32, u: f32, v: f32, y: f32) -> (f32, f32) {
    let (th, bh) = (tw / 2, tw / 2);
    let (u, v, y) = (u / 16.0, v / 16.0, y / 16.0);
    ((u - v) * tw as f32 / 2.0 + tw as f32 / 2.0, (u + v) * th as f32 / 2.0 + (1.0 - y) * bh as f32)
}

/// Invert [`proj`] on a face's own plane: mask pixmap position `(x, y)` ->
/// cell point `(u, v, y)` in sixteenths on the `face` of box `b` (`TOP` lies
/// at `b`'s top, `RIGHT` at its +u side, `LEFT` at its +v side).
fn face_point(face: u8, b: UvBoxF, tw: i32, x: f32, y: f32) -> (f32, f32, f32) {
    let (th, bh) = (tw / 2, tw / 2);
    // Pixels per sixteenth along u - v (screen x) and u + v (screen y).
    let (ppx, ppy) = (tw as f32 / 32.0, th as f32 / 32.0);
    let (_, u1, _, v1, _, y1) = b;
    let d = (x - tw as f32 / 2.0) / ppx; // u - v
    match face {
        TOP => {
            let s = (y - (16.0 - y1) / 16.0 * bh as f32) / ppy; // u + v
            ((d + s) / 2.0, (s - d) / 2.0, y1)
        }
        RIGHT => {
            let v = u1 - d;
            (u1, v, 16.0 - (y - (u1 + v) * ppy) * 16.0 / bh as f32)
        }
        _ => {
            let u = d + v1;
            (u, v1, 16.0 - (y - (u + v1) * ppy) * 16.0 / bh as f32)
        }
    }
}

/// Face-local light coordinates of the cell point `(u, v, y)` (sixteenths,
/// clamped into box `b`) on the world face `world_face`.
fn light_uv(cam: IsoCamera, world_face: Face6, b: UvBoxF, u: f32, v: f32, y: f32) -> (f32, f32) {
    let (u0, u1, v0, v1, y0, y1) = b;
    let clamp = |t: f32, lo: f32, hi: f32| t.clamp(lo, (hi - EDGE).max(lo));
    face_uv(axis_of(world_face), cam.world(clamp(u, u0, u1), clamp(v, v0, v1), clamp(y, y0, y1)))
}

/// Rasterise a projected face quad into a `pw` x `ph` coverage pixmap;
/// `None` for a degenerate (zero-area) face.
fn quad_coverage(quad: [(f32, f32); 4], pw: i32, ph: i32) -> anyhow::Result<Option<Pixmap>> {
    let mut pb = PathBuilder::new();
    pb.move_to(quad[0].0, quad[0].1);
    for &(x, y) in &quad[1..] {
        pb.line_to(x, y);
    }
    pb.close();
    let Some(path) = pb.finish() else { return Ok(None) };
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = false;
    let mut pm = Pixmap::new(pw as u32, ph as u32).ok_or_else(|| anyhow::anyhow!("bad tile size"))?;
    pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    Ok(Some(pm))
}

fn shape_mask(shape: Shape, cam: IsoCamera, tw: i32) -> anyhow::Result<ShapeMask> {
    let (th, bh) = (tw / 2, tw / 2);
    let (pw, ph) = (tw, th + bh);
    let proj = |u: u8, v: u8, y: u8| proj(tw, u as f32, v as f32, y as f32);
    let (right_face, left_face) = cam.side_faces();

    let boxes: Vec<UvBox> = shape
        .boxes()
        .iter()
        .map(|b| {
            let (u0, u1, v0, v1) = cam.box_uv(b);
            (u0, u1, v0, v1, b[1], b[4])
        })
        .collect();
    let behind = |a: &UvBox, b: &UvBox| a.1 <= b.0 || a.3 <= b.2 || a.5 <= b.4;

    let mut label: Vec<Option<(u8, f32, f32)>> = vec![None; (pw * ph) as usize];
    for (u0, u1, v0, v1, y0, y1) in paint_order(boxes, behind) {
        let faces: [([(u8, u8, u8); 4], u8, Face6, bool); 3] = [
            ([(u0, v0, y1), (u1, v0, y1), (u1, v1, y1), (u0, v1, y1)], TOP, Face6::Up, y1 == 16),
            ([(u1, v0, y1), (u1, v1, y1), (u1, v1, y0), (u1, v0, y0)], RIGHT, right_face, u1 == 16),
            ([(u0, v1, y1), (u1, v1, y1), (u1, v1, y0), (u0, v1, y0)], LEFT, left_face, v1 == 16),
        ];
        let bf: UvBoxF = (u0 as f32, u1 as f32, v0 as f32, v1 as f32, y0 as f32, y1 as f32);
        for (quad, face, world_face, boundary) in faces {
            // Degenerate (zero-area) faces have no path: nothing to draw.
            let Some(pm) = quad_coverage(quad.map(|(u, v, y)| proj(u, v, y)), pw, ph)? else { continue };
            let tag = face | if boundary { BOUNDARY } else { 0 };
            for (i, p) in pm.pixels().iter().enumerate() {
                if p.alpha() > 0 {
                    let (px, py) = ((i as i32 % pw) as f32 + 0.5, (i as i32 / pw) as f32 + 0.5);
                    let (u, v, yy) = face_point(face, bf, tw, px, py);
                    let (lu, lv) = light_uv(cam, world_face, bf, u, v, yy);
                    label[i] = Some((tag, lu, lv));
                }
            }
        }
    }
    Ok(label
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.map(|(t, lu, lv)| (i as i32 % pw - tw / 2, i as i32 / pw, t, lu, lv)))
        .collect())
}

/// Mask of a textured block model: the pixel coverage of its boxes' faces is
/// rasterised exactly like [`shape_mask`], and each covered pixel is coloured
/// by box-filtering the face's texels under it (sub-samples clamped into the
/// face) times the face's shade. A face is painted over what is already in
/// the mask, so a cut-out texel leaves the box behind visible and a
/// translucent one blends over it.
///
/// Only the part of the model inside one cell is drawn: `off` is the
/// model's own cell minus that cell (in cells), `[0; 3]` for its own. Boxes
/// are clipped to the cell, and a face whose plane lies outside it is left
/// to the cell it lies in ([`texel::in_cell`]).
fn tex_mask(geo: &BlockGeometry, cam: IsoCamera, tw: i32, off: [i32; 3]) -> anyhow::Result<TexMask> {
    let (th, bh) = (tw / 2, tw / 2);
    let (pw, ph) = (tw, th + bh);
    // Sub-samples per pixel axis: a texel each at tw = 8 (top faces 4 px tall).
    let ss = ((32 + tw - 1) / tw).clamp(1, 4);
    let n = (ss * ss) as f32;
    let (right_face, left_face) = cam.side_faces();

    // Each element's box in the drawn cell's sixteenths, clipped to the cell,
    // and which of its camera-facing faces (top, +u, +v) lie in the cell.
    let shift = off.map(|o| 16.0 * o as f32);
    let max_side = |f: Face6| matches!(f, Face6::Up | Face6::East | Face6::South);
    let boxes: Vec<(UvBoxF, [bool; 3], &Element)> = geo
        .elements
        .iter()
        .filter_map(|e| {
            let (mut from, mut to) = (e.from, e.to);
            for k in 0..3 {
                let (lo, hi) = ((e.from[k] + shift[k]).clamp(0.0, 16.0), (e.to[k] + shift[k]).clamp(0.0, 16.0));
                // Clipped away, or cut down to a sliver on the boundary.
                if lo > hi || (lo == hi && e.from[k] < e.to[k]) {
                    return None;
                }
                (from[k], to[k]) = (lo, hi);
            }
            let keep = [Face6::Up, right_face, left_face].map(|f| {
                let k = axis_of(f);
                let plane = if max_side(f) { e.to[k] } else { e.from[k] };
                texel::in_cell(plane + shift[k], off[k])
            });
            let (u0, u1, v0, v1) = cam.box_uv_f(from, to);
            Some(((u0, u1, v0, v1, from[1], to[1]), keep, e))
        })
        .collect();
    type Clipped<'e> = (UvBoxF, [bool; 3], &'e Element);
    let behind = |a: &Clipped, b: &Clipped| a.0 .1 <= b.0 .0 || a.0 .3 <= b.0 .2 || a.0 .5 <= b.0 .4;

    let mut label: Vec<Option<(u8, Rgba, f32, f32)>> = vec![None; (pw * ph) as usize];
    for (b, keep, e) in paint_order(boxes, behind) {
        let (u0, u1, v0, v1, y0, y1) = b;
        let faces: [([(f32, f32, f32); 4], u8, Face6, bool); 3] = [
            ([(u0, v0, y1), (u1, v0, y1), (u1, v1, y1), (u0, v1, y1)], TOP, Face6::Up, y1 >= 16.0 - EDGE),
            ([(u1, v0, y1), (u1, v1, y1), (u1, v1, y0), (u1, v0, y0)], RIGHT, right_face, u1 >= 16.0 - EDGE),
            ([(u0, v1, y1), (u1, v1, y1), (u1, v1, y0), (u0, v1, y0)], LEFT, left_face, v1 >= 16.0 - EDGE),
        ];
        for (quad, face, world_face, boundary) in faces {
            if !keep[face as usize] {
                continue;
            }
            let Some(ft) = texel::face(e, world_face) else { continue };
            let Some(pm) = quad_coverage(quad.map(|(u, v, y)| proj(tw, u, v, y)), pw, ph)? else { continue };
            let shade_f = if ft.shade { FACE_SHADE[face as usize] } else { 1.0 };
            let tag = face | if boundary { BOUNDARY } else { 0 };
            let clamp = |t: f32, lo: f32, hi: f32| t.clamp(lo, (hi - EDGE).max(lo));
            for (i, p) in pm.pixels().iter().enumerate() {
                if p.alpha() == 0 {
                    continue;
                }
                let (px, py) = ((i as i32 % pw) as f32, (i as i32 / pw) as f32);
                let (mut rgb, mut a_sum) = ([0.0f32; 3], 0.0f32);
                for sj in 0..ss {
                    for si in 0..ss {
                        let x = px + (si as f32 + 0.5) / ss as f32;
                        let y = py + (sj as f32 + 0.5) / ss as f32;
                        // Invert the projection on the face's own plane.
                        let (u, v, yy) = face_point(face, b, tw, x, y);
                        // Back into the model's own cell to read the texel.
                        let p = cam.world(clamp(u, u0, u1), clamp(v, v0, v1), clamp(yy, y0, y1));
                        let c = ft.sample_at([p[0] - shift[0], p[1] - shift[1], p[2] - shift[2]]);
                        let a = c[3] as f32;
                        for k in 0..3 {
                            rgb[k] += c[k] as f32 * a;
                        }
                        a_sum += a;
                    }
                }
                let c = texel::average(rgb, a_sum, n);
                if c[3] == 0 {
                    continue; // cut-out: what is behind stays
                }
                let c = shade(c, shade_f);
                let c = match label[i] {
                    Some((_, below, _, _)) if c[3] < 255 => over(c, below),
                    _ => c,
                };
                let (u, v, yy) = face_point(face, b, tw, px + 0.5, py + 0.5);
                let (lu, lv) = light_uv(cam, world_face, b, u, v, yy);
                label[i] = Some((tag, c, lu, lv));
            }
        }
    }
    Ok(label
        .iter()
        .enumerate()
        .filter_map(|(i, l)| l.map(|(t, c, lu, lv)| (i as i32 % pw - tw / 2, i as i32 / pw, t, c, lu, lv)))
        .collect())
}

/// Straight-alpha `c` composited over straight-alpha `below`.
fn over(c: Rgba, below: Rgba) -> Rgba {
    let a = c[3] as f32 / 255.0;
    let b = below[3] as f32 / 255.0 * (1.0 - a);
    let out_a = a + b;
    if out_a <= 0.0 {
        return [0, 0, 0, 0];
    }
    let ch = |k: usize| ((c[k] as f32 * a + below[k] as f32 * b) / out_a).round().clamp(0.0, 255.0) as u8;
    [ch(0), ch(1), ch(2), (out_a * 255.0).round() as u8]
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

/// Render the view from `cam`, unlit.
pub fn render_iso(g: &VoxelGrid, cam: IsoCamera, tile_w: u32) -> anyhow::Result<Pixmap> {
    render_iso_with(g, cam, tile_w, None)
}

/// Render the view from `cam`, lit by `lit` when given (see the module doc).
pub fn render_iso_with(g: &VoxelGrid, cam: IsoCamera, tile_w: u32, lit: Option<&Lit>) -> anyhow::Result<Pixmap> {
    let tw = tile_width(tile_w);
    let (th, bh) = (tw / 2, tw / 2);
    let (w, h) = image_size(g, cam, tile_w);
    let mut pm = canvas(w, h)?;
    if let Some(l) = lit {
        let [r, g, b, a] = l.background();
        pm.fill(resvg::tiny_skia::Color::from_rgba8(r, g, b, a));
    }
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
    // Textured materials get a mask of their own; those that are not solid
    // cubes (cut-outs, panes, part blocks) never hide a neighbour's face.
    let models = texel::flat_models(g);
    let mut tex_masks: Vec<Option<TexMask>> = Vec::with_capacity(models.len());
    let mut cutout: Vec<bool> = Vec::with_capacity(models.len());
    for (m, model) in models.iter().enumerate() {
        tex_masks.push(model.as_ref().map(|fm| tex_mask(fm.geo, cam, tw, [0; 3])).transpose()?);
        cutout.push(model.as_ref().is_some_and(|fm| !fm.solid && g.is_opaque(m as u16)));
    }
    let hides = |n: u16, m: u16| g.hides(n, m) && !cutout[n as usize];

    // Pieces of textured models lying in a neighbouring cell: one mask per
    // (material, model's cell minus drawn cell), and which grid columns
    // (x * nz + z) hold a cell such a piece lies in.
    let mut over_masks: HashMap<(u16, [i32; 3]), TexMask> = HashMap::default();
    let mut over_cols: Vec<bool> = Vec::new();
    for ([x, y, z], sources) in g.overhang_cells() {
        for s in sources {
            let sm = g.get(s[0], s[1], s[2]);
            let Some(fm) = &models[sm as usize] else { continue };
            let off = [s[0] - x, s[1] - y, s[2] - z];
            if !over_masks.contains_key(&(sm, off)) {
                over_masks.insert((sm, off), tex_mask(fm.geo, cam, tw, off)?);
            }
            over_cols.resize((g.nx * g.nz) as usize, false);
            over_cols[(x * g.nz + z) as usize] = true;
        }
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

    // Light of one of the cell's faces, from the cell on its outside: the
    // neighbour across a boundary face, the cell itself for an inner face.
    // Computed once per face per cell (`cache` is keyed by face and boundary).
    let (right_face, left_face) = cam.side_faces();
    let face_axes = [1usize, axis_of(right_face), axis_of(left_face)];
    let face_light = |l: &Lit, u: i32, y: i32, v: i32, tag: u8, cache: &mut [Option<FaceLight>; 6]| -> FaceLight {
        let face = (tag & 3) as usize;
        let boundary = tag & BOUNDARY != 0;
        *cache[face + if boundary { 3 } else { 0 }].get_or_insert_with(|| {
            let (du, dy, dv) = match (face as u8, boundary) {
                (_, false) => (0, 0, 0),
                (TOP, true) => (0, 1, 0),
                (RIGHT, true) => (1, 0, 0),
                _ => (0, 0, 1),
            };
            let (x, z) = map(u + du, v + dv);
            l.face([x, y + dy, z], face_axes[face])
        })
    };

    // Paint textured mask `mask` with its anchor at `(ax, ay)` for cell
    // `(u, y, v)`: `hidden[face]` tells which boundary faces its neighbours
    // cover, `depth` how deep a translucent column it tops.
    #[allow(clippy::too_many_arguments)]
    let paint_tex = |data: &mut [u8],
                     mask: &TexMask,
                     hidden: [bool; 3],
                     depth: i32,
                     (ax, ay): (i32, i32),
                     (u, y, v): (i32, i32, i32),
                     cache: &mut [Option<FaceLight>; 6]| {
        for &(dx, dy, tag, c, lu, lv) in mask {
            let face = (tag & 3) as usize;
            if tag & BOUNDARY != 0 && hidden[face] {
                continue;
            }
            let c = if depth > 1 && face == TOP as usize && c[3] < 255 { stacked(c, depth) } else { c };
            let c = match lit {
                Some(l) => tint(c, face_light(l, u, y, v, tag, cache).at(lu, lv)),
                None => c,
            };
            let (px, py) = (ax + dx, ay + dy);
            if px >= 0 && py >= 0 && px < iw && py < ih {
                blend(data, ((py * iw + px) * 4) as usize, c);
            }
        }
    };

    // Per u along the current diagonal: its column holds overhanging pieces
    // (read once per diagonal, and only on diagonals that have any).
    let mut over_row: Vec<bool> = vec![false; if over_cols.is_empty() { 0 } else { nu as usize }];
    for s in 0..(nu + nv - 1) {
        let us = (s - (nv - 1)).max(0)..=s.min(nu - 1);
        let mut diagonal_over = false;
        if !over_row.is_empty() {
            for u in us.clone() {
                let (x, z) = map(u, s - u);
                over_row[u as usize] = over_cols[(x * g.nz + z) as usize];
                diagonal_over |= over_row[u as usize];
            }
        }
        for y in 0..g.ny {
            for u in us.clone() {
                let v = s - u;
                let (x, z) = map(u, v);
                let m = g.get(x, y, z);
                let over = if diagonal_over && over_row[u as usize] { g.overhangs_into(x, y, z) } else { &[][..] };
                if m == 0 && over.is_empty() {
                    continue;
                }
                let ax = (u - v) * tw / 2 + off_x;
                let ay = (u + v) * th / 2 - y * bh + off_y;
                // Per face of this cell (keyed by face and boundary): its light,
                // and in the flat-colour path its finished colour when the light
                // is uniform across it.
                let mut cache: [Option<FaceLight>; 6] = [None; 6];
                let mut ready: [Option<Rgba>; 6] = [None; 6];
                // Pieces of neighbours' models lying in this cell go first, lit
                // by this cell's faces; the cell's own block paints over them.
                for src in over {
                    let sm = g.get(src[0], src[1], src[2]);
                    if let Some(mask) = over_masks.get(&(sm, [src[0] - x, src[1] - y, src[2] - z])) {
                        let hidden =
                            [hides(at(u, y + 1, v), sm), hides(at(u + 1, y, v), sm), hides(at(u, y, v + 1), sm)];
                        paint_tex(data, mask, hidden, 1, (ax, ay), (u, y, v), &mut cache);
                    }
                }
                if m == 0 {
                    continue;
                }
                // hidden[face]: that boundary face is covered by its neighbour.
                let hidden = [hides(at(u, y + 1, v), m), hides(at(u + 1, y, v), m), hides(at(u, y, v + 1), m)];
                // Every view ray into the cell leaves through its top, +u or +v
                // side, so with all three covered nothing of this block shows.
                if hidden.iter().all(|&h| h) {
                    continue;
                }
                if let Some(mask) = &tex_masks[m as usize] {
                    let depth = if g.is_opaque(m) { 1 } else { column_depth(&at, u, y, v, m) };
                    paint_tex(data, mask, hidden, depth, (ax, ay), (u, y, v), &mut cache);
                    continue;
                }
                let c = g.color(m);
                let colors = [column_alpha(c, &at, u, y, v, m), shade(c, 0.8), shade(c, 0.64)];
                for &(dx, dy, tag, lu, lv) in &masks[mat_mask[m as usize]] {
                    let face = (tag & 3) as usize;
                    if tag & BOUNDARY != 0 && hidden[face] {
                        continue;
                    }
                    let c = match lit {
                        None => colors[face],
                        Some(l) => {
                            let key = face + if tag & BOUNDARY != 0 { 3 } else { 0 };
                            match ready[key] {
                                Some(c) => c,
                                None => {
                                    let fl = face_light(l, u, y, v, tag, &mut cache);
                                    let c = tint(colors[face], fl.at(lu, lv));
                                    if fl.uniform {
                                        ready[key] = Some(c);
                                    }
                                    c
                                }
                            }
                        }
                    };
                    let (px, py) = (ax + dx, ay + dy);
                    if px >= 0 && py >= 0 && px < iw && py < ih {
                        blend(data, ((py * iw + px) * 4) as usize, c);
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
    stacked(c, column_depth(at, u, y, v, m))
}

/// Number of blocks of material `m` stacked from `(u, y, v)` downwards (up to six).
fn column_depth(at: &impl Fn(i32, i32, i32) -> u16, u: i32, y: i32, v: i32, m: u16) -> i32 {
    (1..6).take_while(|d| at(u, y - d, v) == m).count() as i32 + 1
}

/// `c` with the opacity of `depth` layers of itself.
fn stacked(c: Rgba, depth: i32) -> Rgba {
    let clear = (1.0 - c[3] as f32 / 255.0).powi(depth);
    [c[0], c[1], c[2], ((1.0 - clear) * 255.0).round() as u8]
}
