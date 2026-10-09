//! Interactive first-person viewer: exports the exposed faces of a
//! [`VoxelGrid`] as compact binary buffers and wraps them in one self-contained
//! HTML page (three.js from cdnjs, the buffers inlined as base64) so a saved
//! town can be walked or flown through in a browser straight from disk — no
//! server, no build step. Colours, block shapes and, when a Minecraft client
//! jar is available (see [`assets`](super::assets)), textures come from the
//! same grid the PNG renders use, so the two agree.
//!
//! Flat face record (16 bytes, little-endian; positions in sixteenths of a
//! block, relative to the grid origin), used for materials without a model:
//!
//! ```text
//! i16 x, y, z   min corner of the face rectangle (the normal axis holds the plane)
//! i16 w, h      extent along the face's in-plane axes (u, v)
//! u8  dir       0 +X east, 1 -X west, 2 +Y up, 3 -Y down, 4 +Z south, 5 -Z north
//! u8  pad
//! u8  r g b a   straight-alpha colour; alpha < 255 is drawn translucent
//! ```
//!
//! In-plane axes: ±X → (u = z, v = y); ±Y → (u = x, v = z); ±Z → (u = x, v = y).
//! Full-cube faces are greedy-merged per slice, so a meadow or a flat roof is a
//! handful of quads instead of thousands; partial-block faces (slabs, stairs,
//! fences, panes, …) are emitted per box, unmerged. A face is skipped when the
//! neighbour across it hides it ([`VoxelGrid::hides`]) or, inside one shape,
//! when a sibling box covers it.
//!
//! Materials with a model ([`BlockGeometry`]) go into a second buffer instead,
//! drawn from one texture atlas (a PNG embedded in the page) through a key
//! table (`meta.tex.keys`: one row per distinct texture, UV rect, orientation,
//! tint and shading, see [`key_row`]):
//!
//! ```text
//! A record (16 bytes), axis-aligned model faces:
//! i16 x, y, z, w, h; u8 dir      as above
//! u8  flags                      bit 0: double-sided
//! u16 key                        row of meta.tex.keys
//! u16 pad
//!
//! R record (52 bytes), rotated or fractional faces (plants, ladders, …):
//! f32 corners[4][3]              counter-clockwise seen from outside, in blocks
//!                                from the grid origin; texture corners are
//!                                (u0,v0) (u0,v1) (u1,v1) (u1,v0) of the key's UV rect
//! u16 key; u8 dir; u8 flags
//! ```
//!
//! Full 16x16 boundary faces showing a whole texture are greedy-merged like the
//! flat ones; the page tiles the texture across the merged quad in its fragment
//! shader (`atlas cell + fract(uv) * size`), so the meadow stays a few quads.
//! Every other model face is emitted per cell. Coincident faces with the same
//! texture frame (grass side + tinted overlay) are baked into one composite
//! texture instead of fighting for depth.

use std::io::Write as _;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use flate2::{write::GzEncoder, Compression};
use resvg::tiny_skia::{ColorU8, Pixmap};

use super::assets::{BlockGeometry, Element, Face6, FaceTex, Texture};
use super::grid::{Rgba, VoxelGrid, BACKGROUND};
use super::scene::Scene;
use super::shape::Box16;
use crate::geometry::Rect3D;
use crate::util::HashMap;

/// Bytes per flat face record and per axis-aligned textured (A) record.
pub const RECORD_BYTES: usize = 16;
/// Bytes per rotated textured (R) record.
pub const ROTATED_RECORD_BYTES: usize = 52;

/// Largest grid edge the `i16` sixteenth coordinates can address.
const MAX_EDGE: i32 = i16::MAX as i32 / 16;

/// Tolerance for treating model coordinates as whole sixteenths.
const EPS: f32 = 1e-3;
/// Outward offset (sixteenths) for a coincident overlay face that could not
/// be baked into its base.
const NUDGE: f32 = 0.25;
const WHITE: Rgba = [255, 255, 255, 255];

const HTML_TEMPLATE: &str = include_str!("viewer.html");

type V3 = [f32; 3];

#[derive(Debug, Clone, Copy, Default)]
pub struct ViewerStats {
    /// Quads in the buffers (merged full-block faces + partial-block faces).
    pub faces: usize,
    /// Of which came from merging full-cube faces (flat or textured).
    pub merged_faces: usize,
    /// Of which are unmerged box or model faces.
    pub partial_faces: usize,
    /// Of which are textured model faces (axis-aligned and rotated).
    pub textured_faces: usize,
    /// Of which are rotated or fractional textured quads (R records).
    pub rotated_faces: usize,
    /// Raw buffer size in bytes (all records).
    pub buffer_bytes: usize,
    /// The buffers gzipped, as embedded in the page (before base64).
    pub gzip_bytes: usize,
    /// Size of the written HTML file.
    pub html_bytes: usize,
    /// Distinct textures packed into the atlas (0 without textures).
    pub atlas_textures: usize,
    /// Atlas image size in pixels.
    pub atlas_size: (u32, u32),
    /// Rows in the face key table.
    pub face_keys: usize,
}

/// Everything the page embeds.
pub struct Export {
    /// Flat-colour face records.
    pub flat: Vec<u8>,
    /// Textured faces, or `None` when no material has a model.
    pub textured: Option<TexturedExport>,
    pub stats: ViewerStats,
}

pub struct TexturedExport {
    /// A records followed by R records.
    pub faces: Vec<u8>,
    pub a_count: usize,
    pub r_count: usize,
    /// Key table rows, as embedded in `meta.tex.keys`.
    pub keys: Vec<serde_json::Value>,
    pub atlas_png: Vec<u8>,
    pub atlas_size: (u32, u32),
}

/// Offsets to the six neighbours, indexed by `dir`.
const DIRS: [(i32, i32, i32); 6] = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)];

/// `(u, v)` axis indices (0 = x, 1 = y, 2 = z) for faces normal to `axis`.
fn in_plane(axis: usize) -> (usize, usize) {
    match axis {
        0 => (2, 1),
        1 => (0, 2),
        _ => (0, 1),
    }
}

/// Growable buffer of face records.
#[derive(Default)]
struct FaceBuf {
    bytes: Vec<u8>,
    count: usize,
}

