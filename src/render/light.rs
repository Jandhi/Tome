//! Minecraft-style lighting for the software renderers: a [`LightMap`] holds
//! sky light and block light (0..15) for every cell of a [`VoxelGrid`],
//! computed with the game's rules, and turns them into a brightness factor a
//! renderer multiplies a face's colour by. Lighting is off everywhere unless a
//! renderer is handed a [`Lighting`].
//!
//! Rules (the game's, reduced to what the grid knows):
//! - Sky light is 15 in every cell nothing blocks from above and travels
//!   straight down undiminished; from there it spreads sideways and down into
//!   shade losing 1 per block.
//! - Block light starts at an emitter's level ([`emission`]) and loses 1 per
//!   block. Emitters are a table in this file ([`EMITTERS`] plus the few
//!   state-dependent families below it): adding one is a one-line change.
//! - An opaque full cube stops light and holds none. A partial opaque block
//!   (slab, stairs, door) stops light only through the faces its shape covers
//!   completely, so a slab or stair roof still darkens the attic under it
//!   while a fence or torch lets light past. Translucent blocks (glass,
//!   water, leaves) pass light; water and leaves have an opacity of 1, so
//!   they dim even the sky light falling straight down (air costs nothing
//!   there) and otherwise cost the usual 1 per block.
//!
//! How a face should be lit (for any renderer, flat views included): take
//! the cell on the face's *outside* — the neighbour across the face when the
//! face lies on the cell boundary, the block's own cell when the face is
//! inside it (a bottom slab's top) — and multiply the face's colour, after
//! its directional shade, by [`LightMap::brightness`] of that cell; or, for
//! smooth lighting, by [`LightMap::face_light_at`], which interpolates the
//! face's four corners, each the average of the up-to-four cells touching
//! the corner on the outside with opaque neighbours darkening it (the
//! game's smooth lighting / ambient occlusion). [`LightSampler`] bundles a
//! map with a [`Lighting`] and a brightness lookup table for the inner loop.

use anyhow::anyhow;

use super::grid::VoxelGrid;
use super::scene::Scene;
use super::shape::{self, Box16};

/// Brightness at light level 0. Not pitch black, so a cellar still reads.
pub const AMBIENT: f32 = 0.12;
/// Mix between the game's raw brightness ramp and its gamma-boosted version
/// (the in-game "Brightness" slider at its default, halfway).
const GAMMA: f32 = 0.5;
/// How much three opaque neighbours darken a face corner: the corner keeps
/// `1 - AO` of its light (the game steps 1.0, 0.8, 0.6, 0.4).
const AO: f32 = 0.6;

/// An RGB multiplier, each channel 0..1.
pub type Rgb = [f32; 3];

/// Colour of full block light (torches, lanterns): the game's warm orange.
pub const BLOCK_TINT: Rgb = [1.0, 0.86, 0.64];
/// Colour of sky light at night (moonlight), blended toward white by day.
pub const NIGHT_SKY_TINT: Rgb = [0.60, 0.70, 1.0];

/// Lighting settings a renderer applies when it lights at all.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lighting {
    /// Scale on the sky channel: 1.0 at noon, [`Lighting::NIGHT`] (0.22) for
    /// a moonlit night, as the game dims open sky to about level 4 at night.
    pub daylight: f32,
    /// Interpolate light across each face from its corners (the game's smooth
    /// lighting, with opaque neighbours darkening corners); `false` lights a
    /// whole face by its outside cell.
    pub smooth: bool,
    /// Colour the light like the game: block light warm ([`BLOCK_TINT`]),
    /// sky light white by day and blue at night ([`NIGHT_SKY_TINT`]). `false`
    /// keeps every channel at the plain brightness.
    pub tinted: bool,
}

impl Lighting {
    pub const DAY: f32 = 1.0;
    pub const DUSK: f32 = 0.5;
    pub const NIGHT: f32 = 0.22;

