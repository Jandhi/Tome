//! Texture sampling for the flat views (isometric, top-down, elevations).
//!
//! A material's block model ([`BlockGeometry`]) is usable by these views when
//! every element is world-oriented; models with freely rotated elements
//! (crops, flowers, levers) keep the flat colour path, as do blocks without a
//! model. The views look along one axis, so each one only ever sees faces of
//! one kind (upward faces from above, the facing side faces from the side);
//! those faces are rasterised once per material into box-filtered `px` x `px`
//! "stamps" of the cell, ordered nearest-first, and a pixel's colour is the
//! first stamp texel with any alpha along its ray. Texels with zero alpha are
//! holes the ray passes through (leaves, panes, trapdoor windows), as in the
//! perspective view.

use super::assets::{BlockGeometry, Element, Face6, FaceTex, V3};
use super::grid::{Rgba, VoxelGrid};

/// A material's model as the flat views use it.
pub struct FlatModel<'a> {
    pub geo: &'a BlockGeometry,
    /// The block is opaque and its model holds a full cube whose six faces
    /// are fully opaque, so it hides whatever lies behind it like a plain
    /// opaque block; anything else may show what is behind through holes.
    pub solid: bool,
}

/// Per-material models for the flat views: `None` for materials without a
/// model and for models with rotated elements.
pub fn flat_models(g: &VoxelGrid) -> Vec<Option<FlatModel<'_>>> {
    (0..g.material_count())
        .map(|m| {
            let geo = g.geometry(m as u16)?;
            if geo.elements.iter().any(|e| e.local.is_some()) {
                return None;
            }
            Some(FlatModel { geo, solid: g.is_opaque(m as u16) && is_solid(geo) })
        })
        .collect()
}

fn is_solid(geo: &BlockGeometry) -> bool {
    geo.elements.iter().any(|e| {
        let full = e.from.iter().all(|&v| v <= 0.0) && e.to.iter().all(|&v| v >= 16.0);
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
/// `z = j` (north up, east right).
pub fn top_stamps(geo: &BlockGeometry, px: usize) -> Vec<(f32, Vec<Rgba>)> {
    let mut faces: Vec<(f32, usize, &Element, &FaceTex)> = geo
        .elements
        .iter()
        .enumerate()
        .filter_map(|(i, e)| face(e, Face6::Up).map(|ft| (e.to[1], i, e, ft)))
        .collect();
    faces.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)));
    faces
        .into_iter()
        .map(|(h, _, e, ft)| {
            let raster = stamp(px, |fi, fj| {
                let (x, z) = (16.0 * fi, 16.0 * fj);
                let inside = x >= e.from[0] && x < e.to[0] && z >= e.from[2] && z < e.to[2];
                inside.then(|| ft.sample_at([x, h, z]))
            });
            (h, raster)
        })
        .collect()
}

/// Stamps of the model's faces in direction `f` (a side) seen from that
/// side, nearest the viewer first. Raster pixel `(i, j)` runs left to right
/// across the face as the viewer sees it (`flip` mirrors the lateral axis,
/// for sides where world coordinates run right to left) and top to bottom.
pub fn side_stamps(geo: &BlockGeometry, f: Face6, px: usize, flip: bool) -> Vec<Vec<Rgba>> {
    // Lateral axis, depth axis, and whether the viewer is on the axis' max side.
    let (lat, depth, toward_max) = match f {
        Face6::South => (0, 2, true),
        Face6::North => (0, 2, false),
        Face6::East => (2, 0, true),
        Face6::West => (2, 0, false),
        Face6::Up | Face6::Down => return Vec::new(),
    };
    let mut faces: Vec<(f32, usize, &Element, &FaceTex)> = geo
        .elements
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let plane = if toward_max { e.to[depth] } else { e.from[depth] };
            let key = if toward_max { plane } else { -plane };
            face(e, f).map(|ft| (key, i, e, ft))
        })
        .collect();
    faces.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)));
    faces
        .into_iter()
        .map(|(key, _, e, ft)| {
            let plane = if toward_max { key } else { -key };
            stamp(px, |fi, fj| {
                let l = 16.0 * if flip { 1.0 - fi } else { fi };
                let y = 16.0 * (1.0 - fj);
                if l < e.from[lat] || l >= e.to[lat] || y < e.from[1] || y >= e.to[1] {
                    return None;
                }
                let mut p: V3 = [0.0; 3];
                p[lat] = l;
                p[1] = y;
                p[depth] = plane;
                Some(ft.sample_at(p))
            })
        })
        .collect()
}