impl FaceBuf {
    fn push_rect(&mut self, p: [i32; 3], w: i32, h: i32, dir: u8, tail: [u8; 5]) {
        for v in [p[0], p[1], p[2], w, h] {
            debug_assert!(i16::try_from(v).is_ok(), "face coordinate {v} out of i16 range");
            self.bytes.extend_from_slice(&(v as i16).to_le_bytes());
        }
        self.bytes.push(dir);
        self.bytes.extend_from_slice(&tail);
        self.count += 1;
    }

    fn push(&mut self, p: [i32; 3], w: i32, h: i32, dir: u8, c: Rgba) {
        self.push_rect(p, w, h, dir, [0, c[0], c[1], c[2], c[3]]);
    }

    fn push_textured(&mut self, p: [i32; 3], w: i32, h: i32, dir: u8, flags: u8, key: u16) {
        let k = key.to_le_bytes();
        self.push_rect(p, w, h, dir, [flags, k[0], k[1], 0, 0]);
    }

    fn push_rotated(&mut self, corners: [V3; 4], key: u16, dir: u8, flags: u8) {
        for c in corners {
            for v in c {
                self.bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        self.bytes.extend_from_slice(&key.to_le_bytes());
        self.bytes.push(dir);
        self.bytes.push(flags);
        self.count += 1;
    }
}

fn pack(c: Rgba) -> u32 {
    u32::from_le_bytes(c)
}

fn unpack(k: u32) -> Rgba {
    k.to_le_bytes()
}

/// Export every exposed face of `g`: the flat-colour buffer plus, when the
/// grid carries block models, the textured buffer and atlas.
pub fn export(g: &VoxelGrid) -> Result<Export> {
    if g.nx > MAX_EDGE || g.ny > MAX_EDGE || g.nz > MAX_EDGE {
        return Err(anyhow!("grid {}x{}x{} too large for the viewer (max edge {MAX_EDGE} blocks)", g.nx, g.ny, g.nz));
    }
    let plain = |m: u16| g.geometry(m).is_none();
    let mut buf = FaceBuf::default();
    full_faces(g, &mut buf, &plain);
    let merged = buf.count;
    partial_faces(g, &mut buf, &plain);
    let mut stats = ViewerStats {
        faces: buf.count,
        merged_faces: merged,
        partial_faces: buf.count - merged,
        buffer_bytes: buf.bytes.len(),
        ..Default::default()
    };
    let textured = if g.has_textures() { Some(export_textured(g, &mut stats)?) } else { None };
    Ok(Export { flat: buf.bytes, textured, stats })
}

/// The flat-colour face buffer of `g` and the stats of the whole export
/// (`html_bytes` left at 0).
pub fn export_faces(g: &VoxelGrid) -> Result<(Vec<u8>, ViewerStats)> {
    let e = export(g)?;
    Ok((e.flat, e.stats))
}

/// Boundary faces of full-cube cells of `plain` materials, greedy-merged per
/// slice and colour.
fn full_faces(g: &VoxelGrid, out: &mut FaceBuf, plain: &dyn Fn(u16) -> bool) {
    let n = [g.nx, g.ny, g.nz];
    for axis in 0..3 {
        let (ua, va) = in_plane(axis);
        let (nu, nv) = (n[ua], n[va]);
        // Colour key per cell of the slice, 0 = no exposed face there.
        let mut pos = vec![0u32; (nu * nv) as usize];
        let mut neg = vec![0u32; (nu * nv) as usize];
        for s in 0..n[axis] {
            for v in 0..nv {
                for u in 0..nu {
                    let mut p = [0; 3];
                    p[axis] = s;
                    p[ua] = u;
                    p[va] = v;
                    let m = g.get(p[0], p[1], p[2]);
                    if !g.is_full(m) || !plain(m) {
                        continue;
                    }
                    let key = pack(g.color(m));
                    let mut q = p;
                    q[axis] = s + 1;
                    if !g.hides(g.get(q[0], q[1], q[2]), m) {
                        pos[(v * nu + u) as usize] = key;
                    }
                    q[axis] = s - 1;
                    if !g.hides(g.get(q[0], q[1], q[2]), m) {
                        neg[(v * nu + u) as usize] = key;
                    }
                }
            }
            for (mask, positive) in [(&mut pos, true), (&mut neg, false)] {
                let plane = if positive { (s + 1) * 16 } else { s * 16 };
                let dir = (axis * 2 + usize::from(!positive)) as u8;
                merge_rects(mask, nu, nv, |u, v, w, h, key| {
                    let mut p = [0; 3];
                    p[axis] = plane;
                    p[ua] = u * 16;
                    p[va] = v * 16;
                    out.push(p, w * 16, h * 16, dir, unpack(key));
                });
            }
        }
    }
}

/// Greedy rectangle cover of the non-zero cells of `mask` (`nu` wide, `nv`
/// tall, row-major by `v`), one key per rectangle. Consumed cells are zeroed,
/// so the mask is clean for the next slice.
fn merge_rects(mask: &mut [u32], nu: i32, nv: i32, mut emit: impl FnMut(i32, i32, i32, i32, u32)) {
    let at = |u: i32, v: i32| (v * nu + u) as usize;
    for v in 0..nv {
        for u in 0..nu {
            let key = mask[at(u, v)];
            if key == 0 {
                continue;
            }
            let mut w = 1;
            while u + w < nu && mask[at(u + w, v)] == key {
                w += 1;
            }
            let mut h = 1;
            'grow: while v + h < nv {
                for du in 0..w {
                    if mask[at(u + du, v + h)] != key {
                        break 'grow;
                    }
                }
                h += 1;
            }
            for dv in 0..h {
                for du in 0..w {
                    mask[at(u + du, v + dv)] = 0;
                }
            }
            emit(u, v, w, h, key);
        }
    }
}

/// Faces of every box of every partial-shape cell of a `plain` material, one
/// quad each.
fn partial_faces(g: &VoxelGrid, out: &mut FaceBuf, plain: &dyn Fn(u16) -> bool) {
    for x in 0..g.nx {
        for z in 0..g.nz {
            for y in 0..g.ny {
                let m = g.get(x, y, z);
                if m == 0 || g.is_full(m) || !plain(m) {
                    continue;
                }
                let c = g.color(m);
                let boxes = g.boxes(m);
                let base = [x * 16, y * 16, z * 16];
                for (bi, b) in boxes.iter().enumerate() {
                    for dir in 0..6 {
                        let axis = dir / 2;
                        let positive = dir % 2 == 0;
                        let plane = if positive { b[axis + 3] } else { b[axis] } as i32;
                        let (ua, va) = in_plane(axis);
                        let (w, h) = (b[ua + 3] as i32 - b[ua] as i32, b[va + 3] as i32 - b[va] as i32);
                        if w <= 0 || h <= 0 {
                            continue;
                        }
                        let on_boundary = plane == if positive { 16 } else { 0 };
                        if on_boundary {
                            let (dx, dy, dz) = DIRS[dir];
                            if g.hides(g.get(x + dx, y + dy, z + dz), m) {
                                continue;
                            }
                        } else if covered_by_sibling(boxes, bi, axis, positive, plane as u8) {
                            continue;
                        }
                        let mut p = base;
                        p[axis] += plane;
                        p[ua] += b[ua] as i32;
                        p[va] += b[va] as i32;
                        out.push(p, w, h, dir as u8, c);
                    }
                }
            }
        }
    }
}

/// Whether another box of the same shape sits flush against face `plane`
/// of box `bi` (on the `positive` side of `axis`) and covers it entirely.
fn covered_by_sibling(boxes: &[Box16], bi: usize, axis: usize, positive: bool, plane: u8) -> bool {
    let b = boxes[bi];
    let (ua, va) = in_plane(axis);
    boxes.iter().enumerate().any(|(oi, o)| {
        oi != bi
            && (if positive { o[axis] } else { o[axis + 3] }) == plane
            && o[ua] <= b[ua]
            && o[ua + 3] >= b[ua + 3]
            && o[va] <= b[va]
            && o[va + 3] >= b[va + 3]
    })
}

// ---------------------------------------------------------------------------
// Textured faces
// ---------------------------------------------------------------------------

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn mul_mat(m: &[[f32; 3]; 3], v: V3) -> V3 {
    let dot = |r: V3| r[0] * v[0] + r[1] * v[1] + r[2] * v[2];
    [dot(m[0]), dot(m[1]), dot(m[2])]
}
fn transpose(m: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    [[m[0][0], m[1][0], m[2][0]], [m[0][1], m[1][1], m[2][1]], [m[0][2], m[1][2], m[2][2]]]
}
fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < EPS
}

