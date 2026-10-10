//! Texture sampling for the flat views (isometric, top-down, elevations).
//!
//! Every material with a block model ([`BlockGeometry`]) is drawn from it;
//! blocks without one keep the flat colour path. The views look along one
//! axis, so each one only ever sees faces of one kind (upward faces from
//! above, the facing side faces from the side); those faces are rasterised
//! once per material into box-filtered `px` x `px` "stamps" of the cell,
//! ordered nearest-first, and a pixel's colour is the first stamp texel with
//! any alpha along its ray. Texels with zero alpha are holes the ray passes
//! through (leaves, panes, trapdoor windows), as in the perspective view.
//!
//! A model with freely rotated elements (a banner or sign at a rotation
//! between the four sides, torches, levers) has no faces lying in the
//! view's planes, so its stamps are cast instead: orthographic rays through
//! the elements ([`ray_hits`]), the n-th face each sub-sample's ray meets
//! going into the n-th stamp ([`peel`]).

use super::assets::{BlockGeometry, Element, Face6, FaceTex, V3};
use super::grid::{Rgba, VoxelGrid};
use super::perspective::slab;

/// A material's model as the flat views use it.
pub struct FlatModel<'a> {
    pub geo: &'a BlockGeometry,
    /// The block is opaque and its model holds a full cube whose six faces
    /// are fully opaque, so it hides whatever lies behind it like a plain
    /// opaque block; anything else may show what is behind through holes.
    pub solid: bool,
}

/// Per-material models for the flat views: `None` for materials without a
/// model.
pub fn flat_models(g: &VoxelGrid) -> Vec<Option<FlatModel<'_>>> {
    (0..g.material_count())
        .map(|m| {
            let geo = g.geometry(m as u16)?;
            Some(FlatModel { geo, solid: g.is_opaque(m as u16) && is_solid(geo) })
        })
        .collect()
}

/// Whether the model has freely rotated elements, which the flat views
/// draw by casting rays ([`ray_hits`]) instead of stamping aligned faces.
pub fn is_rotated(geo: &BlockGeometry) -> bool {
    geo.elements.iter().any(|e| e.local.is_some())
}

fn is_solid(geo: &BlockGeometry) -> bool {
    geo.elements.iter().any(|e| {
        let full = e.local.is_none() && e.from.iter().all(|&v| v <= 0.0) && e.to.iter().all(|&v| v >= 16.0);
        full && Face6::ALL.iter().all(|&f| face(e, f).is_some_and(|ft| ft.texture.avg[3] == 255))
    })
}

/// The element's face in direction `f`, if the model has one.
pub fn face(e: &Element, f: Face6) -> Option<&FaceTex> {
    e.faces.iter().find(|(k, _)| *k == f).map(|(_, ft)| ft)
}

/// A `px` x `px` raster (row-major) of one face of the cell. Pixel `(i, j)`
/// box-filters `sample(fi, fj)` over the pixel, `fi` and `fj` being
/// fractions across and down the raster; `None` samples lie outside the face
/// and count as transparent, so a face covering part of a pixel gets partial
/// alpha and blends with what is behind.
pub fn stamp(px: usize, sample: impl Fn(f32, f32) -> Option<Rgba>) -> Vec<Rgba> {
    // One sub-sample per texel while the raster is coarser than the texture.
    let ss = (16usize).div_ceil(px).clamp(1, 16);
    let n = (ss * ss) as f32;
    let mut out = Vec::with_capacity(px * px);
    for j in 0..px {
        for i in 0..px {
            let (mut rgb, mut a_sum) = ([0.0f32; 3], 0.0f32);
            for sj in 0..ss {
                for si in 0..ss {
                    let fi = (i as f32 + (si as f32 + 0.5) / ss as f32) / px as f32;
                    let fj = (j as f32 + (sj as f32 + 0.5) / ss as f32) / px as f32;
                    if let Some(c) = sample(fi, fj) {
                        let a = c[3] as f32;
                        for k in 0..3 {
                            rgb[k] += c[k] as f32 * a;
                        }
                        a_sum += a;
                    }
                }
            }
            out.push(average(rgb, a_sum, n));
        }
    }
    out
}