    pub fn day() -> Lighting {
        Lighting { daylight: Self::DAY, smooth: true, tinted: true }
    }

    pub fn dusk() -> Lighting {
        Lighting { daylight: Self::DUSK, smooth: true, tinted: true }
    }

    pub fn night() -> Lighting {
        Lighting { daylight: Self::NIGHT, smooth: true, tinted: true }
    }

    /// Colour of sky light under this daylight: white at noon, moonlight blue
    /// at night, between the two at dusk.
    pub fn sky_tint(&self) -> Rgb {
        if !self.tinted {
            return [1.0; 3];
        }
        let d = self.daylight.clamp(0.0, 1.0);
        [
            NIGHT_SKY_TINT[0] + (1.0 - NIGHT_SKY_TINT[0]) * d,
            NIGHT_SKY_TINT[1] + (1.0 - NIGHT_SKY_TINT[1]) * d,
            NIGHT_SKY_TINT[2] + (1.0 - NIGHT_SKY_TINT[2]) * d,
        ]
    }

    /// Colour of block light under this lighting.
    pub fn block_tint(&self) -> Rgb {
        if self.tinted { BLOCK_TINT } else { [1.0; 3] }
    }

    /// Coloured brightness of a packed `(block << 4) | sky` level: each
    /// channel takes the brighter of the tinted sky and block contributions,
    /// so a lamp-lit room glows warm and a moonlit yard goes blue, while a
    /// sunlit face stays white.
    pub fn rgb_of(&self, packed: u8) -> Rgb {
        let sky = LightMap::curve((packed & 15) as f32 * self.daylight);
        let block = LightMap::curve((packed >> 4) as f32);
        let (st, bt) = (self.sky_tint(), self.block_tint());
        [(sky * st[0]).max(block * bt[0]), (sky * st[1]).max(block * bt[1]), (sky * st[2]).max(block * bt[2])]
    }

    /// `day`, `night`, `dusk` or a daylight factor in `0..=1` (smooth lighting on).
    pub fn parse(spec: &str) -> anyhow::Result<Lighting> {
        let daylight = match spec {
            "day" => Self::DAY,
            "night" => Self::NIGHT,
            "dusk" => Self::DUSK,
            s => s
                .parse::<f32>()
                .ok()
                .filter(|d| (0.0..=1.0).contains(d))
                .ok_or_else(|| anyhow!("--light takes day, night, dusk or a daylight factor in 0..1, not {spec:?}"))?,
        };
        Ok(Lighting { daylight, smooth: true, tinted: true })
    }

    /// Short name for file and directory names: `day`, `night`, `dusk` or `light0.30`.
    pub fn tag(&self) -> String {
        if self.daylight == Self::DAY {
            "day".into()
        } else if self.daylight == Self::NIGHT {
            "night".into()
        } else if self.daylight == Self::DUSK {
            "dusk".into()
        } else {
            format!("light{:.2}", self.daylight)
        }
    }
}

impl Default for Lighting {
    fn default() -> Self {
        Lighting::day()
    }
}

/// What a block does to light, from its block string; stored per material by
/// the [`VoxelGrid`] (the id is gone once interned).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LightProps {
    /// Light level the block emits, 0 for most.
    pub emit: u8,
    /// The block's opacity: light passing through loses `max(1, filter)`
    /// per block instead of 1, and sky light falling straight down loses
    /// `filter` instead of nothing (water, leaves 1; tinted glass 15).
    pub filter: u8,
    /// Passes light although it is drawn opaque (leaves).
    pub transparent: bool,
}

/// When a table entry emits: always, only with `lit=true` (unlit by default,
/// like a candle or furnace), or unless `lit=false` (lit by default, like a
/// campfire or redstone torch).
#[derive(Clone, Copy)]
enum When {
    Always,
    Lit,
    LitDefault,
}

