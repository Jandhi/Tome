//! Lighting glue for the flat views (isometric, top-down, elevations): a
//! [`Lit`] wraps a [`LightSampler`] for one grid and hands each renderer the
//! light of a whole face at once ([`FaceLight`], the face's four smooth-lit
//! corners) so the per-pixel work is a bilinear blend and a multiply
//! ([`tint`]). A renderer given `None` runs its unlit arithmetic untouched.
//!
//! Which cell lights a face follows [`super::light`]'s module doc: the cell
//! on the face's outside, i.e. the neighbour across a boundary face or the
//! block's own cell for a face inside it. Samples behind the first one along
//! a ray (water below its surface, what shows through glass) take their own
//! cell's light. An outside cell beyond the grid (above the topmost block,
//! or past a side, as the front row of an elevation has) counts as open sky,
//! as it does in the map itself.

use super::grid::{Rgba, VoxelGrid};
use super::light::{LightMap, LightSampler, Lighting, Rgb};

/// How much three opaque neighbours darken a face corner: the corner keeps
/// `1 - AO` of its light. The same figure as `light::AO`, so a face lit here
/// matches one lit through [`LightSampler::face_corners_rgb`].
const AO: f32 = 0.6;

/// A light map under one lighting, ready to light faces.
pub struct Lit<'a> {
    map: &'a LightMap,
    sampler: LightSampler<'a>,
    smooth: bool,
    /// Grid size; cells outside it are open sky.
    size: [i32; 3],
    /// Background colour: [`super::grid::BACKGROUND`] under the sky light.
    background: Rgba,
}

impl<'a> Lit<'a> {
    /// `map` must be `g`'s light map ([`LightMap::new`] of the same grid).
    pub fn new(g: &VoxelGrid, map: &'a LightMap, lighting: Lighting) -> Self {
        let sampler = LightSampler::new(map, lighting);
        let background = tint(super::grid::BACKGROUND, sampler.sky_rgb());
        Lit { map, sampler, smooth: lighting.smooth, size: [g.nx, g.ny, g.nz], background }
    }

    fn patch<const T1: usize, const T2: usize>(&self, outside: [i32; 3]) -> [(Rgb, bool, bool); 9] {
        let map = self.map;
        let mut patch = [([0.0f32; 3], false, false); 9];
        for (i, row) in patch.chunks_mut(3).enumerate() {
            for (j, slot) in row.iter_mut().enumerate() {
                let mut c = outside;
                c[T1] += i as i32 - 1;
                c[T2] += j as i32 - 1;
                let solid = map.is_solid(c[0], c[1], c[2]);
                let glows = solid && map.block_light(c[0], c[1], c[2]) > 0;
                *slot = (self.sampler.at_rgb(c), solid, glows);
            }
        }
        patch
    }

    /// The four smooth-lit corners of a face, all three channels in one
    /// pass over the corner neighbourhoods: [`LightMap::face_corners`]'s
    /// rule (each corner averages the outside cell and the up-to-three cells
    /// beside it on the face's outside, opaque cells adding nothing unless
    /// they glow and darkening the corner), which
    /// [`LightSampler::face_corners_rgb`] runs once per channel.
    fn corners(&self, outside: [i32; 3], axis: usize) -> [Rgb; 4] {
        // The 3x3 cells around `outside` in the face's plane, read once:
        // colour, whether the cell is an opaque cube, whether it glows. The
        // tangent axes are compile-time constants per axis so the offsets are.
        let patch = match axis {
            0 => self.patch::<1, 2>(outside),
            1 => self.patch::<0, 2>(outside),
            _ => self.patch::<0, 1>(outside),
        };
        let at = |di: i32, dj: i32| patch[((di + 1) * 3 + (dj + 1)) as usize];
        let mut out = [[0.0; 3]; 4];
        for (ci, (su, sv)) in [(-1, -1), (1, -1), (-1, 1), (1, 1)].into_iter().enumerate() {
            let (o, a, b, c) = (at(0, 0), at(su, 0), at(0, sv), at(su, sv));
            // A corner walled in on both sides is hidden, like a solid cell.
            let sc = c.1 || (a.1 && b.1);
            let (mut sum, mut n) = ([0.0f32; 3], 0);
            // Solid cells add nothing, unless they glow themselves.
            for (cell, s) in [(o, false), (a, a.1), (b, b.1), (c, sc)] {
                if !s || cell.2 {
                    for k in 0..3 {
                        sum[k] += cell.0[k];
                    }
                    n += 1;
                }
            }
            let solids = a.1 as u8 + b.1 as u8 + sc as u8;
            let f = if n == 0 { 0.0 } else { (1.0 - AO * solids as f32 / 3.0) / n as f32 };
            out[ci] = [sum[0] * f, sum[1] * f, sum[2] * f];
        }
        out
    }

