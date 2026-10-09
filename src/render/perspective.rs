//! Perspective views: a pinhole camera that can stand anywhere, including
//! inside a room, rendered by casting one ray per pixel through the
//! [`VoxelGrid`] and hitting each block's [`Box16`](super::shape::Box16)
//! shapes. Faces are shaded by direction like the isometric views (top
//! brightest, east/west darkest) with a little distance fog so near and far
//! walls read apart; translucent materials (glass, water) blend front to back.
//! With [`PerspectiveOptions::lighting`] set, every hit is also multiplied by
//! the Minecraft-style light of its face (see [`super::light`]).

use resvg::tiny_skia::{Pixmap, PixmapPaint, Transform};

use super::assets::{BlockGeometry, Face6};
use super::grid::{canvas, shade, RayAcc, Rgba, VoxelGrid, BACKGROUND};
use super::light::{LightMap, LightSampler, Lighting};

#[derive(Debug, Clone, Copy)]
pub struct PerspectiveCamera {
    /// Eye position in absolute world coordinates; block `x` spans `x..x+1`,
    /// so a player standing in cell `(x, y, z)` has eyes near `(x+0.5, y+1.62, z+0.5)`.
    pub eye: [f32; 3],
    /// Point the camera looks at, absolute world coordinates.
    pub target: [f32; 3],
    /// Horizontal field of view in degrees.
    pub fov_deg: f32,
}

/// Brightness of a face by its normal: up, down, east/west, north/south.
const SHADE_TOP: f32 = 1.0;
const SHADE_BOTTOM: f32 = 0.55;
const SHADE_X: f32 = 0.64;
const SHADE_Z: f32 = 0.8;
/// Fog: colours darken by `FOG_AMOUNT` over `FOG_RANGE` blocks, then stay.
const FOG_RANGE: f32 = 24.0;
const FOG_AMOUNT: f32 = 0.35;
/// Step past a cell boundary before locating the start cell.
const EPS: f32 = 1e-4;
/// Step off a hit face along its normal to find the cell on its outside.
const FACE_EPS: f32 = 1e-3;

type V3 = [f32; 3];