/// Straight-alpha mean of `n` samples accumulated as alpha-weighted colour
/// sums `rgb` with total alpha `a_sum`.
pub fn average(rgb: [f32; 3], a_sum: f32, n: f32) -> Rgba {
    let alpha = (a_sum / n).round() as u8;
    if alpha == 0 || a_sum <= 0.0 {
        return [0, 0, 0, 0];
    }
    let ch = |k: usize| (rgb[k] / a_sum).round().clamp(0.0, 255.0) as u8;
    [ch(0), ch(1), ch(2), alpha]
}

/// Stamps of the model's upward faces seen from above: `(face height in
/// sixteenths, raster)`, highest first; among coincident faces the later
/// element (an overlay) comes first. Raster pixel `(i, j)` is at `x = i`,
/// `z = j` (north up, east right). Faces above the cell (a pot's neck)
/// are left to the cell above, see [`top_stamps_in`].
pub fn top_stamps(geo: &BlockGeometry, px: usize) -> Vec<(f32, Vec<Rgba>)> {
    top_stamps_in(geo, px, [0; 3])
}

/// [`top_stamps`] of the part of the model lying in another cell, `off`
/// being the model's own cell minus that one (in cells): the raster covers
/// that cell's footprint and the heights are that cell's, so a model
/// reaching into a neighbour is drawn there piece by piece (see
/// [`in_cell`]).
pub fn top_stamps_in(geo: &BlockGeometry, px: usize, off: [i32; 3]) -> Vec<(f32, Vec<Rgba>)> {
    if is_rotated(geo) {
        // Straight down from above the cell; the key is the hit's height.
        return peel(geo, px, off, |fi, fj| ([16.0 * fi, 64.0, 16.0 * fj], [0.0, -1.0, 0.0]), |p| p[1]);
    }
    let shift = off.map(|o| 16.0 * o as f32);
    let mut faces: Vec<(f32, usize, &Element, &FaceTex)> = geo
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| in_cell(e.to[1] + shift[1], off[1]))
        .filter_map(|(i, e)| face(e, Face6::Up).map(|ft| (e.to[1], i, e, ft)))
        .collect();
    faces.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)));
    faces
        .into_iter()
        .map(|(h, _, e, ft)| {
            let raster = stamp(px, |fi, fj| {
                let (x, z) = (16.0 * fi - shift[0], 16.0 * fj - shift[2]);
                let inside = x >= e.from[0] && x < e.to[0] && z >= e.from[2] && z < e.to[2];
                inside.then(|| ft.sample_at([x, h, z]))
            });
            (h + shift[1], raster)
        })
        .collect()
}

/// Whether a face whose plane lies at `p` sixteenths along its normal axis
/// in some cell is drawn by that cell, `off` being the model's own cell minus
/// that cell along the axis. The model's own cell keeps every face on its
/// boundary; a cell it reaches into takes those strictly inside it plus the
/// ones on its far boundary, so no face is drawn twice.
pub fn in_cell(p: f32, off: i32) -> bool {
    const E: f32 = 1e-3;
    match off {
        0 => (-E..=16.0 + E).contains(&p),
        o if o > 0 => (-E..16.0 - E).contains(&p),
        _ => p > E && p <= 16.0 + E,
    }
}