    fn inside(&self, cell: [i32; 3]) -> bool {
        (0..3).all(|k| cell[k] >= 0 && cell[k] < self.size[k])
    }

    /// The image background (sky / empty columns) dimmed by the sky light.
    pub fn background(&self) -> Rgba {
        self.background
    }

    /// Coloured light of a cell, for samples that are not a lit face.
    pub fn cell(&self, cell: [i32; 3]) -> Rgb {
        if !self.inside(cell) {
            return self.sampler.sky_rgb();
        }
        self.sampler.at_rgb(cell)
    }

    /// Light of a face whose outside cell is `outside` and whose normal runs
    /// along `axis` (0 x, 1 y, 2 z), interpolated across it when smooth
    /// lighting is on and flat otherwise.
    pub fn face(&self, outside: [i32; 3], axis: usize) -> FaceLight {
        if !self.inside(outside) {
            FaceLight::flat(self.sampler.sky_rgb())
        } else if self.smooth {
            FaceLight::new(self.corners(outside, axis))
        } else {
            FaceLight::flat(self.sampler.at_rgb(outside))
        }
    }
}

/// The light over one face, sampled by face-local `(u, v)` in `0..1` over
/// the two tangent axes in ascending order (x before y before z): bilinear
/// between its four corners, held as the corner `(lo, lo)` plus the
/// differences along `u`, along `v` and across, so a sample is three
/// multiply-adds per channel.
#[derive(Clone, Copy, Debug)]
pub struct FaceLight {
    c0: Rgb,
    du: Rgb,
    dv: Rgb,
    duv: Rgb,
    /// All four corners are equal (open ground, a wall away from anything):
    /// the light is `c0` everywhere, and a caller can colour the face once.
    pub uniform: bool,
}

impl FaceLight {
    /// From corners `(lo, lo), (hi, lo), (lo, hi), (hi, hi)`.
    pub fn new(c: [Rgb; 4]) -> FaceLight {
        let d = |a: Rgb, b: Rgb| [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let uniform = c[1] == c[0] && c[2] == c[0] && c[3] == c[0];
        // Bilinear: c0 + (c1 - c0) u + (c2 - c0) v + ((c3 - c2) - (c1 - c0)) u v.
        FaceLight { c0: c[0], du: d(c[0], c[1]), dv: d(c[0], c[2]), duv: d(d(c[0], c[1]), d(c[2], c[3])), uniform }
    }

    /// The same light everywhere on the face.
    pub fn flat(c: Rgb) -> FaceLight {
        FaceLight { c0: c, du: [0.0; 3], dv: [0.0; 3], duv: [0.0; 3], uniform: true }
    }

    pub fn at(&self, u: f32, v: f32) -> Rgb {
        let uv = u * v;
        let ch = |k: usize| self.c0[k] + self.du[k] * u + self.dv[k] * v + self.duv[k] * uv;
        [ch(0), ch(1), ch(2)]
    }
}

/// Multiply the colour channels by `f` per channel (alpha unchanged),
/// rounding to nearest. This runs once per painted pixel, so it rounds by
/// adding a half and truncating (the cast saturates) rather than through
/// `f32::round`, a library call on some targets.
pub fn tint(c: Rgba, f: Rgb) -> Rgba {
    let s = |v: u8, f: f32| (v as f32 * f + 0.5) as u8;
    [s(c[0], f[0]), s(c[1], f[1]), s(c[2], f[2]), c[3]]
}

/// Axis of a block face's normal: 0 for east/west, 1 for up/down, 2 for north/south.
pub fn axis_of(face: super::assets::Face6) -> usize {
    use super::assets::Face6;
    match face {
        Face6::East | Face6::West => 0,
        Face6::Up | Face6::Down => 1,
        Face6::North | Face6::South => 2,
    }
}

/// Face-local `(u, v)` of a point `p` in cell sixteenths on a face whose
/// normal runs along `axis`: the fractions along the two tangent axes.
pub fn face_uv(axis: usize, p: [f32; 3]) -> (f32, f32) {
    match axis {
        0 => (p[1] / 16.0, p[2] / 16.0),
        1 => (p[0] / 16.0, p[2] / 16.0),
        _ => (p[0] / 16.0, p[1] / 16.0),
    }
}