/// Light-emitting blocks: `(id without namespace, level, when)`. Families
/// whose level depends on more state (candles, copper bulbs, cave vines,
/// the `light` block, respawn anchors, sea pickles) are handled in
/// [`emission`] after this table misses.
const EMITTERS: &[(&str, u8, When)] = &[
    ("torch", 14, When::Always),
    ("wall_torch", 14, When::Always),
    ("soul_torch", 10, When::Always),
    ("soul_wall_torch", 10, When::Always),
    ("redstone_torch", 7, When::LitDefault),
    ("redstone_wall_torch", 7, When::LitDefault),
    ("lantern", 15, When::Always),
    ("soul_lantern", 10, When::Always),
    ("campfire", 15, When::LitDefault),
    ("soul_campfire", 10, When::LitDefault),
    ("fire", 15, When::Always),
    ("soul_fire", 10, When::Always),
    ("lava", 15, When::Always),
    ("lava_cauldron", 15, When::Always),
    ("glowstone", 15, When::Always),
    ("sea_lantern", 15, When::Always),
    ("shroomlight", 15, When::Always),
    ("jack_o_lantern", 15, When::Always),
    ("beacon", 15, When::Always),
    ("conduit", 15, When::Always),
    ("end_rod", 14, When::Always),
    ("end_portal", 15, When::Always),
    ("end_portal_frame", 1, When::Always),
    ("nether_portal", 11, When::Always),
    ("ochre_froglight", 15, When::Always),
    ("verdant_froglight", 15, When::Always),
    ("pearlescent_froglight", 15, When::Always),
    ("redstone_lamp", 15, When::Lit),
    ("furnace", 13, When::Lit),
    ("blast_furnace", 13, When::Lit),
    ("smoker", 13, When::Lit),
    ("redstone_ore", 9, When::Lit),
    ("deepslate_redstone_ore", 9, When::Lit),
    ("crying_obsidian", 10, When::Always),
    ("enchanting_table", 7, When::Always),
    ("ender_chest", 7, When::Always),
    ("glow_lichen", 7, When::Always),
    ("sculk_catalyst", 6, When::Always),
    ("amethyst_cluster", 5, When::Always),
    ("large_amethyst_bud", 4, When::Always),
    ("magma_block", 3, When::Always),
    ("medium_amethyst_bud", 2, When::Always),
    ("small_amethyst_bud", 1, When::Always),
    ("brewing_stand", 1, When::Always),
    ("brown_mushroom", 1, When::Always),
    ("dragon_egg", 1, When::Always),
    ("sculk_sensor", 1, When::Always),
];

/// Light each candle of a lit candle block gives.
const CANDLE_LIGHT: u8 = 3;

/// Blocks with an opacity above 0 that still let light through: `(id,
/// opacity)`. Leaves (any `*_leaves`) have opacity 1 and are handled by suffix.
const FILTERS: &[(&str, u8)] = &[
    ("water", 1),
    ("bubble_column", 1),
    ("cobweb", 1),
    ("slime_block", 1),
    ("honey_block", 1),
    ("tinted_glass", 15),
];

/// Block id without namespace or state: `minecraft:candle[lit=true]` -> `candle`.
fn base_id(block: &str) -> &str {
    let id = Scene::strip_state(block);
    id.strip_prefix("minecraft:").unwrap_or(id)
}