/// Stamps of the model's faces in direction `f` (a side) seen from that
/// side, nearest the viewer first, each with its plane's distance toward the
/// viewer in sixteenths (larger is nearer). Raster pixel `(i, j)` runs left
/// to right across the face as the viewer sees it (`flip` mirrors the
/// lateral axis, for sides where world coordinates run right to left) and top
/// to bottom. `off` is the model's own cell minus the cell drawn (as in
/// [`top_stamps_in`]): `[0; 3]` for the model's own cell, which keeps the
/// parts inside it, another to draw the part that reaches into a neighbour.
pub fn side_stamps(geo: &BlockGeometry, f: Face6, px: usize, flip: bool, off: [i32; 3]) -> Vec<(f32, Vec<Rgba>)> {
    // Lateral axis, depth axis, and whether the viewer is on the axis' max side.
    let (lat, depth, toward_max) = match f {
        Face6::South => (0, 2, true),
        Face6::North => (0, 2, false),
        Face6::East => (2, 0, true),
        Face6::West => (2, 0, false),
        Face6::Up | Face6::Down => return Vec::new(),
    };
    if is_rotated(geo) {
        // From the viewer's side of the cell toward it; the key is the hit's
        // distance toward the viewer.
        let sign = if toward_max { 1.0 } else { -1.0 };
        let ray = |fi: f32, fj: f32| {
            let mut o: V3 = [0.0; 3];
            o[lat] = 16.0 * if flip { 1.0 - fi } else { fi };
            o[1] = 16.0 * (1.0 - fj);
            o[depth] = 8.0 + 56.0 * sign;
            let mut d: V3 = [0.0; 3];
            d[depth] = -sign;
            (o, d)
        };
        return peel(geo, px, off, ray, |p| sign * p[depth]);
    }
    let shift = off.map(|o| 16.0 * o as f32);
    let mut faces: Vec<(f32, usize, &Element, &FaceTex)> = geo
        .elements
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let plane = if toward_max { e.to[depth] } else { e.from[depth] };
            if !in_cell(plane + shift[depth], off[depth]) {
                return None;
            }
            let key = if toward_max { plane } else { -plane };
            face(e, f).map(|ft| (key, i, e, ft))
        })
        .collect();
    faces.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)));
    faces
        .into_iter()
        .map(|(key, _, e, ft)| {
            let plane = if toward_max { key } else { -key };
            let raster = stamp(px, |fi, fj| {
                let l = 16.0 * if flip { 1.0 - fi } else { fi } - shift[lat];
                let y = 16.0 * (1.0 - fj) - shift[1];
                if l < e.from[lat] || l >= e.to[lat] || y < e.from[1] || y >= e.to[1] {
                    return None;
                }
                let mut p: V3 = [0.0; 3];
                p[lat] = l;
                p[1] = y;
                p[depth] = plane;
                Some(ft.sample_at(p))
            });
            (key + if toward_max { shift[depth] } else { -shift[depth] }, raster)
        })
        .collect()
}

/// A face a ray enters: distance along the ray, the texel there, the face's
/// world normal, whether it takes directional shade, and the point hit in
/// the model's cell (sixteenths).
pub struct RayHit {
    pub t: f32,
    pub color: Rgba,
    pub normal: V3,
    pub shade: bool,
    pub p: V3,
}

/// Every face of `geo` the ray `o + t d` (the model's cell, sixteenths)
/// enters, nearest first, as the perspective view finds them: one entry
/// face per element (a face the model leaves open lets the ray through),
/// zero-alpha texels skipped, coincident faces with the later element first.
pub fn ray_hits(geo: &BlockGeometry, o: V3, d: V3, hits: &mut Vec<(usize, RayHit)>) {
    hits.clear();
    for (idx, e) in geo.elements.iter().enumerate() {
        let (oe, de) = match &e.local {
            Some(f) => (f.point(o), f.dir(d)),
            None => (o, d),
        };
        let Some((t0, t1, axis, through_max)) = slab(oe, de, e.from, e.to) else { continue };
        if t1 < 0.0 || t0 < 0.0 {
            continue;
        }
        let Some(ft) = face(e, Face6::entered(axis, through_max)) else { continue };
        let pe = [oe[0] + de[0] * t0, oe[1] + de[1] * t0, oe[2] + de[2] * t0];
        let color = ft.sample_at(pe);
        if color[3] == 0 {
            continue;
        }
        // A rotated element's normal comes back to world space through the
        // rotation's transpose (its inverse).
        let sign = if through_max { 1.0 } else { -1.0 };
        let normal = match &e.local {
            Some(f) => [sign * f.m[axis][0], sign * f.m[axis][1], sign * f.m[axis][2]],
            None => {
                let mut n = [0.0; 3];
                n[axis] = sign;
                n
            }
        };
        let p = [o[0] + d[0] * t0, o[1] + d[1] * t0, o[2] + d[2] * t0];
        hits.push((idx, RayHit { t: t0, color, normal, shade: ft.shade, p }));
    }
    hits.sort_by(|a, b| a.1.t.total_cmp(&b.1.t).then(b.0.cmp(&a.0)));
}