/// Viewer `dir` index of a model face.
fn face_dir(f: Face6) -> u8 {
    match f {
        Face6::East => 0,
        Face6::West => 1,
        Face6::Up => 2,
        Face6::Down => 3,
        Face6::South => 4,
        Face6::North => 5,
    }
}

fn normal(dir: u8) -> V3 {
    let (x, y, z) = DIRS[dir as usize];
    [x as f32, y as f32, z as f32]
}

/// The `dir` whose axis dominates `n`.
fn dominant_dir(n: V3) -> u8 {
    let axis = (0..3).max_by(|&a, &b| n[a].abs().total_cmp(&n[b].abs())).unwrap_or(1);
    (axis * 2) as u8 + u8::from(n[axis] < 0.0)
}

/// Multiply the colour channels of `c` by `t`.
fn tinted(c: Rgba, t: Rgba) -> Rgba {
    [
        ((c[0] as u32 * t[0] as u32) / 255) as u8,
        ((c[1] as u32 * t[1] as u32) / 255) as u8,
        ((c[2] as u32 * t[2] as u32) / 255) as u8,
        c[3],
    ]
}

/// Straight-alpha `top` composited onto `under`.
fn over(top: Rgba, under: Rgba) -> Rgba {
    let (ta, ua) = (top[3] as f32 / 255.0, under[3] as f32 / 255.0);
    let a = ta + ua * (1.0 - ta);
    if a <= 0.0 {
        return [0, 0, 0, 0];
    }
    let ch = |k: usize| ((top[k] as f32 * ta + under[k] as f32 * ua * (1.0 - ta)) / a).round() as u8;
    [ch(0), ch(1), ch(2), (a * 255.0).round() as u8]
}

/// Every distinct texture a page uses, packed into one image.
#[derive(Default)]
struct Atlas {
    textures: Vec<Arc<Texture>>,
    /// Jar textures by `Arc` identity.
    by_ptr: HashMap<usize, u16>,
    /// Baked overlays by (base, overlay, base tint, overlay tint).
    composites: HashMap<(u16, u16, Rgba, Rgba), u16>,
}

impl Atlas {
    fn intern(&mut self, t: &Arc<Texture>) -> u16 {
        let p = Arc::as_ptr(t) as usize;
        if let Some(&i) = self.by_ptr.get(&p) {
            return i;
        }
        self.textures.push(t.clone());
        let i = (self.textures.len() - 1) as u16;
        self.by_ptr.insert(p, i);
        i
    }

    /// Texture `top` (tinted `tt`) composited onto `base` (tinted `bt`),
    /// baked once per combination. Both must have the same size.
    fn composite(&mut self, base: u16, bt: Option<Rgba>, top: u16, tt: Option<Rgba>) -> u16 {
        let key = (base, top, bt.unwrap_or(WHITE), tt.unwrap_or(WHITE));
        if let Some(&i) = self.composites.get(&key) {
            return i;
        }
        let (b, t) = (&self.textures[base as usize], &self.textures[top as usize]);
        let rgba = b.rgba.iter().zip(&t.rgba).map(|(&bc, &tc)| over(tinted(tc, key.3), tinted(bc, key.2))).collect();
        self.textures.push(Arc::new(Texture { size: b.size, rgba, avg: b.avg }));
        let i = (self.textures.len() - 1) as u16;
        self.composites.insert(key, i);
        i
    }

    /// Shelf-pack the textures, largest first, into a power-of-two-wide
    /// image: `(cell origin per texture, width, height)`.
    fn pack(&self) -> (Vec<(u32, u32)>, u32, u32) {
        let area: u32 = self.textures.iter().map(|t| t.size * t.size).sum();
        let largest = self.textures.iter().map(|t| t.size).max().unwrap_or(16);
        let mut width = 16u32;
        while width * width < area {
            width *= 2;
        }
        let width = width.max(largest.next_power_of_two());
        let mut order: Vec<usize> = (0..self.textures.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(self.textures[i].size));
        let mut pos = vec![(0, 0); self.textures.len()];
        let (mut x, mut y, mut row) = (0, 0, 0);
        for i in order {
            let s = self.textures[i].size;
            if x + s > width {
                x = 0;
                y += row;
                row = 0;
            }
            pos[i] = (x, y);
            x += s;
            row = row.max(s);
        }
        (pos, width, (y + row).max(1))
    }