/// Light level a block emits (0 for most), honouring `lit`, `candles`,
/// oxidation and the other states that matter.
pub fn emission(block: &str) -> u8 {
    let name = base_id(block);
    let lit = |default: bool| match shape::state(block, "lit") {
        Some(v) => v == "true",
        None => default,
    };
    let when = |level: u8, when: When| match when {
        When::Always => level,
        When::Lit if lit(false) => level,
        When::LitDefault if lit(true) => level,
        _ => 0,
    };
    if let Some(&(_, level, w)) = EMITTERS.iter().find(|e| e.0 == name) {
        return when(level, w);
    }
    if name == "candle" || name.ends_with("_candle") {
        let count: u8 = shape::state(block, "candles").and_then(|c| c.parse().ok()).unwrap_or(1);
        return when(CANDLE_LIGHT.saturating_mul(count.clamp(1, 4)), When::Lit);
    }
    if name.ends_with("candle_cake") {
        return when(CANDLE_LIGHT, When::Lit);
    }
    if let Some(rest) = name.strip_suffix("copper_bulb") {
        let level = match rest.strip_prefix("waxed_").unwrap_or(rest) {
            "" => 15,
            "exposed_" => 12,
            "weathered_" => 8,
            "oxidized_" => 4,
            _ => 0,
        };
        return when(level, When::Lit);
    }
    if name.starts_with("cave_vines") {
        return if shape::state(block, "berries") == Some("true") { 14 } else { 0 };
    }
    match name {
        "light" => shape::state(block, "level").and_then(|l| l.parse().ok()).unwrap_or(15),
        "respawn_anchor" => match shape::state(block, "charges").and_then(|c| c.parse::<u8>().ok()).unwrap_or(0) {
            0 => 0,
            c => (c.min(4) * 4).saturating_sub(1),
        },
        "sea_pickle" => {
            let n: u8 = shape::state(block, "pickles").and_then(|p| p.parse().ok()).unwrap_or(1);
            if shape::state(block, "waterlogged") == Some("false") { 0 } else { 6 + 3 * (n.clamp(1, 4) - 1) }
        }
        _ => 0,
    }
}

/// Emission and filtering of a block string.
pub fn light_props(block: &str) -> LightProps {
    let name = base_id(block);
    let emit = emission(block);
    if name.ends_with("_leaves") {
        return LightProps { emit, filter: 1, transparent: true };
    }
    let filter = FILTERS.iter().find(|f| f.0 == name).map_or(0, |f| f.1);
    LightProps { emit, filter, transparent: false }
}

// Face bits, in `Face6` order: a block fully covering a face stops light through it.
const DOWN: u8 = 1;
const UP: u8 = 2;
const NORTH: u8 = 4;
const SOUTH: u8 = 8;
const WEST: u8 = 16;
const EAST: u8 = 32;
const ALL_FACES: u8 = 63;

/// The six neighbour directions: offset, the face light leaves through, the
/// face it enters the neighbour through.
const DIRS: [([i32; 3], u8, u8); 6] = [
    ([0, -1, 0], DOWN, UP),
    ([0, 1, 0], UP, DOWN),
    ([0, 0, -1], NORTH, SOUTH),
    ([0, 0, 1], SOUTH, NORTH),
    ([-1, 0, 0], WEST, EAST),
    ([1, 0, 0], EAST, WEST),
];

/// Faces of the cell that `boxes` cover completely.
fn occlusion(boxes: &[Box16]) -> u8 {
    let mut mask = 0;
    // (face bit, normal axis, plane in sixteenths, the two tangent axes)
    for (bit, axis, plane, a, b) in [
        (DOWN, 1, 0, 0, 2),
        (UP, 1, 16, 0, 2),
        (NORTH, 2, 0, 0, 1),
        (SOUTH, 2, 16, 0, 1),
        (WEST, 0, 0, 1, 2),
        (EAST, 0, 16, 1, 2),
    ] {
        let mut rows = [0u16; 16];
        for bx in boxes {
            if (plane == 0 && bx[axis] != 0) || (plane == 16 && bx[axis + 3] != 16) {
                continue;
            }
            for r in bx[b]..bx[b + 3] {
                for c in bx[a]..bx[a + 3] {
                    rows[r as usize] |= 1 << c;
                }
            }
        }
        if rows.iter().all(|&r| r == u16::MAX) {
            mask |= bit;
        }
    }
    mask
}