/// Whether point `p` (sixteenths of the drawn cell) lies in that cell, `off`
/// being the model's cell minus it ([`in_cell`] on every axis).
pub fn point_in_cell(p: V3, off: [i32; 3]) -> bool {
    (0..3).all(|k| in_cell(p[k], off[k]))
}

/// Stamps of a rotated model by casting rays, in the stamp layout of
/// [`top_stamps_in`] / [`side_stamps`]: `ray(fi, fj)` gives the origin and
/// direction (drawn cell, sixteenths) of the ray through raster fraction
/// `(fi, fj)`, starting outside anything the model can reach. Each
/// sub-sample's ray keeps the faces it meets inside the drawn cell, and its
/// n-th goes into the n-th stamp, box-filtered per pixel; a stamp's key is
/// the mean of `key(hit point)` over its hits.
pub fn peel(
    geo: &BlockGeometry,
    px: usize,
    off: [i32; 3],
    ray: impl Fn(f32, f32) -> (V3, V3),
    key: impl Fn(V3) -> f32,
) -> Vec<(f32, Vec<Rgba>)> {
    let shift = off.map(|o| 16.0 * o as f32);
    let ss = (16usize).div_ceil(px).clamp(1, 16);
    let n = (ss * ss) as f32;
    // Per stamp: alpha-weighted colour sums and alpha per pixel, key sum and count.
    let mut sums: Vec<(Vec<([f32; 3], f32)>, f32, usize)> = Vec::new();
    let mut hits = Vec::new();
    for j in 0..px {
        for i in 0..px {
            for sj in 0..ss {
                for si in 0..ss {
                    let fi = (i as f32 + (si as f32 + 0.5) / ss as f32) / px as f32;
                    let fj = (j as f32 + (sj as f32 + 0.5) / ss as f32) / px as f32;
                    let (o, d) = ray(fi, fj);
                    ray_hits(geo, [o[0] - shift[0], o[1] - shift[1], o[2] - shift[2]], d, &mut hits);
                    let inside = hits.iter().filter_map(|(_, h)| {
                        let p = [h.p[0] + shift[0], h.p[1] + shift[1], h.p[2] + shift[2]];
                        point_in_cell(p, off).then_some((p, h.color))
                    });
                    for (layer, (p, c)) in inside.enumerate() {
                        if sums.len() <= layer {
                            sums.push((vec![([0.0; 3], 0.0); px * px], 0.0, 0));
                        }
                        let (pixels, key_sum, count) = &mut sums[layer];
                        let (rgb, a_sum) = &mut pixels[j * px + i];
                        let a = c[3] as f32;
                        for k in 0..3 {
                            rgb[k] += c[k] as f32 * a;
                        }
                        *a_sum += a;
                        *key_sum += key(p);
                        *count += 1;
                    }
                }
            }
        }
    }
    sums.into_iter()
        .map(|(pixels, key_sum, count)| {
            let raster = pixels.into_iter().map(|(rgb, a_sum)| average(rgb, a_sum, n)).collect();
            (key_sum / count.max(1) as f32, raster)
        })
        .collect()
}