    fn png(&self, pos: &[(u32, u32)], w: u32, h: u32) -> Result<Vec<u8>> {
        let mut pm = Pixmap::new(w, h).ok_or_else(|| anyhow!("bad atlas size {w}x{h}"))?;
        let px = pm.pixels_mut();
        for (t, &(x0, y0)) in self.textures.iter().zip(pos) {
            for y in 0..t.size {
                for x in 0..t.size {
                    let c = t.rgba[(y * t.size + x) as usize];
                    px[((y0 + y) * w + x0 + x) as usize] = ColorU8::from_rgba(c[0], c[1], c[2], c[3]).premultiply();
                }
            }
        }
        pm.encode_png().map_err(|e| anyhow!("encode atlas: {e}"))
    }
}

/// How one kind of face is textured: a row of the page's key table.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct FaceKey {
    tex: u16,
    /// `[u0, v0, u1, v1]` in Minecraft texture units (0..16), as `f32` bits.
    uv: [u32; 4],
    /// Face extent along the texture's u and v (sixteenths): the tiling period.
    period: [u8; 2],
    /// bit 0: u runs along the face's in-plane v axis; bit 1: u runs
    /// negative; bit 2: v runs negative.
    orient: u8,
    tint: [u8; 3],
    shade: bool,
    translucent: bool,
}

#[derive(Default)]
struct Keys {
    list: Vec<FaceKey>,
    index: HashMap<FaceKey, u16>,
}

impl Keys {
    fn intern(&mut self, k: FaceKey) -> u16 {
        if let Some(&i) = self.index.get(&k) {
            return i;
        }
        self.list.push(k);
        let i = (self.list.len() - 1) as u16;
        self.index.insert(k, i);
        i
    }
}

fn tint3(t: Option<Rgba>) -> [u8; 3] {
    let t = t.unwrap_or(WHITE);
    [t[0], t[1], t[2]]
}

/// Key for a face whose corners carry their own texture coordinates.
fn plain_key(tex: u16, ft: &FaceTex, tint: Option<Rgba>, translucent: bool) -> FaceKey {
    FaceKey { tex, uv: ft.uv.map(f32::to_bits), period: [16, 16], orient: 0, tint: tint3(tint), shade: ft.shade, translucent }
}

/// Key table row: `[cell x, cell y, size, u0, v0, u1, v1, period u, period
/// v, orient, tint r, g, b, flags]` with flags bit 0 = shaded, bit 1 =
/// translucent.
fn key_row(k: &FaceKey, (cx, cy): (u32, u32), size: u32) -> serde_json::Value {
    let uv = k.uv.map(|b| (f32::from_bits(b) as f64 * 1000.0).round() / 1000.0);
    serde_json::json!([
        cx,
        cy,
        size,
        uv[0],
        uv[1],
        uv[2],
        uv[3],
        k.period[0],
        k.period[1],
        k.orient,
        k.tint[0],
        k.tint[1],
        k.tint[2],
        u8::from(k.shade) | u8::from(k.translucent) << 1
    ])
}

/// An axis-aligned model face in cell-local sixteenths.
#[derive(Clone, Copy)]
struct AQuad {
    dir: u8,
    plane: i32,
    u: i32,
    v: i32,
    w: i32,
    h: i32,
    key: u16,
    double: bool,
    /// A full boundary face showing a whole texture: merged per slice.
    tile: bool,
}

/// A model face with explicit corners (cell-local blocks).
struct RQuad {
    corners: [V3; 4],
    key: u16,
    dir: u8,
    double: bool,
}

#[derive(Default)]
struct MatQuads {
    a: Vec<AQuad>,
    r: Vec<RQuad>,
}

/// An axis-aligned face on whole sixteenths, before compositing and culling.
struct AFace<'a> {
    elem: usize,
    dir: u8,
    plane: i32,
    /// `[u, v, w, h]` in the face's in-plane axes.
    rect: [i32; 4],
    ft: &'a FaceTex,
    tex: u16,
    tint: Option<Rgba>,
    double: bool,
}

/// Each coordinate rounded to a whole sixteenth, if all are within `EPS` of one.
fn whole<const N: usize>(v: [f32; N]) -> Option<[i32; N]> {
    let mut out = [0; N];
    for (o, &x) in out.iter_mut().zip(&v) {
        let r = x.round();
        if (x - r).abs() >= EPS {
            return None;
        }
        *o = r as i32;
    }
    Some(out)
}

/// Whether two faces lay their texture identically (so they can be baked together).
fn same_frame(a: &FaceTex, b: &FaceTex) -> bool {
    a.texture.size == b.texture.size
        && a.uv.iter().zip(&b.uv).all(|(&x, &y)| near(x, y))
        && (0..3).all(|k| near(a.origin[k], b.origin[k]) && near(a.du[k], b.du[k]) && near(a.dv[k], b.dv[k]))
}

fn whole_texture(uv: [f32; 4]) -> bool {
    near((uv[2] - uv[0]).abs(), 16.0) && near((uv[3] - uv[1]).abs(), 16.0)
}

/// Whether another element's box sits flush against the face at `plane`
/// (facing `positive` along `axis`, covering `rect`) and hides it entirely.
fn covered_by_element(elements: &[Element], me: usize, axis: usize, positive: bool, plane: i32, rect: [i32; 4]) -> bool {
    let (ua, va) = in_plane(axis);
    let (u0, v0, u1, v1) = (rect[0] as f32, rect[1] as f32, (rect[0] + rect[2]) as f32, (rect[1] + rect[3]) as f32);
    elements.iter().enumerate().any(|(oi, o)| {
        oi != me
            && o.local.is_none()
            && o.to[axis] - o.from[axis] > EPS
            && near(if positive { o.from[axis] } else { o.to[axis] }, plane as f32)
            && o.from[ua] <= u0 + EPS
            && o.to[ua] >= u1 - EPS
            && o.from[va] <= v0 + EPS
            && o.to[va] >= v1 - EPS
    })
}

