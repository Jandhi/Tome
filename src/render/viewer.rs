//! Interactive first-person viewer: exports the exposed faces of a
//! [`VoxelGrid`] as a compact binary buffer and wraps it in one self-contained
//! HTML page (three.js from cdnjs, the buffer inlined as base64) so a saved
//! town can be walked or flown through in a browser straight from disk — no
//! server, no build step. Colours and block shapes come from the same grid the
//! PNG renders use, so the two agree.
//!
//! Face record (16 bytes, little-endian; positions in sixteenths of a block,
//! relative to the grid origin):
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

use std::io::Write as _;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use flate2::{write::GzEncoder, Compression};

use super::grid::{Rgba, VoxelGrid, BACKGROUND};
use super::scene::Scene;
use super::shape::Box16;
use crate::geometry::Rect3D;

/// Bytes per face record in the exported buffer.
pub const RECORD_BYTES: usize = 16;

/// Largest grid edge the `i16` sixteenth coordinates can address.
const MAX_EDGE: i32 = i16::MAX as i32 / 16;

const HTML_TEMPLATE: &str = include_str!("viewer.html");

#[derive(Debug, Clone, Copy, Default)]
pub struct ViewerStats {
    /// Quads in the buffer (merged full-block faces + partial-block faces).
    pub faces: usize,
    /// Of which came from merging full-cube faces.
    pub merged_faces: usize,
    /// Of which are partial-block box faces (never merged).
    pub partial_faces: usize,
    /// Raw buffer size in bytes (`faces * RECORD_BYTES`).
    pub buffer_bytes: usize,
    /// The buffer gzipped, as embedded in the page (before base64).
    pub gzip_bytes: usize,
    /// Size of the written HTML file.
    pub html_bytes: usize,
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
    fn push(&mut self, p: [i32; 3], w: i32, h: i32, dir: u8, c: Rgba) {
        for v in [p[0], p[1], p[2], w, h] {
            debug_assert!(i16::try_from(v).is_ok(), "face coordinate {v} out of i16 range");
            self.bytes.extend_from_slice(&(v as i16).to_le_bytes());
        }
        self.bytes.push(dir);
        self.bytes.push(0);
        self.bytes.extend_from_slice(&c);
        self.count += 1;
    }
}

fn pack(c: Rgba) -> u32 {
    u32::from_le_bytes(c)
}

fn unpack(k: u32) -> Rgba {
    k.to_le_bytes()
}

/// Export every exposed face of `g`. Returns the buffer and the stats
/// (`html_bytes` left at 0).
pub fn export_faces(g: &VoxelGrid) -> Result<(Vec<u8>, ViewerStats)> {
    if g.nx > MAX_EDGE || g.ny > MAX_EDGE || g.nz > MAX_EDGE {
        return Err(anyhow!("grid {}x{}x{} too large for the viewer (max edge {MAX_EDGE} blocks)", g.nx, g.ny, g.nz));
    }
    let mut buf = FaceBuf::default();
    full_faces(g, &mut buf);
    let merged = buf.count;
    partial_faces(g, &mut buf);
    let stats = ViewerStats {
        faces: buf.count,
        merged_faces: merged,
        partial_faces: buf.count - merged,
        buffer_bytes: buf.bytes.len(),
        gzip_bytes: 0,
        html_bytes: 0,
    };
    Ok((buf.bytes, stats))
}

/// Boundary faces of full-cube cells, greedy-merged per slice and colour.
fn full_faces(g: &VoxelGrid, out: &mut FaceBuf) {
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
                    if !g.is_full(m) {
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

/// Faces of every box of every partial-shape cell, one quad each.
fn partial_faces(g: &VoxelGrid, out: &mut FaceBuf) {
    for x in 0..g.nx {
        for z in 0..g.nz {
            for y in 0..g.ny {
                let m = g.get(x, y, z);
                if m == 0 || g.is_full(m) {
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

/// Build the viewer page for `scene` (optionally cropped to `region`, absolute
/// coords) and write it to `out`. The page is self-contained apart from
/// three.js, which it loads from cdnjs. The face buffer is gzipped (the page
/// inflates it with `DecompressionStream`) and embedded as base64.
pub fn write_viewer_html(scene: &Scene, region: Option<Rect3D>, out: &Path) -> Result<ViewerStats> {
    let g = VoxelGrid::from_scene(scene, region).ok_or_else(|| anyhow!("nothing visible to export"))?;
    let (bytes, mut stats) = export_faces(&g)?;
    let mut enc = GzEncoder::new(Vec::with_capacity(bytes.len() / 4), Compression::default());
    enc.write_all(&bytes)?;
    let gz = enc.finish()?;
    stats.gzip_bytes = gz.len();
    let title = match scene.meta.get("seed") {
        Some(seed) => format!("Tome town {seed}"),
        None => "Tome town".to_string(),
    };
    let meta = serde_json::json!({
        "origin": [g.origin.x, g.origin.y, g.origin.z],
        "size": [g.nx, g.ny, g.nz],
        "faces": stats.faces,
        "encoding": "gzip",
        "background": [BACKGROUND[0], BACKGROUND[1], BACKGROUND[2]],
        "title": title,
        "scene_meta": scene.meta,
    });
    // `<` never reaches the page's HTML parser (it would end the JSON <script>).
    let meta_json = meta.to_string().replace('<', "\\u003c");
    let html = HTML_TEMPLATE
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

    fn faces(scene: &Scene) -> (Vec<[i16; 5]>, Vec<u8>, ViewerStats) {
        let g = VoxelGrid::from_scene(scene, None).unwrap();
        let (bytes, stats) = export_faces(&g).unwrap();
        assert_eq!(bytes.len(), stats.faces * RECORD_BYTES);
        let mut recs = Vec::new();
        let mut dirs = Vec::new();
        for r in bytes.chunks(RECORD_BYTES) {
            let v = |i: usize| i16::from_le_bytes([r[i * 2], r[i * 2 + 1]]);
            recs.push([v(0), v(1), v(2), v(3), v(4)]);
            dirs.push(r[10]);
        }
        (recs, dirs, stats)
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
        // Different colours do not merge.
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
        assert!(!html.contains("__DATA__") && !html.contains("__META__") && !html.contains("__TITLE__"));
    }
}