/// Per-material light behaviour.
#[derive(Clone, Copy, Default)]
struct Mat {
    /// Opaque full cube: stops light, holds none (unless it emits).
    solid: bool,
    /// Faces light cannot pass through.
    occ: u8,
    emit: u8,
    filter: u8,
}

impl Mat {
    fn of(g: &VoxelGrid, m: u16) -> Mat {
        if m == 0 {
            return Mat::default();
        }
        let lp = g.light_props(m);
        let opaque = g.is_opaque(m) && !lp.transparent;
        let solid = opaque && g.is_full(m);
        let occ = if !opaque {
            0
        } else if solid {
            ALL_FACES
        } else {
            occlusion(g.boxes(m))
        };
        Mat { solid, occ, emit: lp.emit, filter: lp.filter }
    }
}

/// Sky and block light per cell of a grid, in the grid's local coordinates.
pub struct LightMap {
    nx: i32,
    ny: i32,
    nz: i32,
    /// Sky light in the low nibble, block light in the high nibble; the grid's
    /// cell order (`(x * nz + z) * ny + y`).
    levels: Vec<u8>,
    /// Opaque full cubes, one bit per cell.
    solid: Vec<u64>,
    /// Cells that emit light.
    pub emitters: usize,
}

impl LightMap {
    /// Compute both channels for `g`. Open sky above the grid; its sides are closed.
    pub fn new(g: &VoxelGrid) -> LightMap {
        let (nx, ny, nz) = (g.nx, g.ny, g.nz);
        let n = (nx as usize) * (ny as usize) * (nz as usize);
        let cells = g.cells();
        let mats: Vec<Mat> = (0..g.material_count()).map(|m| Mat::of(g, m as u16)).collect();
        let mut solid = vec![0u64; n.div_ceil(64)];
        for (i, &c) in cells.iter().enumerate() {
            if mats[c as usize].solid {
                solid[i >> 6] |= 1 << (i & 63);
            }
        }
        let mut lm = LightMap { nx, ny, nz, levels: vec![0; n], solid, emitters: 0 };
        let mut buckets: [Vec<u32>; 16] = Default::default();

        // Block light: every emitter is a source at its own level.
        for (i, &c) in cells.iter().enumerate() {
            let e = mats[c as usize].emit;
            if e > 0 {
                lm.levels[i] = e << 4;
                lm.emitters += 1;
                if e > 1 {
                    buckets[e as usize].push(i as u32);
                }
            }
        }
        lm.spread::<4>(&mats, cells, &mut buckets);

        // Sky light: each column is 15 from the top down to the first thing
        // that stops or dims it. `bottom` is the lowest y of that run (ny when
        // the column has none).
        let ny_u = ny as usize;
        let mut bottom = vec![ny; (nx * nz) as usize];
        for col in 0..(nx * nz) as usize {
            let base = col * ny_u;
            let mut y = ny - 1;
            let mut b = ny;
            while y >= 0 {
                let i = base + y as usize;
                let m = &mats[cells[i] as usize];
                if m.solid || m.occ & UP != 0 {
                    break;
                }
                let l = 15u8.saturating_sub(m.filter);
                lm.levels[i] |= l;
                if l < 15 {
                    // Dimmed (water, leaves): from here on it spreads like any light.
                    if l > 1 {
                        buckets[l as usize].push(i as u32);
                    }
                    break;
                }
                b = y;
                if m.occ & DOWN != 0 {
                    break;
                }
                y -= 1;
            }
            bottom[col] = b;
        }
        // Sources: the cells of each run that face a cell outside any run —
        // the run's bottom cell, and the side faces where a neighbouring
        // column's run ends higher up.
        for x in 0..nx {
            for z in 0..nz {
                let col = (x * nz + z) as usize;
                let b = bottom[col];
                if b >= ny {
                    continue;
                }
                let mut hi = if b > 0 { b + 1 } else { b };
                for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let (xx, zz) = (x + dx, z + dz);
                    if xx >= 0 && xx < nx && zz >= 0 && zz < nz {
                        hi = hi.max(bottom[(xx * nz + zz) as usize]);
                    }
                }
                for y in b..hi.min(ny) {
                    buckets[15].push((col * ny_u + y as usize) as u32);
                }
            }
        }
        lm.spread::<0>(&mats, cells, &mut buckets);
        lm
    }

    /// Flood the channel stored at bit `SHIFT` outward from the queued
    /// sources, highest level first, so every cell is settled once.
    fn spread<const SHIFT: u8>(&mut self, mats: &[Mat], cells: &[u16], buckets: &mut [Vec<u32>; 16]) {
        let (ny, nz) = (self.ny, self.nz);
        let strides: [isize; 6] = [-1, 1, -(ny as isize), ny as isize, -((ny * nz) as isize), (ny * nz) as isize];
        let mask = 15u8 << SHIFT;
        for level in (2..=15u8).rev() {
            let bucket = std::mem::take(&mut buckets[level as usize]);
            for &i in &bucket {
                let i = i as usize;
                if (self.levels[i] & mask) >> SHIFT != level {
                    continue;
                }
                let y = (i % ny as usize) as i32;
                let t = i / ny as usize;
                let (z, x) = ((t % nz as usize) as i32, (t / nz as usize) as i32);
                let from = mats[cells[i] as usize];
                for (k, (d, out_face, in_face)) in DIRS.iter().enumerate() {
                    let (xx, yy, zz) = (x + d[0], y + d[1], z + d[2]);
                    if xx < 0 || yy < 0 || zz < 0 || xx >= self.nx || yy >= ny || zz >= nz {
                        continue;
                    }
                    // A solid emitter radiates through all its faces.
                    if !from.solid && from.occ & out_face != 0 {
                        continue;
                    }
                    let j = (i as isize + strides[k]) as usize;
                    let to = mats[cells[j] as usize];
                    if to.solid || to.occ & in_face != 0 {
                        continue;
                    }
                    // The game's cost per block: 1, or the block's own opacity if more.
                    let nl = level.saturating_sub(to.filter.max(1));
                    if nl > (self.levels[j] & mask) >> SHIFT {
                        self.levels[j] = (self.levels[j] & !mask) | (nl << SHIFT);
                        if nl > 1 {
                            buckets[nl as usize].push(j as u32);
                        }
                    }
                }
            }
        }
    }

    /// Index of the cell nearest to `(x, y, z)` (coordinates clamp into the grid).
    #[inline]
    fn clamp_index(&self, x: i32, y: i32, z: i32) -> usize {
        let (x, y, z) = (x.clamp(0, self.nx - 1), y.clamp(0, self.ny - 1), z.clamp(0, self.nz - 1));
        ((x * self.nz + z) * self.ny + y) as usize
    }

    /// Sky light 0..15 at a local cell (clamped into the grid).
    pub fn sky_light(&self, x: i32, y: i32, z: i32) -> u8 {
        self.levels[self.clamp_index(x, y, z)] & 15
    }

    /// Block light 0..15 at a local cell (clamped into the grid).
    pub fn block_light(&self, x: i32, y: i32, z: i32) -> u8 {
        self.levels[self.clamp_index(x, y, z)] >> 4
    }

    /// `true` when the cell holds an opaque full cube; `false` outside the grid.
    pub fn is_solid(&self, x: i32, y: i32, z: i32) -> bool {
        if x < 0 || y < 0 || z < 0 || x >= self.nx || y >= self.ny || z >= self.nz {
            return false;
        }
        let i = ((x * self.nz + z) * self.ny + y) as usize;
        self.solid[i >> 6] & (1 << (i & 63)) != 0
    }

    /// Effective light level of a packed cell: the sky channel scaled by
    /// `daylight`, or the block channel, whichever is brighter.
    #[inline]
    fn level(packed: u8, daylight: f32) -> f32 {
        ((packed & 15) as f32 * daylight).max((packed >> 4) as f32)
    }

    /// Brightness 0..1 of a light level 0..15: the game's ramp
    /// `x / (4 - 3x)` mixed half-way with its gamma-boosted form, over an
    /// ambient floor so level 0 stays visible.
    pub fn curve(level: f32) -> f32 {
        let x = (level / 15.0).clamp(0.0, 1.0);
        let r = x / (4.0 - 3.0 * x);
        let g = 1.0 - (1.0 - r).powi(4);
        AMBIENT + (1.0 - AMBIENT) * (r + (g - r) * GAMMA)
    }

    /// Brightness 0..1 of a cell under `daylight` (1.0 noon, 0.22 night).
    pub fn brightness(&self, x: i32, y: i32, z: i32, daylight: f32) -> f32 {
        Self::curve(Self::level(self.levels[self.clamp_index(x, y, z)], daylight))
    }

    /// Smooth lighting: the light at the four corners of a face whose outside
    /// cell is `outside` and whose normal runs along `axis` (0 x, 1 y, 2 z).
    /// Corners are ordered `(lo, lo), (hi, lo), (lo, hi), (hi, hi)` over the
    /// two tangent axes in ascending order (x before y before z). Each corner
    /// averages the cells touching it on the face's outside; opaque cells
    /// contribute no light and darken the corner.
    pub fn face_corners(&self, outside: [i32; 3], axis: usize, daylight: f32) -> [f32; 4] {
        self.corners_with(outside, axis, |i| Self::curve(Self::level(self.levels[i], daylight)))
    }

    /// Smooth light at `(u, v)` in `0..1` across the face (see [`face_corners`](Self::face_corners)).
    pub fn face_light_at(&self, outside: [i32; 3], axis: usize, u: f32, v: f32, daylight: f32) -> f32 {
        bilinear(self.face_corners(outside, axis, daylight), u, v)
    }

    fn corners_with(&self, outside: [i32; 3], axis: usize, bright: impl Fn(usize) -> f32) -> [f32; 4] {
        let (t1, t2) = match axis {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        let mut out = [0.0; 4];
        for (ci, (su, sv)) in [(-1, -1), (1, -1), (-1, 1), (1, 1)].into_iter().enumerate() {
            let mut a = outside;
            a[t1] += su;
            let mut b = outside;
            b[t2] += sv;
            let mut c = a;
            c[t2] += sv;
            let sa = self.is_solid(a[0], a[1], a[2]);
            let sb = self.is_solid(b[0], b[1], b[2]);
            // A corner walled in on both sides is hidden, like a solid cell.
            let sc = self.is_solid(c[0], c[1], c[2]) || (sa && sb);
            let (mut sum, mut n) = (0.0, 0);
            for (cell, s) in [(outside, false), (a, sa), (b, sb), (c, sc)] {
                let i = self.clamp_index(cell[0], cell[1], cell[2]);
                // Solid cells add nothing, unless they glow themselves.
                if !s || self.levels[i] >> 4 > 0 {
                    sum += bright(i);
                    n += 1;
                }
            }
            let solids = sa as u8 + sb as u8 + sc as u8;
            out[ci] = if n == 0 { 0.0 } else { sum / n as f32 * (1.0 - AO * solids as f32 / 3.0) };
        }
        out
    }

    /// How many non-solid cells sit at each sky and block level: `(sky, block)`.
    pub fn histogram(&self) -> ([usize; 16], [usize; 16]) {
        let (mut sky, mut block) = ([0; 16], [0; 16]);
        for (i, &p) in self.levels.iter().enumerate() {
            if self.solid[i >> 6] & (1 << (i & 63)) == 0 {
                sky[(p & 15) as usize] += 1;
                block[(p >> 4) as usize] += 1;
            }
        }
        (sky, block)
    }

    /// Number of cells.
    pub fn len(&self) -> usize {
        self.levels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.levels.is_empty()
    }
}