/// `(orient bits, period)` describing how `ft` lays its texture over the
/// face normal to `axis` with in-plane `rect`, or `None` when the frame is
/// not anchored on the face's corners along its axes.
fn orientation(ft: &FaceTex, axis: usize, rect: [i32; 4]) -> Option<(u8, [u8; 2])> {
    let (ua, va) = in_plane(axis);
    let swap = ft.du[va].abs() > ft.du[ua].abs();
    let (sa, ta) = if swap { (va, ua) } else { (ua, va) };
    let (s_min, s_ext) = if swap { (rect[1], rect[3]) } else { (rect[0], rect[2]) };
    let (t_min, t_ext) = if swap { (rect[0], rect[2]) } else { (rect[1], rect[3]) };
    let flip_s = ft.du[sa] < 0.0;
    let flip_t = ft.dv[ta] < 0.0;
    let ok = near(ft.du[sa].abs(), s_ext as f32)
        && near(ft.dv[ta].abs(), t_ext as f32)
        && near(ft.du[ta], 0.0)
        && near(ft.dv[sa], 0.0)
        && near(ft.du[axis], 0.0)
        && near(ft.dv[axis], 0.0)
        && near(ft.origin[sa], (if flip_s { s_min + s_ext } else { s_min }) as f32)
        && near(ft.origin[ta], (if flip_t { t_min + t_ext } else { t_min }) as f32);
    if !ok || !(1..=255).contains(&s_ext) || !(1..=255).contains(&t_ext) {
        return None;
    }
    Some((u8::from(swap) | u8::from(flip_s) << 1 | u8::from(flip_t) << 2, [s_ext as u8, t_ext as u8]))
}

/// The quad of face `ft` of element `e` with explicit corners, pushed
/// `nudge` sixteenths along its normal, in cell-local blocks.
fn rotated_quad(e: &Element, ft: &FaceTex, dir: u8, key: u16, double: bool, nudge: f32) -> RQuad {
    let n = normal(dir);
    let o = add(ft.origin, scale(n, nudge));
    let local = [o, add(o, ft.dv), add(add(o, ft.du), ft.dv), add(o, ft.du)];
    let world = |p: V3| match &e.local {
        Some(f) => mul_mat(&transpose(&f.m), sub(p, f.t)),
        None => p,
    };
    let corners = local.map(|p| scale(world(p), 1.0 / 16.0));
    let dir = match &e.local {
        Some(f) => dominant_dir(mul_mat(&transpose(&f.m), n)),
        None => dir,
    };
    RQuad { corners, key, dir, double }
}

/// The exported faces of one material's model, in cell-local coordinates.
fn material_quads(geo: &BlockGeometry, translucent: bool, atlas: &mut Atlas, keys: &mut Keys) -> MatQuads {
    let mut out = MatQuads::default();
    let mut planar: Vec<AFace> = Vec::new();
    for (ei, e) in geo.elements.iter().enumerate() {
        for (face, ft) in &e.faces {
            let dir = face_dir(*face);
            let axis = (dir / 2) as usize;
            let positive = dir % 2 == 0;
            // A zero-thickness element showing both its sides is one double-sided quad.
            let thin = e.to[axis] - e.from[axis] < EPS;
            let both = thin && e.faces.iter().any(|(f, _)| face_dir(*f) == dir ^ 1);
            if both && !positive {
                continue;
            }
            let tex = atlas.intern(&ft.texture);
            if e.local.is_some() {
                let key = keys.intern(plain_key(tex, ft, ft.tint, translucent));
                out.r.push(rotated_quad(e, ft, dir, key, both, 0.0));
                continue;
            }
            let plane = if positive { e.to[axis] } else { e.from[axis] };
            let (ua, va) = in_plane(axis);
            let rect = [e.from[ua], e.from[va], e.to[ua] - e.from[ua], e.to[va] - e.from[va]];
            if rect[2] < EPS || rect[3] < EPS {
                continue;
            }
            match whole([plane, rect[0], rect[1], rect[2], rect[3]]) {
                Some([plane, u, v, w, h]) => {
                    planar.push(AFace { elem: ei, dir, plane, rect: [u, v, w, h], ft, tex, tint: ft.tint, double: both })
                }
                None => {
                    let key = keys.intern(plain_key(tex, ft, ft.tint, translucent));
                    out.r.push(rotated_quad(e, ft, dir, key, both, 0.0));
                }
            }
        }
    }

    // Coincident faces: bake the later one (an overlay) into the earlier one
    // when their frames agree; otherwise float it just off the surface.
    let mut kept: Vec<AFace> = Vec::new();
    for f in planar {
        if let Some(prev) = kept.iter_mut().find(|k| k.dir == f.dir && k.plane == f.plane && k.rect == f.rect) {
            if same_frame(prev.ft, f.ft) && atlas.textures[prev.tex as usize].size == f.ft.texture.size {
                prev.tex = atlas.composite(prev.tex, prev.tint, f.tex, f.tint);
                prev.tint = None;
            } else {
                let key = keys.intern(plain_key(f.tex, f.ft, f.tint, translucent));
                out.r.push(rotated_quad(&geo.elements[f.elem], f.ft, f.dir, key, f.double, NUDGE));
            }
            continue;
        }
        kept.push(f);
    }

    for f in kept {
        let axis = (f.dir / 2) as usize;
        let positive = f.dir % 2 == 0;
        let boundary = f.plane == if positive { 16 } else { 0 };
        if !boundary && covered_by_element(&geo.elements, f.elem, axis, positive, f.plane, f.rect) {
            continue;
        }
        let Some((orient, period)) = orientation(f.ft, axis, f.rect) else {
            let key = keys.intern(plain_key(f.tex, f.ft, f.tint, translucent));
            out.r.push(rotated_quad(&geo.elements[f.elem], f.ft, f.dir, key, f.double, 0.0));
            continue;
        };
        let key = keys.intern(FaceKey {
            tex: f.tex,
            uv: f.ft.uv.map(f32::to_bits),
            period,
            orient,
            tint: tint3(f.tint),
            shade: f.ft.shade,
            translucent,
        });
        let tile = boundary && f.rect == [0, 0, 16, 16] && whole_texture(f.ft.uv);
        out.a.push(AQuad { dir: f.dir, plane: f.plane, u: f.rect[0], v: f.rect[1], w: f.rect[2], h: f.rect[3], key, double: f.double, tile });
    }
    out
}