/// Options beyond the camera; the default renders exactly as [`render_perspective`].
#[derive(Default, Clone, Copy)]
pub struct PerspectiveOptions<'a> {
    /// Light every hit by `map` (computed once per grid with
    /// [`LightMap::new`]) under the given [`Lighting`]; the sky background
    /// dims with its daylight. `None` leaves the image unlit.
    pub lighting: Option<(&'a LightMap, Lighting)>,
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn length(a: V3) -> f32 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
fn normalize(a: V3) -> V3 {
    let l = length(a);
    if l < 1e-9 { [0.0, 0.0, 1.0] } else { scale(a, 1.0 / l) }
}

/// Render `cam`'s view of `g` as a `width` x `height` image, unlit.
pub fn render_perspective(g: &VoxelGrid, cam: &PerspectiveCamera, width: u32, height: u32) -> anyhow::Result<Pixmap> {
    render_perspective_with(g, cam, width, height, &PerspectiveOptions::default())
}

/// Render `cam`'s view of `g` as a `width` x `height` image with `opts`.
pub fn render_perspective_with(
    g: &VoxelGrid,
    cam: &PerspectiveCamera,
    width: u32,
    height: u32,
    opts: &PerspectiveOptions,
) -> anyhow::Result<Pixmap> {
    let (w, h) = (width.max(1) as usize, height.max(1) as usize);
    let mut pm = canvas(w, h)?;
    let lit = opts.lighting.map(|(map, lighting)| LightSampler::new(map, lighting));

    // Camera basis in grid-local coordinates.
    let local = |p: V3| [p[0] - g.origin.x as f32, p[1] - g.origin.y as f32, p[2] - g.origin.z as f32];
    let eye = local(cam.eye);
    let forward = normalize(sub(local(cam.target), eye));
    let mut right = cross(forward, [0.0, 1.0, 0.0]);
    if length(right) < 1e-4 {
        // Looking straight up or down: any horizontal right vector will do.
        right = [1.0, 0.0, 0.0];
    }
    let right = normalize(right);
    let up = cross(right, forward);
    let half = (cam.fov_deg.clamp(10.0, 170.0).to_radians() / 2.0).tan();
    let aspect = h as f32 / w as f32;

    let data = pm.data_mut();
    for py in 0..h {
        let sy = (1.0 - 2.0 * (py as f32 + 0.5) / h as f32) * half * aspect;
        for px in 0..w {
            let sx = (2.0 * (px as f32 + 0.5) / w as f32 - 1.0) * half;
            let dir = normalize(add(forward, add(scale(right, sx), scale(up, sy))));
            let c = cast(g, eye, dir, lit.as_ref());
            data[(py * w + px) * 4..][..4].copy_from_slice(&c);
        }
    }
    Ok(pm)
}

/// Ray/box slab test. Returns `(t_enter, t_exit, axis, entered_through_max_plane)`
/// when the ray meets the box; `t_enter` may be negative when the origin is inside.
fn slab(o: V3, d: V3, lo: V3, hi: V3) -> Option<(f32, f32, usize, bool)> {
    let (mut t0, mut t1) = (f32::NEG_INFINITY, f32::INFINITY);
    let (mut axis, mut through_max) = (1, true);
    for k in 0..3 {
        if d[k].abs() < 1e-9 {
            if o[k] < lo[k] || o[k] >= hi[k] {
                return None;
            }
            continue;
        }
        let inv = 1.0 / d[k];
        let (mut a, mut b) = ((lo[k] - o[k]) * inv, (hi[k] - o[k]) * inv);
        let mut max_plane = false;
        if a > b {
            std::mem::swap(&mut a, &mut b);
            max_plane = true;
        }
        if a > t0 {
            t0 = a;
            axis = k;
            through_max = max_plane;
        }
        t1 = t1.min(b);
        if t0 > t1 {
            return None;
        }
    }
    Some((t0, t1, axis, through_max))
}

/// Shade for a face entered along `axis`; `through_max` means the ray came in
/// through the box's max plane on that axis (so for Y, it hit the top face).
fn face_shade(axis: usize, through_max: bool) -> f32 {
    match axis {
        0 => SHADE_X,
        1 => if through_max { SHADE_TOP } else { SHADE_BOTTOM },
        _ => SHADE_Z,
    }
}

/// Outward unit normal of the face entered along `axis` through the box's
/// max (or min) plane.
fn axis_normal(axis: usize, through_max: bool) -> V3 {
    let mut n = [0.0; 3];
    n[axis] = if through_max { 1.0 } else { -1.0 };
    n
}

/// One face the ray crosses in a cell.
struct Hit {
    t: f32,
    /// Element index, so coincident faces keep model order.
    idx: usize,
    color: Rgba,
    shade: f32,
    /// World-space face normal; `None` when the eye is inside the box.
    normal: Option<V3>,
}

/// Nearest hit among the cell's boxes.
fn hit_boxes(g: &VoxelGrid, m: u16, cell: [i32; 3], o: V3, d: V3) -> Option<Hit> {
    let mut best: Option<Hit> = None;
    for b in g.boxes(m) {
        let lo = [cell[0] as f32 + b[0] as f32 / 16.0, cell[1] as f32 + b[1] as f32 / 16.0, cell[2] as f32 + b[2] as f32 / 16.0];
        let hi = [cell[0] as f32 + b[3] as f32 / 16.0, cell[1] as f32 + b[4] as f32 / 16.0, cell[2] as f32 + b[5] as f32 / 16.0];
        if let Some((t0, t1, axis, through_max)) = slab(o, d, lo, hi) {
            if t1 < 0.0 {
                continue;
            }
            // Origin inside the box: it fills the view from here.
            let (t, s, n) = if t0 < 0.0 {
                (0.0, SHADE_BOTTOM, None)
            } else {
                (t0, face_shade(axis, through_max), Some(axis_normal(axis, through_max)))
            };
            if best.as_ref().is_none_or(|h| t < h.t) {
                best = Some(Hit { t, idx: 0, color: g.color(m), shade: s, normal: n });
            }
        }
    }
    best
}

/// Face hits of the ray against a textured block model in `cell`, nearest
/// first, with coincident faces (overlays) ordered so later elements paint
/// over earlier ones. Texels with zero alpha are holes (cut-out leaves,
/// panes, plants) and a face the model leaves open lets the ray through, as
/// Minecraft's back-face culling does.
fn hit_geometry(geo: &BlockGeometry, cell: [i32; 3], o: V3, d: V3, fill: Rgba, hits: &mut Vec<Hit>) {
    hits.clear();
    // Work in sixteenths inside the cell; t is converted back to world units.
    let os = [(o[0] - cell[0] as f32) * 16.0, (o[1] - cell[1] as f32) * 16.0, (o[2] - cell[2] as f32) * 16.0];
    for (idx, e) in geo.elements.iter().enumerate() {
        let (oe, de) = match &e.local {
            Some(f) => (f.point(os), f.dir(d)),
            None => (os, d),
        };
        let Some((t0, t1, axis, through_max)) = slab(oe, de, e.from, e.to) else { continue };
        if t1 < 0.0 {
            continue;
        }
        if t0 < 0.0 {
            // The eye is inside this box: it fills the view in the block's colour.
            hits.push(Hit { t: 0.0, idx, color: fill, shade: SHADE_BOTTOM, normal: None });
            continue;
        }
        let face = Face6::entered(axis, through_max);
        let Some((_, ft)) = e.faces.iter().find(|(f, _)| *f == face) else { continue };
        let p = add(oe, scale(de, t0));
        let c = ft.sample_at(p);
        if c[3] == 0 {
            continue;
        }
        let s = if ft.shade { face_shade(axis, through_max) } else { SHADE_TOP };
        // A freely rotated element's normal comes back to world space through
        // the rotation's transpose (its inverse).
        let n = match &e.local {
            Some(f) => {
                let sign = if through_max { 1.0 } else { -1.0 };
                [sign * f.m[axis][0], sign * f.m[axis][1], sign * f.m[axis][2]]
            }
            None => axis_normal(axis, through_max),
        };
        hits.push(Hit { t: t0 / 16.0, idx, color: c, shade: s, normal: Some(n) });
    }
    hits.sort_by(|a, b| a.t.total_cmp(&b.t).then(b.idx.cmp(&a.idx)));
}

/// Light factor for a hit at distance `t` along the ray in `cell`: the
/// face's smooth or flat light from the cell on its outside, found by
/// stepping off the hit point along the normal; inside a box, the cell's own.
fn light_at(lit: &LightSampler, o: V3, d: V3, cell: [i32; 3], hit: &Hit) -> f32 {
    let Some(n) = hit.normal else { return lit.at(cell) };
    let q = add(add(o, scale(d, hit.t)), scale(n, FACE_EPS));
    let outside = [q[0].floor() as i32, q[1].floor() as i32, q[2].floor() as i32];
    let axis = (0..3).max_by(|&a, &b| n[a].abs().total_cmp(&n[b].abs())).unwrap_or(1);
    let (t1, t2) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    lit.face(outside, axis, q[t1] - q[t1].floor(), q[t2] - q[t2].floor())
}

/// Colour seen along the ray from `o` (grid-local) in direction `d`.
fn cast(g: &VoxelGrid, o: V3, d: V3, lit: Option<&LightSampler>) -> Rgba {
    let bg = match lit {
        Some(l) => shade(BACKGROUND, l.sky()),
        None => BACKGROUND,
    };
    let size = [g.nx as f32, g.ny as f32, g.nz as f32];
    let Some((t_in, t_out, _, _)) = slab(o, d, [0.0; 3], size) else {
        return bg;
    };
    if t_out <= 0.0 {
        return bg;
    }
    let t_start = t_in.max(0.0);
    let p = add(o, scale(d, t_start + EPS));
    let mut cell = [0i32; 3];
    for k in 0..3 {
        cell[k] = (p[k].floor() as i32).clamp(0, size[k] as i32 - 1);
    }
    let mut step = [0i32; 3];
    let mut t_next = [f32::INFINITY; 3];
    let mut t_delta = [f32::INFINITY; 3];
    for k in 0..3 {
        if d[k] > 0.0 {
            step[k] = 1;
            t_delta[k] = 1.0 / d[k];
            t_next[k] = ((cell[k] + 1) as f32 - o[k]) / d[k];
        } else if d[k] < 0.0 {
            step[k] = -1;
            t_delta[k] = -1.0 / d[k];
            t_next[k] = (cell[k] as f32 - o[k]) / d[k];
        }
    }

    let mut acc = RayAcc::default();
    let mut hits = Vec::new();
    let fog_at = |t: f32| 1.0 - FOG_AMOUNT * (t / FOG_RANGE).min(1.0);
    // Shade, fog and (when lit) light for a hit: the unlit factor is the
    // same product as ever, so lighting off changes nothing.
    let factor = |hit: &Hit, cell: [i32; 3]| {
        let f = hit.shade * fog_at(hit.t);
        match lit {
            Some(l) => f * light_at(l, o, d, cell, hit),
            None => f,
        }
    };
    loop {
        let m = g.get(cell[0], cell[1], cell[2]);
        if m != 0 {
            if let Some(geo) = g.geometry(m) {
                hit_geometry(geo, cell, o, d, g.color(m), &mut hits);
                for hit in &hits {
                    if acc.push(shade(hit.color, factor(hit, cell))) {
                        return acc.finish(bg);
                    }
                }
            } else if let Some(hit) = hit_boxes(g, m, cell, o, d) {
                if acc.push(shade(hit.color, factor(&hit, cell))) {
                    return acc.finish(bg);
                }
            }
        }
        // Step into the next cell along the axis whose boundary comes first.
        let k = if t_next[0] < t_next[1] {
            if t_next[0] < t_next[2] { 0 } else { 2 }
        } else if t_next[1] < t_next[2] {
            1
        } else {
            2
        };
        cell[k] += step[k];
        if t_next[k] > t_out || cell[k] < 0 || cell[k] >= size[k] as i32 {
            break;
        }
        t_next[k] += t_delta[k];
    }
    acc.finish(bg)
}

/// Tile equally sized images left to right, top to bottom, `cols` per row,
/// with `gap` pixels of background between them.
pub fn contact_sheet(tiles: &[Pixmap], cols: usize, gap: u32) -> anyhow::Result<Pixmap> {
    let cols = cols.max(1);
    let (tw, th) = tiles.iter().fold((1u32, 1u32), |(w, h), t| (w.max(t.width()), h.max(t.height())));
    let rows = tiles.len().div_ceil(cols).max(1);
    let w = cols as u32 * tw + (cols as u32 + 1) * gap;
    let h = rows as u32 * th + (rows as u32 + 1) * gap;
    let mut sheet = canvas(w as usize, h as usize)?;
    for (i, tile) in tiles.iter().enumerate() {
        let x = gap + (i % cols) as u32 * (tw + gap);
        let y = gap + (i / cols) as u32 * (th + gap);
        sheet.draw_pixmap(x as i32, y as i32, tile.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    }
    Ok(sheet)
}