fn bilinear(c: [f32; 4], u: f32, v: f32) -> f32 {
    let (u, v) = (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
    c[0] * (1.0 - u) * (1.0 - v) + c[1] * u * (1.0 - v) + c[2] * (1.0 - u) * v + c[3] * u * v
}

/// A [`LightMap`] under one [`Lighting`], with the brightness of every
/// packed level pre-computed, for a renderer's inner loop.
pub struct LightSampler<'a> {
    map: &'a LightMap,
    /// Plain brightness per packed level.
    lut: [f32; 256],
    /// Coloured brightness per packed level (see [`Lighting::rgb_of`]).
    lut_rgb: [Rgb; 256],
    smooth: bool,
    sky: f32,
    sky_rgb: Rgb,
}

impl<'a> LightSampler<'a> {
    pub fn new(map: &'a LightMap, lighting: Lighting) -> Self {
        let mut lut = [0.0; 256];
        let mut lut_rgb = [[0.0; 3]; 256];
        for p in 0..256 {
            lut[p] = LightMap::curve(LightMap::level(p as u8, lighting.daylight));
            lut_rgb[p] = lighting.rgb_of(p as u8);
        }
        let sky = LightMap::curve(15.0 * lighting.daylight);
        let st = lighting.sky_tint();
        LightSampler { map, lut, lut_rgb, smooth: lighting.smooth, sky, sky_rgb: [sky * st[0], sky * st[1], sky * st[2]] }
    }