/// Tileable boundary faces of textured materials, greedy-merged per slice
/// and key like [`full_faces`].
fn merged_textured_faces(g: &VoxelGrid, mats: &[Option<MatQuads>], out: &mut FaceBuf) {
    // Key + 1 of the tileable face per material and dir; 0 = none.
    let tile: Vec<[u32; 6]> = mats
        .iter()
        .map(|mq| {
            let mut t = [0u32; 6];
            for q in mq.iter().flat_map(|mq| &mq.a).filter(|q| q.tile) {
                t[q.dir as usize] = q.key as u32 + 1;
            }
            t
        })
        .collect();
    let n = [g.nx, g.ny, g.nz];
    for axis in 0..3 {
        let (ua, va) = in_plane(axis);
        let (nu, nv) = (n[ua], n[va]);
        let mut pos = vec![0u32; (nu * nv) as usize];
        let mut neg = vec![0u32; (nu * nv) as usize];
        for s in 0..n[axis] {
            for v in 0..nv {
                for u in 0..nu {
                    let mut p = [0; 3];
                    p[axis] = s;
                    p[ua] = u;
                    p[va] = v;
                    let m = g.get(p[0], p[1], p[2]);
                    let t = tile[m as usize];
                    let mut q = p;
                    q[axis] = s + 1;
                    if t[axis * 2] != 0 && !g.hides(g.get(q[0], q[1], q[2]), m) {
                        pos[(v * nu + u) as usize] = t[axis * 2];
                    }
                    q[axis] = s - 1;
                    if t[axis * 2 + 1] != 0 && !g.hides(g.get(q[0], q[1], q[2]), m) {
                        neg[(v * nu + u) as usize] = t[axis * 2 + 1];
                    }
                }
            }
            for (mask, positive) in [(&mut pos, true), (&mut neg, false)] {
                let plane = if positive { (s + 1) * 16 } else { s * 16 };
                let dir = (axis * 2 + usize::from(!positive)) as u8;
                merge_rects(mask, nu, nv, |u, v, w, h, key| {
                    let mut p = [0; 3];
                    p[axis] = plane;
                    p[ua] = u * 16;
                    p[va] = v * 16;
                    out.push_textured(p, w * 16, h * 16, dir, 0, (key - 1) as u16);
                });
            }
        }
    }
}

/// The remaining model faces of every textured cell, one record each.
fn cell_textured_faces(g: &VoxelGrid, mats: &[Option<MatQuads>], a: &mut FaceBuf, r: &mut FaceBuf) {
    for x in 0..g.nx {
        for z in 0..g.nz {
            for y in 0..g.ny {
                let m = g.get(x, y, z);
                let Some(mq) = mats[m as usize].as_ref() else { continue };
                let base = [x * 16, y * 16, z * 16];
                for q in mq.a.iter().filter(|q| !q.tile) {
                    let axis = (q.dir / 2) as usize;
                    let positive = q.dir % 2 == 0;
                    if q.plane == if positive { 16 } else { 0 } {
                        let (dx, dy, dz) = DIRS[q.dir as usize];
                        if g.hides(g.get(x + dx, y + dy, z + dz), m) {
                            continue;
                        }
                    }
                    let (ua, va) = in_plane(axis);
                    let mut p = base;
                    p[axis] += q.plane;
                    p[ua] += q.u;
                    p[va] += q.v;
                    a.push_textured(p, q.w, q.h, q.dir, u8::from(q.double), q.key);
                }
                for q in &mq.r {
                    let cell = [x as f32, y as f32, z as f32];
                    r.push_rotated(q.corners.map(|c| add(c, cell)), q.key, q.dir, u8::from(q.double));
                }
            }
        }
    }
}

fn export_textured(g: &VoxelGrid, stats: &mut ViewerStats) -> Result<TexturedExport> {
    let mut atlas = Atlas::default();
    let mut keys = Keys::default();
    let mats: Vec<Option<MatQuads>> = (0..g.material_count() as u16)
        .map(|m| g.geometry(m).map(|geo| material_quads(geo, !g.is_opaque(m), &mut atlas, &mut keys)))
        .collect();
    let (mut a, mut r) = (FaceBuf::default(), FaceBuf::default());
    merged_textured_faces(g, &mats, &mut a);
    let merged = a.count;
    cell_textured_faces(g, &mats, &mut a, &mut r);
    let (pos, w, h) = atlas.pack();
    let atlas_png = atlas.png(&pos, w, h)?;
    let rows = keys.list.iter().map(|k| key_row(k, pos[k.tex as usize], atlas.textures[k.tex as usize].size)).collect();

    stats.faces += a.count + r.count;
    stats.merged_faces += merged;
    stats.partial_faces += a.count - merged + r.count;
    stats.textured_faces = a.count + r.count;
    stats.rotated_faces = r.count;
    stats.buffer_bytes += a.bytes.len() + r.bytes.len();
    stats.atlas_textures = atlas.textures.len();
    stats.atlas_size = (w, h);
    stats.face_keys = keys.list.len();

    let mut faces = a.bytes;
    faces.extend_from_slice(&r.bytes);
    Ok(TexturedExport { faces, a_count: a.count, r_count: r.count, keys: rows, atlas_png, atlas_size: (w, h) })
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

/// Standard base64 (RFC 4648, with padding).
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        s.push(T[(n >> 18 & 63) as usize] as char);
        s.push(T[(n >> 12 & 63) as usize] as char);
        s.push(if chunk.len() > 1 { T[(n >> 6 & 63) as usize] as char } else { '=' });
        s.push(if chunk.len() > 2 { T[(n & 63) as usize] as char } else { '=' });
    }
    s
}

fn gzip(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut enc = GzEncoder::new(Vec::with_capacity(bytes.len() / 4), Compression::default());
    enc.write_all(bytes)?;
    Ok(enc.finish()?)
}

/// The page template for a flat or textured page. `viewer.html` holds both:
/// lines between `<!--@if textured-->` and `<!--@else-->` (or `<!--@end-->`)
/// are kept only for textured pages, lines between `<!--@else-->` and
/// `<!--@end-->` only for flat ones, and the marker lines themselves vanish.
fn template(textured: bool) -> String {
    let mut out = String::with_capacity(HTML_TEMPLATE.len());
    let mut keep = true;
    for line in HTML_TEMPLATE.split_inclusive('\n') {
        match line.trim() {
            "<!--@if textured-->" => keep = textured,
            "<!--@else-->" => keep = !textured,
            "<!--@end-->" => keep = true,
            _ => {
                if keep {
                    out.push_str(line);
                }
            }
        }
    }
    out
}

/// Build the viewer page for `scene` (optionally cropped to `region`, absolute
/// coords) and write it to `out`. The page is self-contained apart from
/// three.js, which it loads from cdnjs. The face buffers are gzipped (the page
/// inflates them with `DecompressionStream`) and embedded as base64, as is the
/// texture atlas PNG when the grid carries block models.
pub fn write_viewer_html(scene: &Scene, region: Option<Rect3D>, out: &Path) -> Result<ViewerStats> {
    let g = VoxelGrid::from_scene(scene, region).ok_or_else(|| anyhow!("nothing visible to export"))?;
    let Export { flat, textured, mut stats } = export(&g)?;
    let gz = gzip(&flat)?;
    stats.gzip_bytes = gz.len();
    let title = match scene.meta.get("seed") {
        Some(seed) => format!("Tome town {seed}"),
        None => "Tome town".to_string(),
    };
    let mut meta = serde_json::json!({
        "origin": [g.origin.x, g.origin.y, g.origin.z],
        "size": [g.nx, g.ny, g.nz],
        "faces": stats.faces,
        "encoding": "gzip",
        "background": [BACKGROUND[0], BACKGROUND[1], BACKGROUND[2]],
        "title": title,
        "scene_meta": scene.meta,
    });
    let mut html = template(textured.is_some());
    if let Some(t) = &textured {
        let tgz = gzip(&t.faces)?;
        stats.gzip_bytes += tgz.len();
        meta["tex"] = serde_json::json!({
            "a": t.a_count,
            "r": t.r_count,
            "atlas": [t.atlas_size.0, t.atlas_size.1],
            "keys": t.keys,
        });
        html = html.replacen("__TEX__", &base64(&tgz), 1).replacen("__ATLAS__", &base64(&t.atlas_png), 1);
    }
    // `<` never reaches the page's HTML parser (it would end the JSON <script>).
    let meta_json = meta.to_string().replace('<', "\\u003c");
    let html = html
        .replacen("__TITLE__", &html_escape(&title), 1)
        .replacen("__META__", &meta_json, 1)
        .replacen("__DATA__", &base64(&gz), 1);
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::write(out, &html).with_context(|| format!("write {}", out.display()))?;
    stats.html_bytes = html.len();
    Ok(stats)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point3D;

    fn scene_with(blocks: &[(i32, i32, i32, &str)]) -> Scene {
        let mut s = Scene::new(Rect3D { origin: Point3D::new(0, 60, 0), size: Point3D::new(8, 8, 8) });
        for &(x, y, z, b) in blocks {
            s.insert(Point3D::new(x, y, z), b);
        }
        s
    }

    /// Rectangle records (flat ones, then textured A ones): the five `i16`
    /// coordinates and the dir of each. Both layouts share that prefix, so
    /// the geometry checks hold with or without a Minecraft jar.
    fn faces(scene: &Scene) -> (Vec<[i16; 5]>, Vec<u8>, ViewerStats) {
        let g = VoxelGrid::from_scene(scene, None).unwrap();
        let e = export(&g).unwrap();
        let mut bytes = e.flat;
        let mut rotated = 0;
        if let Some(t) = &e.textured {
            bytes.extend_from_slice(&t.faces[..t.a_count * RECORD_BYTES]);
            rotated = t.r_count;
        }
        assert_eq!(bytes.len(), (e.stats.faces - rotated) * RECORD_BYTES);
        let mut recs = Vec::new();
        let mut dirs = Vec::new();
        for r in bytes.chunks(RECORD_BYTES) {
            let v = |i: usize| i16::from_le_bytes([r[i * 2], r[i * 2 + 1]]);
            recs.push([v(0), v(1), v(2), v(3), v(4)]);
            dirs.push(r[10]);
        }
        (recs, dirs, e.stats)
    }

    #[test]
    fn lone_cube_has_six_faces() {
        let (recs, dirs, stats) = faces(&scene_with(&[(1, 61, 1, "minecraft:stone")]));
        assert_eq!(stats.faces, 6);
        assert_eq!(stats.partial_faces, 0);
        let mut d = dirs.clone();
        d.sort_unstable();
        assert_eq!(d, vec![0, 1, 2, 3, 4, 5]);
        // Every face is a full 16x16 quad anchored on the cell, which is local
        // (1, 0, 1): the grid's y range shrinks to the blocks present.
        let cell = [1, 0, 1];
        for (r, &dir) in recs.iter().zip(&dirs) {
            assert_eq!((r[3], r[4]), (16, 16));
            let axis = (dir / 2) as usize;
            let plane = if dir % 2 == 0 { (cell[axis] + 1) * 16 } else { cell[axis] * 16 };
            assert_eq!(r[axis], plane, "dir {dir}");
        }
    }

    #[test]
    fn adjacent_cubes_merge_and_hide_shared_face() {
        // Two stone side by side along x: shared faces culled, the rest merged into 6 quads.
        let s = scene_with(&[(1, 61, 1, "minecraft:stone"), (2, 61, 1, "minecraft:stone")]);
        let (recs, dirs, stats) = faces(&s);
        assert_eq!(stats.faces, 6);
        let top = recs.iter().zip(&dirs).find(|(_, &d)| d == 2).unwrap().0;
        assert_eq!(top, &[16, 16, 16, 32, 16]);
        // Different colours (or textures) do not merge.
        let s = scene_with(&[(1, 61, 1, "minecraft:stone"), (2, 61, 1, "minecraft:oak_planks")]);
        assert_eq!(faces(&s).2.faces, 10);
    }

    #[test]
    fn slab_on_cube_culls_the_touching_faces() {
        let s = scene_with(&[(1, 61, 1, "minecraft:stone"), (1, 62, 1, "minecraft:stone_slab[type=bottom]")]);
        let (recs, dirs, stats) = faces(&s);
        // Cube: 6 (its top stays, the slab is not a full block). Slab: 5 (bottom hidden by the cube).
        assert_eq!(stats.merged_faces, 6);
        assert_eq!(stats.partial_faces, 5);
        // Slab cell is local y = 1, so its top sits at 16 + 8 sixteenths.
        let slab_top = recs.iter().zip(&dirs).filter(|(_, &d)| d == 2).find(|(r, _)| r[1] == 24).unwrap().0;
        assert_eq!(slab_top, &[16, 24, 16, 16, 16]);
    }

    #[test]
    fn water_has_no_inner_faces() {
        let s = scene_with(&[(1, 61, 1, "minecraft:water"), (2, 61, 1, "minecraft:water"), (1, 60, 1, "minecraft:water")]);
        let (recs, _, stats) = faces(&s);
        // An L of three water cells: 3 * 6 = 18 faces minus 2 shared pairs = 14, merged where coplanar.
        assert!(stats.faces < 14, "{}", stats.faces);
        assert!(recs.iter().all(|r| r[3] > 0 && r[4] > 0));
    }

    #[test]
    fn base64_matches_rfc_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn writes_a_self_contained_page() {
        let s = scene_with(&[(1, 61, 1, "minecraft:stone"), (2, 61, 1, "minecraft:glass")]);
        let dir = std::env::temp_dir().join("tome_viewer_test");
        let path = dir.join("viewer.html");
        let stats = write_viewer_html(&s, None, &path).unwrap();
        let html = std::fs::read_to_string(&path).unwrap();
        assert_eq!(stats.html_bytes, html.len());
        assert!(html.contains("cdnjs.cloudflare.com/ajax/libs/three.js/"));
        assert!(html.contains(r#""origin":[0,61,0]"#));
        for placeholder in ["__DATA__", "__META__", "__TITLE__", "__TEX__", "__ATLAS__", "<!--@"] {
            assert!(!html.contains(placeholder), "{placeholder} left in the page");
        }
    }

    #[test]
    fn template_variants_select_their_lines() {
        let flat = template(false);
        let textured = template(true);
        assert!(!flat.contains("<!--@") && !textured.contains("<!--@"));
        assert!(flat.contains("__DATA__") && textured.contains("__DATA__"));
        assert!(!flat.contains("__TEX__") && !flat.contains("__ATLAS__"));
        assert!(textured.contains("__TEX__") && textured.contains("__ATLAS__"));
        // Markers come in balanced if/[else/]end groups, and the line endings
        // of the file survive the split.
        let count = |m: &str| HTML_TEMPLATE.lines().filter(|l| l.trim() == m).count();
        assert_eq!(count("<!--@if textured-->"), count("<!--@end-->"));
        assert!(count("<!--@else-->") <= count("<!--@if textured-->"));
        assert_eq!(flat.contains('\r'), HTML_TEMPLATE.contains('\r'));
        assert!(flat.len() < textured.len());
    }

    #[test]
    fn rotated_quads_keep_their_winding_and_dir() {
        // A full-cube Up face in the element frame, no local rotation.
        let tex = Arc::new(Texture { size: 16, rgba: vec![[0, 0, 0, 255]; 256], avg: [0, 0, 0, 255] });
        let ft = FaceTex {
            texture: tex,
            origin: [0.0, 16.0, 0.0],
            du: [16.0, 0.0, 0.0],
            dv: [0.0, 0.0, 16.0],
            uv: [0.0, 0.0, 16.0, 16.0],
            tint: None,
            shade: true,
        };
        let e = Element { from: [0.0; 3], to: [16.0; 3], faces: vec![(Face6::Up, ft)], local: None };
        let (_, ft) = &e.faces[0];
        let q = rotated_quad(&e, ft, 2, 0, false, 0.0);
        let c = q.corners;
        // Counter-clockwise seen from above: (c1 - c0) x (c2 - c1) points up.
        let (a, b) = (sub(c[1], c[0]), sub(c[2], c[1]));
        let cross_y = a[2] * b[0] - a[0] * b[2];
        assert!(cross_y > 0.0, "corners {c:?}");
        assert_eq!(c[0], [0.0, 1.0, 0.0]);
        assert_eq!(c[2], [1.0, 1.0, 1.0]);
        assert_eq!(q.dir, 2);
        assert_eq!(dominant_dir([0.1, -0.9, 0.2]), 3);
        assert_eq!(dominant_dir([-0.7, 0.0, 0.69]), 1);
    }

    #[test]
    fn orientation_reads_minecraft_face_frames() {
        let tex = Arc::new(Texture { size: 16, rgba: vec![[0, 0, 0, 255]; 256], avg: [0, 0, 0, 255] });
        let ft = |origin: V3, du: V3, dv: V3| FaceTex {
            texture: tex.clone(),
            origin,
            du,
            dv,
            uv: [0.0, 0.0, 16.0, 16.0],
            tint: None,
            shade: true,
        };
        let full = [0, 0, 16, 16];
        // Up: u along +x, v along +z.
        assert_eq!(orientation(&ft([0.0, 16.0, 0.0], [16.0, 0.0, 0.0], [0.0, 0.0, 16.0]), 1, full), Some((0, [16, 16])));
        // North: u along -x, v along -y (both flipped).
        assert_eq!(orientation(&ft([16.0, 16.0, 0.0], [-16.0, 0.0, 0.0], [0.0, -16.0, 0.0]), 2, full), Some((6, [16, 16])));
        // Up rotated 90 degrees: u along +z (the in-plane v axis), v along -x.
        assert_eq!(orientation(&ft([16.0, 16.0, 0.0], [0.0, 0.0, 16.0], [-16.0, 0.0, 0.0]), 1, full), Some((5, [16, 16])));
        // A frame anchored off the face's corner is not expressible.
        assert_eq!(orientation(&ft([4.0, 16.0, 0.0], [16.0, 0.0, 0.0], [0.0, 0.0, 16.0]), 1, full), None);
        // Half-height south side: the period follows the face extent.
        assert_eq!(orientation(&ft([0.0, 8.0, 16.0], [16.0, 0.0, 0.0], [0.0, -8.0, 0.0]), 2, [0, 0, 16, 8]), Some((4, [16, 8])));
    }

    #[test]
    fn overlays_bake_over_their_base() {
        let base = Arc::new(Texture { size: 1, rgba: vec![[100, 100, 100, 255]], avg: [100, 100, 100, 255] });
        let overlay = Arc::new(Texture { size: 1, rgba: vec![[200, 200, 200, 128]], avg: [200, 200, 200, 128] });
        let mut atlas = Atlas::default();
        let (b, o) = (atlas.intern(&base), atlas.intern(&overlay));
        assert_eq!(atlas.intern(&base), b, "same Arc interns once");
        let c = atlas.composite(b, None, o, Some([0, 255, 0, 255]));
        assert_eq!(atlas.composite(b, None, o, Some([0, 255, 0, 255])), c, "baked once per combination");
        let px = atlas.textures[c as usize].rgba[0];
        // Green-tinted half-opaque overlay over grey: red/blue halve, green rises.
        assert_eq!(px[3], 255);
        assert!(px[0] < 60 && px[2] < 60 && px[1] > 140, "{px:?}");
        let (pos, w, h) = atlas.pack();
        assert_eq!(pos.len(), 3);
        assert!(w >= 16 && h >= 1);
        assert!(atlas.png(&pos, w, h).unwrap().starts_with(&[0x89, b'P', b'N', b'G']));
    }
}