    /// Brightness of open sky under this lighting (for backgrounds).
    pub fn sky(&self) -> f32 {
        self.sky
    }

    /// Colour of open sky under this lighting (for backgrounds).
    pub fn sky_rgb(&self) -> Rgb {
        self.sky_rgb
    }

    /// Brightness of a cell (coordinates clamp into the grid).
    pub fn at(&self, cell: [i32; 3]) -> f32 {
        self.lut[self.map.levels[self.map.clamp_index(cell[0], cell[1], cell[2])] as usize]
    }

    /// Coloured light of a cell (coordinates clamp into the grid).
    pub fn at_rgb(&self, cell: [i32; 3]) -> Rgb {
        self.lut_rgb[self.map.levels[self.map.clamp_index(cell[0], cell[1], cell[2])] as usize]
    }

    /// Light for a face: smooth-interpolated at `(u, v)` across the face from
    /// its corners, or the outside cell's brightness when smooth lighting is off.
    pub fn face(&self, outside: [i32; 3], axis: usize, u: f32, v: f32) -> f32 {
        if !self.smooth {
            return self.at(outside);
        }
        bilinear(self.map.corners_with(outside, axis, |i| self.lut[self.map.levels[i] as usize]), u, v)
    }

    /// Coloured light for a face, interpolated like [`face`](Self::face).
    pub fn face_rgb(&self, outside: [i32; 3], axis: usize, u: f32, v: f32) -> Rgb {
        if !self.smooth {
            return self.at_rgb(outside);
        }
        let mut out = [0.0; 3];
        for (k, o) in out.iter_mut().enumerate() {
            *o = bilinear(self.map.corners_with(outside, axis, |i| self.lut_rgb[self.map.levels[i] as usize][k]), u, v);
        }
        out
    }

    /// The four corner colours of a face (see [`LightMap::face_corners`]),
    /// for renderers that interpolate on their own, such as a vertex-coloured mesh.
    pub fn face_corners_rgb(&self, outside: [i32; 3], axis: usize) -> [Rgb; 4] {
        let mut out = [[0.0; 3]; 4];
        for k in 0..3 {
            let c = self.map.corners_with(outside, axis, |i| self.lut_rgb[self.map.levels[i] as usize][k]);
            for (ci, v) in c.into_iter().enumerate() {
                out[ci][k] = v;
            }
        }
        out
    }
}
