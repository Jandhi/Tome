//! Deck addition · **Masts + spars** — the keel-stepped poles and the spars that carry
//! the sails.
//!
//! - **Masts:** logs, keel-stepped from the bottom (`y = 0`), ~hull-length tall, count
//!   from the [`SizeTier`] (1 / 2 / 3 / 3). Optional forward lean (`0` = vertical).
//! - **Spars (this pass):**
//!   - **Main yards** — horizontal **slab** cross-pieces stacked up each mast, carrying
//!     the larger (square) sails; widest at the bottom, narrowing upward.
//!   - **Top finial** — a couple of **fences** straight on top of the mast.
//!   - **Aft stay** — a **slab/stair** spar running aft (toward the stern) from the
//!     aftmost masthead, for the aft (spanker) sail.
//!
//! Sails themselves are the next step; they hang off these.

use std::collections::HashMap;

use crate::generator::materials::{MaterialPlacer, Placer};
use crate::geometry::Point3D;
use crate::minecraft::{string_to_block, Block, BlockForm};

use super::super::palette::ShipPart;
use super::super::tuning::{
    BULWARK_HEIGHT,
    FLAG_COLORS, FLAG_HOIST_HEIGHT, FLAG_MAX_LEN, FLAG_MIN_LEN, FLAG_WAVE_AMP_Y, FLAG_WAVE_AMP_Z,
    FLAG_WAVE_FREQ_Y, FLAG_WAVE_FREQ_Z, FLAG_WAVE_Z_PHASE, JIB_CHANCE_HUGE, JIB_CHANCE_LARGE,
    JIB_CHANCE_MEDIUM, JIB_CURVE_FRAC, JIB_CURVE_MAX, JIB_FOOT_HANGER_COUNT, JIB_HEAD_RIGGING,
    MAST_GAFF_STAIR_FACE, MAST_HEIGHT_FACTOR,
    MAST_MAX_YARDS, MAST_NEST_CHANCE, MAST_NEST_HALF, MAST_NEST_HEIGHT_FRACTION,
    MAST_NEST_MIN_HEIGHT, MAST_NEST_PLATFORM_HALF, MAST_NEST_YARD_GAP, MAST_SAIL_GROWTH,
    MAST_SAIL_TOP_HEIGHT, MAST_SPANKER_BOOM_CLEARANCE, MAST_SPANKER_BOOM_FRACTION,
    MAST_SPANKER_CHANCE, MAST_SPANKER_GAFF_RUN_FRACTION, MAST_SPANKER_LUFF_FRACTION, MAST_STAY_THICK,
    MAST_TOP_FENCE, MAST_TOP_FENCE_MULTI,
    MAST_TOP_YARD_DROP_H1, MAST_TOP_YARD_DROP_H2, MAST_YARD_FORWARD, MAST_YARD_HALF_FRACTION,
    MAST_YARD_NARROW_MAX, MAST_YARD_SPAN_PER_SAIL, SAIL_BELLY_POW, SAIL_BIG_HALF_WIDTH,
    SAIL_BILLOW_DIR, SAIL_BLOCK, SAIL_COMBINED_EDGE, SAIL_CURTAIN_CURVE_POW,
    SAIL_CURTAIN_SIZE_GAIN, SAIL_FOOT_CLEARANCE, SAIL_JIB_FOOT_RAISE, SAIL_JIB_WIND_FACTOR,
    SAIL_SPANKER_FOOT_LIFT, SAIL_SPANKER_WIND_FACTOR,
};
use super::super::{ShipDir, ShipCtx};
use super::SizeTier;
use super::{DeckContext, DeckState, RiggingMaterial, SailBillow, SailState};

/// A horizontal main yard: centred on the mast at `(x, y)`, spanning `±half_width` in z.
/// `sail_height` is how far its sail hangs below it (down toward the next yard / deck).
#[derive(Debug, Clone, Copy)]
pub struct Yard {
    pub x: i32,
    pub y: i32,
    pub half_width: i32,
    pub sail_height: i32,
}

/// One spanker spar cell: a slab (boom, flat) or a stair (gaff, 45° double-stairs).
#[derive(Debug, Clone, Copy)]
pub struct SparCell {
    pub local: Point3D,
    pub form: BlockForm,
    /// `true` = top slab / upside-down stair; `false` = bottom slab / right-side-up stair.
    pub top_half: bool,
    /// Stair facing (gaff only).
    pub facing: Option<ShipDir>,
}

/// The gaff-rigged spanker on the aftmost mast: a near-horizontal **boom** (slabs) along
/// the foot and a **gaff** rising aft at 45° (double-stairs) along the head, in the
/// centreline (`z = 0`) plane. `sail` is the flat (`z = 0`) canvas region bounded by the
/// boom (foot), the gaff + leech (head/aft) and the mast (luff) — billowed to leeward when
/// deployed (`Full`).
#[derive(Debug, Clone)]
pub struct Spanker {
    pub boom: Vec<SparCell>,
    pub gaff: Vec<SparCell>,
    pub sail: Vec<Point3D>,
}

/// A crow's nest / mast platform: a slab floor (mast through the centre) ringed by a
/// fence basket.
#[derive(Debug, Clone)]
pub struct Nest {
    pub floor: Vec<Point3D>,
    pub rail: Vec<Point3D>,
}

/// A masthead pennant: a short wool ribbon streaming **aft** off the mast's finial,
/// staggered in `y` and `z` so it reads as cloth flapping in the wind rather than a flat
/// plane. Placed as wool in [`build`].
#[derive(Debug, Clone)]
pub struct Flag {
    pub cells: Vec<Point3D>,
}

/// One mast and its spars (local frame).
#[derive(Debug, Clone)]
pub struct Mast {
    /// Keel-stepped base (`z = 0`, `y = 0`).
    pub base_x: i32,
    pub height: i32,
    /// The pole's log cells.
    pub cells: Vec<Point3D>,
    /// Horizontal main yards (slabs), bottom (widest) → top.
    pub yards: Vec<Yard>,
    /// Fence finial straight on top.
    pub top_fence: Vec<Point3D>,
    /// Masthead pennant streaming aft off the finial.
    pub flag: Flag,
    /// Gaff-rigged spanker — only on the aftmost mast (and only by chance).
    pub spanker: Option<Spanker>,
    /// Platforms: an unfenced intermediate platform on tall masts + a fenced top crow's
    /// nest at the tallest mast's top.
    pub nests: Vec<Nest>,
    /// Local Y the **lowest** sail's foot sits at — clear of the deck **and its railing**
    /// (see `deck_clearance` in [`build_masts_model`]). The render hangs the lowest sail
    /// down to here; the yard layout reserves the room.
    pub sail_foot_base: i32,
}

/// Pure-geometry masts in the local frame.
#[derive(Debug, Clone)]
pub struct MastModel {
    pub masts: Vec<Mast>,
}

/// `(x-fraction of length, height-fraction of the main mast)` per mast, fore → aft. The
/// mainmast (1.0) is the tallest; the fore/mizzen are shorter.
fn layout(count: i32) -> &'static [(f32, f32)] {
    match count {
        ..=1 => &[(0.50, 1.0)],
        2 => &[(0.30, 0.85), (0.70, 1.0)],
        _ => &[(0.15, 0.90), (0.50, 1.0), (0.85, 0.80)], // 3 (fore, main, mizzen)
    }
}

/// One **top** slab (the boom).
fn boom_slab(x: i32, y: i32) -> SparCell {
    SparCell { local: Point3D::new(x, y, 0), form: BlockForm::Slab, top_half: true, facing: None }
}
/// A gaff stair. The gaff rises aft, so an **upside-down** stair faces opposite a
/// **right-side-up** one — the two meet into one continuous 45° diagonal (stairs on both
/// sides, the same trick as the bowsprit spar).
fn gaff_stair(x: i32, y: i32, top_half: bool) -> SparCell {
    let facing = if top_half { MAST_GAFF_STAIR_FACE.opposite() } else { MAST_GAFF_STAIR_FACE };
    SparCell { local: Point3D::new(x, y, 0), form: BlockForm::Stairs, top_half, facing: Some(facing) }
}

/// Build the gaff-rigged spanker at the aftmost mast (`base_x`, height `mast_h`),
/// `weather_y` = weather deck. A near-horizontal **boom** of slabs runs aft along the
/// foot; a **gaff** rises aft from the throat at a smooth **45°** built from double-stairs.
fn build_spanker(base_x: i32, mast_h: i32, weather_y: i32, length: i32) -> Spanker {
    let boom_len = ((length as f32) * MAST_SPANKER_BOOM_FRACTION).round().max(4.0) as i32;
    let boom_y = weather_y + MAST_SPANKER_BOOM_CLEARANCE;

    // Boom: horizontal slabs aft of the mast.
    let boom = (1..=boom_len).map(|i| boom_slab(base_x - i, boom_y)).collect();

    // Gaff: throat partway up the mast above the boom; rises aft at 45° via double-stairs.
    let throat_y = boom_y + (((mast_h - boom_y) as f32) * MAST_SPANKER_LUFF_FRACTION).round().max(2.0) as i32;
    let gaff_run = ((boom_len as f32) * MAST_SPANKER_GAFF_RUN_FRACTION).round().max(3.0) as i32;
    let mut gaff = Vec::new();
    let mut y = throat_y;
    for i in 0..gaff_run {
        let x = base_x - i; // aft
        // 45° step: upside-down stair (top of this cell) + right-side-up stair (bottom of
        // the one above) → a smooth diagonal beveled on both faces.
        gaff.push(gaff_stair(x, y, true));
        gaff.push(gaff_stair(x, y + 1, false));
        y += 1;
    }

    // Sail region (flat, z = 0): the quadrilateral bounded by the **boom** (foot), the
    // **gaff** then the **leech** (head/aft) and the **mast** (luff). Each column spans from
    // the foot up to the gaff (forward of the peak) or the leech (aft of the peak). The
    // **foot arcs upward** in the centre (`SAIL_SPANKER_FOOT_LIFT`, 0 at the two corners) so
    // the bottom edge lifts off the boom — the wind pushing the sail up. Billowed to leeward
    // at placement; the boom/gaff cells themselves stay as the spars.
    let peak_x = base_x - (gaff_run - 1);
    let peak_y = throat_y + (gaff_run - 1);
    let clew_x = base_x - boom_len; // boom's aft end
    let foot_center = (clew_x + base_x - 1) as f32 / 2.0;
    let foot_half = ((base_x - 1 - clew_x) as f32 / 2.0).max(1.0);
    let mut sail = Vec::new();
    for x in clew_x..=(base_x - 1) {
        let leech_top = if x >= peak_x {
            throat_y + (base_x - x) // under the gaff (rises 1 per block aft)
        } else {
            // leech: straight from the peak down to the clew (boom's aft end)
            let span = (peak_x - clew_x).max(1);
            boom_y + ((peak_y - boom_y) as f32 * (x - clew_x) as f32 / span as f32).round() as i32
        };
        // Keep at least one canvas row above the boom along its whole length, so the boom's
        // aft end (clew) is under sail instead of a bare spar tip — the leech still rises to
        // the gaff tip above it.
        let top = leech_top.max(boom_y + 1);
        // Foot lift: a parabola, 0 at the corners → max in the centre.
        let t = (x as f32 - foot_center) / foot_half; // -1..1 across the boom
        let lift = (SAIL_SPANKER_FOOT_LIFT as f32 * (1.0 - t * t)).round().max(0.0) as i32;
        let bottom = (boom_y + lift).min(top);
        for y in bottom..=top {
            sail.push(Point3D::new(x, y, 0));
        }
    }

    Spanker { boom, gaff, sail }
}

/// Build the masthead pennant: a `length`-block wool ribbon streaming **downwind** from
/// the finial top at `(staff_x, staff_top)`. `stream_sign` is the local-x direction it
/// flies — `-1` aft (at rest / furled), `+1` forward toward the bow (when set sails are
/// drawing the wind from astern), so the flag and the sails read as the same wind. Each
/// step drops a short column whose baseline ripples up/down (`y`) and side-to-side (`z`)
/// on two out-of-phase sine waves whose amplitude **grows toward the free (fly) end** —
/// the hoist is pinned to the staff, the fly whips. The `y`/`z` stagger keeps it a curved
/// 3-D ribbon, never a flat plane.
fn build_flag(staff_x: i32, staff_top: i32, length: i32, phase: f32, stream_sign: i32) -> Flag {
    let mut cells = Vec::new();
    // Each column always steps 1 block downwind (`x`). To keep the ribbon a single
    // connected sheet (every block has a neighbour within a 3×3), the `y`/`z` ripple may
    // only step **±1 per column** toward its target — cloth can't teleport — so consecutive
    // columns are always Chebyshev-1 neighbours and never leave a floating block.
    let (mut cur_y, mut cur_z) = (0, 0);
    for i in 0..length {
        let frac = if length > 1 { i as f32 / (length - 1) as f32 } else { 0.0 };
        // Amplitude grows from the pinned hoist (0) to the whipping fly (frac = 1).
        let target_y = (frac * FLAG_WAVE_AMP_Y * (phase + i as f32 * FLAG_WAVE_FREQ_Y).sin()).round() as i32;
        let target_z = (frac * FLAG_WAVE_AMP_Z
            * (phase + FLAG_WAVE_Z_PHASE + i as f32 * FLAG_WAVE_FREQ_Z).sin())
        .round() as i32;
        cur_y += (target_y - cur_y).clamp(-1, 1);
        cur_z += (target_z - cur_z).clamp(-1, 1);
        // Column height tapers from the hoist body down to a single block at the fly tip.
        let col_h = ((FLAG_HOIST_HEIGHT as f32) * (1.0 - frac) + frac).round().max(1.0) as i32;
        let x = staff_x + stream_sign * (1 + i); // one block off the staff, streaming on
        for h in 0..col_h {
            cells.push(Point3D::new(x, staff_top + cur_y - h, cur_z));
        }
    }
    Flag { cells }
}

/// Billow-depth field for a deployed square sail spanning `z ∈ [-hw, hw]`, `y ∈ [bottom_y,
/// top_y]`, in the chosen [`SailBillow`] shape. Returns `(bulge, ny)` where
/// `bulge[zi * ny + yi]` is the forward depth (blocks) of the cell at `z = zi − hw`,
/// `y = bottom_y + yi`.
///
/// Both shapes end **relaxed to a 1-block gradient** (each cell clamped to its lowest
/// pinned-neighbour + 1) so neighbouring cells never differ by more than 1 in `x` — the
/// sheet is a single hole-free surface (every block has a neighbour within its 3×3, no
/// see-through gaps at an angle).
///
/// - [`SailBillow::Domed`]: target = parabola across the width × `sin` down the height,
///   pinned to 0 at **every** edge; relaxed in 2-D. Curve across the width, straight sides.
/// - [`SailBillow::Curtain`]: a 1-D profile down the height (deep flat middle, drastic
///   ends — `sin^p` with `p < 1`, depth scaled up a touch for wide sails), pinned only at
///   the head/foot and **broadcast across the width**, so every row is flat and the whole
///   length (sides included) curves.
pub fn billow_field(
    hw: i32,
    bottom_y: i32,
    top_y: i32,
    wind: f32,
    shape: SailBillow,
) -> (Vec<i32>, usize) {
    let nz = (2 * hw + 1) as usize;
    let drop = (top_y - bottom_y).max(1);
    let ny = (top_y - bottom_y + 1) as usize;
    let at = |zi: usize, yi: usize| zi * ny + yi;
    let mut b = vec![0i32; nz * ny];

    match shape {
        SailBillow::Domed => {
            for (zi, z) in (-hw..=hw).enumerate() {
                let tz = z as f32 / hw.max(1) as f32; // -1..1 across the width
                let shape_z = 1.0 - tz * tz; // 0 at the luff edges, 1 at the centre
                for yi in 0..ny {
                    let y = bottom_y + yi as i32;
                    let t = (top_y - y) as f32 / drop as f32; // 0 head → 1 foot
                    let shape_y = (std::f32::consts::PI * t).sin().powf(SAIL_BELLY_POW);
                    b[at(zi, yi)] = (wind * shape_z * shape_y).round().max(0.0) as i32;
                }
            }
            // 2-D relax: every edge pinned (exterior = 0).
            for _ in 0..(nz + ny) {
                let mut changed = false;
                for zi in 0..nz {
                    for yi in 0..ny {
                        let left = if zi > 0 { b[at(zi - 1, yi)] } else { 0 };
                        let right = if zi + 1 < nz { b[at(zi + 1, yi)] } else { 0 };
                        let down = if yi > 0 { b[at(zi, yi - 1)] } else { 0 };
                        let up = if yi + 1 < ny { b[at(zi, yi + 1)] } else { 0 };
                        let ceil = left.min(right).min(down).min(up) + 1;
                        if b[at(zi, yi)] > ceil {
                            b[at(zi, yi)] = ceil;
                            changed = true;
                        }
                    }
                }
                if !changed {
                    break;
                }
            }
        }
        SailBillow::Curtain => {
            // Larger sails curve a touch deeper.
            let amp = wind + ((hw - SAIL_BIG_HALF_WIDTH).max(0) as f32) * SAIL_CURTAIN_SIZE_GAIN;
            let mut prof = vec![0i32; ny];
            for yi in 0..ny {
                let t = yi as f32 / (ny - 1).max(1) as f32; // 0 foot .. 1 head (sin is symmetric)
                prof[yi] = (amp * (std::f32::consts::PI * t).sin().powf(SAIL_CURTAIN_CURVE_POW))
                    .round()
                    .max(0.0) as i32;
            }
            // 1-D relax down the height: pinned only at the head/foot (exterior = 0); the
            // sides are free, so the whole row carries the same depth and curves with it.
            for _ in 0..ny {
                let mut changed = false;
                for yi in 0..ny {
                    let down = if yi > 0 { prof[yi - 1] } else { 0 };
                    let up = if yi + 1 < ny { prof[yi + 1] } else { 0 };
                    let ceil = down.min(up) + 1;
                    if prof[yi] > ceil {
                        prof[yi] = ceil;
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
            }
            for zi in 0..nz {
                for yi in 0..ny {
                    b[at(zi, yi)] = prof[yi]; // flat rows — broadcast across the width
                }
            }
        }
        SailBillow::Combined => {
            // Domed `sin`×parabola belly (deepest at the centre), but the across-width factor
            // only falls to `SAIL_COMBINED_EDGE` at the luff edges (not 0), and those edges
            // are left **free** in the relax (like the curtain) — so the sides billow partway
            // instead of pinning flat. Head/foot stay pinned.
            for (zi, z) in (-hw..=hw).enumerate() {
                let tz = z as f32 / hw.max(1) as f32;
                let parab = 1.0 - tz * tz; // 1 centre → 0 edges
                let shape_z = SAIL_COMBINED_EDGE + (1.0 - SAIL_COMBINED_EDGE) * parab; // edge..1
                for yi in 0..ny {
                    let y = bottom_y + yi as i32;
                    let t = (top_y - y) as f32 / drop as f32;
                    let shape_y = (std::f32::consts::PI * t).sin().powf(SAIL_BELLY_POW);
                    b[at(zi, yi)] = (wind * shape_z * shape_y).round().max(0.0) as i32;
                }
            }
            // Relax: head/foot pinned (y exterior = 0), luff sides free (z exterior ignored).
            const FREE: i32 = 1 << 20;
            for _ in 0..(nz + ny) {
                let mut changed = false;
                for zi in 0..nz {
                    for yi in 0..ny {
                        let left = if zi > 0 { b[at(zi - 1, yi)] } else { FREE };
                        let right = if zi + 1 < nz { b[at(zi + 1, yi)] } else { FREE };
                        let down = if yi > 0 { b[at(zi, yi - 1)] } else { 0 };
                        let up = if yi + 1 < ny { b[at(zi, yi + 1)] } else { 0 };
                        let ceil = left.min(right).min(down).min(up) + 1;
                        if b[at(zi, yi)] > ceil {
                            b[at(zi, yi)] = ceil;
                            changed = true;
                        }
                    }
                }
                if !changed {
                    break;
                }
            }
        }
    }

    (b, ny)
}

/// Billow a flat fore-and-aft sail region (cells in the `z = 0` plane) **sideways** to
/// leeward. Returns the leeward `z` depth per cell `(x, y)`.
///
/// The **head (gaff), luff (mast) and leech (aft) edges are pinned to 0** so the canvas
/// tapers onto the spars there (the row under the gaff lands at `z = 0`, a wool block bent
/// onto it). The **foot (boom) is left free** except at its **two corners** (tack + clew,
/// held by the luff/leech) — so the foot billows up off the boom, giving a less boxy belly.
/// The interior starts at the target `wind` and is **relaxed to a 1-block gradient** (clamped
/// to its lowest pinning/in-region neighbour + 1), staying hole-free (no neighbour steps >1).
///
/// `spars` = boom + gaff cells; the boom is the bottom row (`y == foot_y`), the gaff sits
/// above it. A neighbour pins toward 0 if it is the gaff (a spar above the foot) or it is off
/// the canvas to the **sides/top** (`y ≥ foot_y`); off-canvas **below the foot** and the boom
/// itself do **not** pin. The caller offsets each cell to `z = depth · side` and skips spars.
pub fn spanker_billow(
    cells: &[Point3D],
    wind: f32,
    spars: &std::collections::HashSet<(i32, i32)>,
) -> HashMap<(i32, i32), i32> {
    let region: std::collections::HashSet<(i32, i32)> = cells.iter().map(|c| (c.x, c.y)).collect();
    let foot_y = cells.iter().map(|c| c.y).min().unwrap_or(0); // lowest (corner) boom row
    let target = wind.round().max(0.0) as i32;
    let neighbours = |p: (i32, i32)| {
        [(p.0 - 1, p.1), (p.0 + 1, p.1), (p.0, p.1 - 1), (p.0, p.1 + 1)]
    };
    // The foot edge: the lowest region cell of each column (the arced boom edge).
    let mut foot_cells: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    {
        let mut min_y: HashMap<i32, i32> = HashMap::new();
        for c in cells {
            let e = min_y.entry(c.x).or_insert(c.y);
            *e = (*e).min(c.y);
        }
        for (x, y) in min_y {
            foot_cells.insert((x, y));
        }
    }
    // Pins toward 0: the gaff (a spar above the foot), or off-canvas to the **sides/top**.
    // Off-canvas **directly below the foot edge** does not pin → the foot (its arc and the
    // span between the two corners) is free to billow.
    let pins = |q: (i32, i32)| {
        if spars.contains(&q) && q.1 > foot_y {
            return true; // gaff (head)
        }
        if region.contains(&q) {
            return false;
        }
        // off-canvas: pins everywhere except just under the foot edge
        !foot_cells.contains(&(q.0, q.1 + 1))
    };

    const FREE: i32 = 1 << 20; // an off-canvas, non-pinning neighbour imposes no ceiling
    let mut anchors: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    let mut depth: HashMap<(i32, i32), i32> = HashMap::new();
    for c in cells {
        let p = (c.x, c.y);
        // Pinned to 0 if it is the gaff, or it touches a pinning edge (this also pins the two
        // foot corners, where the luff/leech meet the boom).
        if (spars.contains(&p) && p.1 > foot_y) || neighbours(p).into_iter().any(pins) {
            anchors.insert(p);
            depth.insert(p, 0);
        } else {
            depth.insert(p, target);
        }
    }
    // Relax the non-anchored interior to 1-Lipschitz (bounded by the cell count).
    for _ in 0..cells.len() {
        let mut changed = false;
        for c in cells {
            let p = (c.x, c.y);
            if anchors.contains(&p) {
                continue;
            }
            let nb = |q: (i32, i32)| {
                if pins(q) {
                    0
                } else if region.contains(&q) {
                    *depth.get(&q).unwrap_or(&0)
                } else {
                    FREE // off-canvas, non-pinning (below the foot) → no constraint
                }
            };
            let ceil = neighbours(p).into_iter().map(nb).min().unwrap_or(0) + 1;
            let d = depth.get_mut(&p).unwrap();
            if *d > ceil {
                *d = ceil;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    depth
}

/// A platform centred on the mast at `(cx, nest_y)`: a `half`-radius slab floor with the
/// mast passing through the centre. `fenced` adds a fence basket one block up (the top
/// crow's nest); intermediate platforms are unfenced.
fn build_nest(cx: i32, nest_y: i32, half: i32, fenced: bool) -> Nest {
    let mut floor = Vec::new();
    let mut rail = Vec::new();
    for dx in -half..=half {
        for dz in -half..=half {
            if dx == 0 && dz == 0 {
                continue; // the mast passes through the centre
            }
            floor.push(Point3D::new(cx + dx, nest_y, dz));
            if fenced && (dx.abs() == half || dz.abs() == half) {
                rail.push(Point3D::new(cx + dx, nest_y + 1, dz)); // basket fence
            }
        }
    }
    Nest { floor, rail }
}

/// Find a platform Y at or below `target` that's **more than `gap`** blocks from every
/// yard — so a nest never crowds a stay.
fn clear_nest_y(target: i32, yard_ys: &[i32], lo: i32, gap: i32) -> i32 {
    let mut y = target;
    while y > lo && yard_ys.iter().any(|&yy| (yy - y).abs() <= gap) {
        y -= 1;
    }
    y
}

/// Build the masts + spars for a hull of `length`, `count` masts, leaning forward by
/// `lean` (`0.0` = vertical). `deck_rise` (additional-deck height) is added to every
/// mast; `weather_y` is the top weather deck (yards sit above it). `spanker` = whether the
/// aftmost mast carries a spanker (rolled per ship).
pub fn build_masts_model(
    length: i32,
    count: i32,
    lean: f32,
    deck_rise: i32,
    weather_y: i32,
    spanker: bool,
    top_nest: bool,
    flag_phase: f32,
    flag_len_seed: i32,
    flag_stream_sign: i32,
    deck_clearance: i32,
) -> MastModel {
    let main_h = ((length as f32) * MAST_HEIGHT_FACTOR).round().max(6.0) as i32;
    // Taller finial on multi-mast ships (reads better with the mast-to-mast stays).
    let finial = if count >= 2 { MAST_TOP_FENCE_MULTI } else { MAST_TOP_FENCE };
    let specs = layout(count);
    let aft_xf = specs.iter().map(|s| s.0).fold(f32::INFINITY, f32::min);
    let yard_base_hw = ((length as f32) * MAST_YARD_HALF_FRACTION).round().clamp(2.0, 9.0) as i32;
    let height_of = |hf: f32| (((main_h as f32) * hf).round().max(4.0) as i32) + deck_rise;
    let max_h = specs.iter().map(|&(_, hf)| height_of(hf)).fold(0, i32::max);

    let masts = specs
        .iter()
        .map(|&(xf, hf)| {
            let base_x = ((length as f32) * xf).round() as i32;
            let height = height_of(hf);
            let dx = |y: i32| (lean * y as f32).round() as i32; // forward shift with height
            // The tallest mast(s) may get a fenced top crow's nest 1 below the exact top.
            let has_top_nest = top_nest && height == max_h && height >= MAST_NEST_MIN_HEIGHT;
            let top_nest_y = height - 2;

            // Keel-stepped: the foot rests **on** the keel (y = 1), not through its bottom course
            // (y = 0). Runs up to the masthead.
            let cells = (1..height).map(|y| Point3D::new(base_x + dx(y), y, 0)).collect();

            // Main yards distributed so the **lowest sail is the largest**. Sails stack
            // from a foot base a few blocks above the deck (the bottom 2–3 carry no canvas,
            // so the planning must allow for them) up to the top yard near the masthead.
            // Each sail's height is weighted — heaviest at the bottom course, shrinking
            // going up — and yards are placed bottom→top atop each sail, then recorded
            // top→bottom (the order the width/render code expects). The top yard drops an
            // extra block below the masthead on taller masts (room for a topgallant above).
            let top_drop =
                (height > MAST_TOP_YARD_DROP_H1) as i32 + (height > MAST_TOP_YARD_DROP_H2) as i32;
            let mut top = height - 1 - top_drop; // top yard, below the fence finial
            if has_top_nest {
                top = top.min(top_nest_y - 1 - MAST_NEST_YARD_GAP); // keep clear below the nest
            }
            // Bottom of the lowest sail. `deck_clearance` already covers the deck **and
            // its railing**; we add +1 for a wide (big) course so it clears the rail by 3.
            // Excluding this from the budget keeps the lowest sail off the rail.
            let foot_base = weather_y
                + deck_clearance
                + (yard_base_hw >= SAIL_BIG_HALF_WIDTH) as i32;
            let span = (top - foot_base).max(MAST_SAIL_TOP_HEIGHT);
            let n = (span / MAST_YARD_SPAN_PER_SAIL + 1).clamp(1, MAST_MAX_YARDS);
            // Sail-height budget = span minus the (n−1) yard rows between stacked sails.
            let budget = (span - (n - 1)).max(n * MAST_SAIL_TOP_HEIGHT);
            // Weights: bottom sail (k = 0) heaviest, shrinking upward.
            let weights: Vec<f32> =
                (0..n).map(|k| 1.0 + (n - 1 - k) as f32 * MAST_SAIL_GROWTH).collect();
            let wsum: f32 = weights.iter().sum::<f32>().max(1.0);
            let heights: Vec<i32> = weights
                .iter()
                .map(|w| (((budget as f32) * w / wsum).round() as i32).max(MAST_SAIL_TOP_HEIGHT))
                .collect();
            // Place yards bottom→top: yard k caps sail k; a yard row separates stacked sails.
            let mut yard_ys_btt: Vec<i32> = Vec::with_capacity(n as usize);
            let mut y = foot_base;
            for &h in &heights {
                y += h;
                yard_ys_btt.push(y.min(top));
                y += 1;
            }
            // Record `(yard_y, sail_height)` top→bottom.
            let mut entries: Vec<(i32, i32)> = Vec::new();
            for k in (0..n as usize).rev() {
                let yk = yard_ys_btt[k];
                let foot = if k == 0 { foot_base } else { yard_ys_btt[k - 1] + 1 };
                entries.push((yk, (yk - foot).max(1)));
            }
            // Widths: bottom yard widest, narrowing gently going up (capped).
            let n = entries.len();
            let yards: Vec<Yard> = entries
                .iter()
                .enumerate()
                .map(|(idx, &(y, sh))| {
                    let from_bottom = (n - 1 - idx) as i32;
                    let hw = (yard_base_hw - from_bottom.min(MAST_YARD_NARROW_MAX)).max(2);
                    Yard { x: base_x + dx(y) + MAST_YARD_FORWARD, y, half_width: hw, sail_height: sh }
                })
                .collect();

            // Fence finial straight on top (taller on multi-mast ships).
            let xt = base_x + dx(height);
            let top_fence = (0..finial).map(|k| Point3D::new(xt, height + k, 0)).collect();

            // Masthead pennant streaming aft off the top of the finial. Length + ripple
            // phase vary per mast (seeded per ship) so no two flags match.
            let flag_span = FLAG_MAX_LEN - FLAG_MIN_LEN + 1;
            let flag_len = FLAG_MIN_LEN + (flag_len_seed + base_x).rem_euclid(flag_span);
            let flag = build_flag(
                xt,
                height + finial - 1,
                flag_len,
                flag_phase + base_x as f32 * 0.6,
                flag_stream_sign,
            );

            // Spanker only on the aftmost (stern-most) mast, and only when rolled.
            let mast_spanker = if spanker && (xf - aft_xf).abs() < 1e-6 {
                Some(build_spanker(base_x, height, weather_y, length))
            } else {
                None
            };

            // Platforms (clear of the yards): an unfenced 3×3 intermediate on tall masts,
            // plus a fenced 5×5 top crow's nest at the tallest mast's top.
            let yard_ys: Vec<i32> = yards.iter().map(|y: &Yard| y.y).collect();
            let mut nests = Vec::new();
            if height >= MAST_NEST_MIN_HEIGHT {
                let target = ((height as f32) * MAST_NEST_HEIGHT_FRACTION).round() as i32;
                let ny = clear_nest_y(target, &yard_ys, weather_y + 2, MAST_NEST_YARD_GAP);
                nests.push(build_nest(base_x + dx(ny), ny, MAST_NEST_PLATFORM_HALF, false));
            }
            if has_top_nest {
                nests.push(build_nest(base_x + dx(top_nest_y), top_nest_y, MAST_NEST_HALF, true));
            }

            Mast {
                base_x,
                height,
                cells,
                yards,
                top_fence,
                flag,
                spanker: mast_spanker,
                nests,
                sail_foot_base: foot_base,
            }
        })
        .collect();

    MastModel { masts }
}

/// **4-connected** staircase (`z = 0`) from `a` to `b`: each step moves exactly one cell
/// orthogonally (never diagonally), so consecutive cells share a **face** — a continuous line
/// with no corner-only gaps. Steps the axis with the larger remaining distance first.
pub fn step_line_xy(a: Point3D, b: Point3D) -> Vec<(i32, i32)> {
    let (mut x, mut y) = (a.x, a.y);
    let (tx, ty) = (b.x, b.y);
    let mut out = vec![(x, y)];
    while x != tx || y != ty {
        let (rx, ry) = ((tx - x).abs(), (ty - y).abs());
        if (rx >= ry && x != tx) || y == ty {
            x += (tx - x).signum();
        } else {
            y += (ty - y).signum();
        }
        out.push((x, y));
    }
    out
}

/// Cells (`z = 0`) along the straight segment `a`→`b` in the x–y plane (integer DDA).
pub fn line_xy(a: Point3D, b: Point3D) -> Vec<(i32, i32)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let steps = dx.abs().max(dy.abs()).max(1);
    let mut out: Vec<(i32, i32)> = (0..=steps)
        .map(|i| (a.x + dx * i / steps, a.y + dy * i / steps))
        .collect();
    out.dedup();
    out
}

/// Filled triangle cells (`z = 0`) for corners `a, b, c` in the x–y plane (edges included).
pub fn triangle_xy(a: Point3D, b: Point3D, c: Point3D) -> Vec<(i32, i32)> {
    let sign = |p: (i32, i32), q: (i32, i32), r: (i32, i32)| {
        (p.0 - r.0) * (q.1 - r.1) - (q.0 - r.0) * (p.1 - r.1)
    };
    let (ax, ay, bx, by, cx, cy) = (a.x, a.y, b.x, b.y, c.x, c.y);
    let mut out = Vec::new();
    for x in ax.min(bx).min(cx)..=ax.max(bx).max(cx) {
        for y in ay.min(by).min(cy)..=ay.max(by).max(cy) {
            let d1 = sign((x, y), (ax, ay), (bx, by));
            let d2 = sign((x, y), (bx, by), (cx, cy));
            let d3 = sign((x, y), (cx, cy), (ax, ay));
            let neg = d1 < 0 || d2 < 0 || d3 < 0;
            let pos = d1 > 0 || d2 > 0 || d3 > 0;
            if !(neg && pos) {
                out.push((x, y));
            }
        }
    }
    out
}

/// Interior samples (excluding endpoints) of a quadratic Bézier from `p` to `q`, whose control
/// point is the edge midpoint pushed perpendicular by `bulge` blocks **toward** `toward` — so the
/// curve bows **inward** (a hollow toward the opposite corner). Empty for a degenerate edge / zero
/// bulge.
fn bezier_samples(p: Point3D, q: Point3D, toward: Point3D, bulge: f32) -> Vec<(f32, f32)> {
    let (px, py) = (p.x as f32, p.y as f32);
    let (qx, qy) = (q.x as f32, q.y as f32);
    let (dx, dy) = (qx - px, qy - py);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-3 || bulge.abs() < 1e-3 {
        return Vec::new();
    }
    let (mx, my) = ((px + qx) / 2.0, (py + qy) / 2.0);
    // Unit perpendicular to p→q; flip so it points **toward** the opposite corner `toward`.
    let (mut nx, mut ny) = (-dy / len, dx / len);
    if nx * (toward.x as f32 - mx) + ny * (toward.y as f32 - my) < 0.0 {
        nx = -nx;
        ny = -ny;
    }
    let (cx, cy) = (mx + nx * bulge, my + ny * bulge);
    let steps = (len.round() as usize).max(2);
    (1..steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let u = 1.0 - t;
            (u * u * px + 2.0 * u * t * cx + t * t * qx, u * u * py + 2.0 * u * t * cy + t * t * qy)
        })
        .collect()
}

/// Even-odd point-in-polygon test (cell centre `+0.5`).
fn point_in_poly(px: f32, py: f32, v: &[(f32, f32)]) -> bool {
    let n = v.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = v[i];
        let (xj, yj) = v[j];
        if (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Cells in the **bite** between the straight edge `p`→`q` and a quadratic Bézier that bows
/// **inward** (toward the opposite corner `toward`) by `bulge` — i.e. the sliver to *carve off* the
/// triangle so the edge becomes concave. The bite polygon (`p` → bezier → `q`, closed straight back
/// to `p`) is always **simple** (the single bow can't cross its own chord), so the fill is robust.
fn edge_bite(p: Point3D, q: Point3D, toward: Point3D, bulge: f32) -> std::collections::HashSet<(i32, i32)> {
    let samples = bezier_samples(p, q, toward, bulge);
    if samples.is_empty() {
        return std::collections::HashSet::new();
    }
    let mut poly: Vec<(f32, f32)> = vec![(p.x as f32, p.y as f32)];
    poly.extend(samples);
    poly.push((q.x as f32, q.y as f32));
    let (minx, maxx) = (
        poly.iter().map(|v| v.0).fold(f32::INFINITY, f32::min).floor() as i32,
        poly.iter().map(|v| v.0).fold(f32::NEG_INFINITY, f32::max).ceil() as i32,
    );
    let (miny, maxy) = (
        poly.iter().map(|v| v.1).fold(f32::INFINITY, f32::min).floor() as i32,
        poly.iter().map(|v| v.1).fold(f32::NEG_INFINITY, f32::max).ceil() as i32,
    );
    let mut out = std::collections::HashSet::new();
    for y in miny..=maxy {
        for x in minx..=maxx {
            if point_in_poly(x as f32 + 0.5, y as f32 + 0.5, &poly) {
                out.insert((x, y));
            }
        }
    }
    out
}

/// Filled jib outline (`z = 0`): the **luff** b→c stays **straight** (it's on the forestay), but the
/// **foot** a→b and **leech** c→a bow gently **inward** (a hollow of `foot_bulge`/`leech_bulge`
/// blocks at their midpoints), so the sail reads as cloth rather than a rigid triangle. Built as the
/// straight triangle **minus the two edge bites** ([`edge_bite`]) — robust to the curves crossing
/// near the shared corner (an outline polygon would self-intersect there and leave holes). The
/// straight luff and the three corners are kept so the pinned luff/anchor cells are always present.
pub fn curved_sail_xy(a: Point3D, b: Point3D, c: Point3D, foot_bulge: f32, leech_bulge: f32) -> Vec<(i32, i32)> {
    let mut cells: std::collections::HashSet<(i32, i32)> = triangle_xy(a, b, c).into_iter().collect();
    let foot_bite = edge_bite(a, b, c, foot_bulge); // foot a→b bows toward c
    let leech_bite = edge_bite(a, c, b, leech_bulge); // leech a→c bows toward b
    cells.retain(|p| !foot_bite.contains(p) && !leech_bite.contains(p));
    // Keep the straight luff (pinned) + the three corners (anchors / apex) present.
    cells.extend(line_xy(b, c));
    for corner in [a, b, c] {
        cells.insert((corner.x, corner.y));
    }
    cells.into_iter().collect()
}

/// Billow a flat triangular sail (cells in the `z = 0` plane) **sideways** to leeward, with
/// the `pinned` cells (the luff line, on the forestay) held flat at 0 and everything else
/// free to billow — held only where it meets a pinned cell. The interior starts at the target
/// `wind` and is **relaxed to a 1-block gradient** (each cell clamped to its lowest in-region
/// neighbour + 1; off-region neighbours impose no ceiling), so it is hole-free.
pub fn jib_billow(
    cells: &[(i32, i32)],
    wind: f32,
    pinned: &std::collections::HashSet<(i32, i32)>,
) -> HashMap<(i32, i32), i32> {
    let region: std::collections::HashSet<(i32, i32)> = cells.iter().copied().collect();
    let target = wind.round().max(0.0) as i32;
    let mut depth: HashMap<(i32, i32), i32> = cells
        .iter()
        .map(|&p| (p, if pinned.contains(&p) { 0 } else { target }))
        .collect();
    const FREE: i32 = 1 << 20;
    for _ in 0..cells.len() {
        let mut changed = false;
        for &p in cells {
            if pinned.contains(&p) {
                continue;
            }
            let nb = |q: (i32, i32)| {
                if region.contains(&q) {
                    *depth.get(&q).unwrap_or(&0)
                } else {
                    FREE
                }
            };
            let ceil = nb((p.0 - 1, p.1))
                .min(nb((p.0 + 1, p.1)))
                .min(nb((p.0, p.1 - 1)))
                .min(nb((p.0, p.1 + 1)))
                + 1;
            let d = depth.get_mut(&p).unwrap();
            if *d > ceil {
                *d = ceil;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    depth
}

/// Place the masts (logs) + spars (slabs/fences/stairs) and record them in `state`.
pub async fn build(ctx: &mut ShipCtx<'_>, dc: &DeckContext<'_>, state: &mut DeckState) {
    let deck_rise = (state.top_y - dc.deck.deck_y).max(0);
    let has_spanker = ctx.rng.rand_i32_range(0, 100) < MAST_SPANKER_CHANCE;
    let has_top_nest = ctx.rng.rand_i32_range(0, 100) < MAST_NEST_CHANCE;
    // Jib (triangular headsail off the bowsprit): optional, with the chance **rising with
    // ship size**. Needs a bowsprit (built earlier; Medium+), so Small effectively never.
    let jib_chance = match dc.tier {
        SizeTier::Small => 0,
        SizeTier::Medium => JIB_CHANCE_MEDIUM,
        SizeTier::Large => JIB_CHANCE_LARGE,
        SizeTier::Huge => JIB_CHANCE_HUGE,
    };
    let has_jib = ctx.rng.rand_i32_range(0, 100) < jib_chance;
    // Masthead flags: one wool colour for the whole ship (her colours); ripple phase +
    // length seed rolled per ship, varied per mast inside the model.
    let flag_color = (*ctx.rng.choose(FLAG_COLORS)).to_string();
    let flag_phase = (ctx.rng.rand_i32_range(0, 360) as f32).to_radians();
    let flag_len_seed = ctx.rng.rand_i32_range(0, 1000);
    // With set sails the wind is from astern (it bellies the sails toward `SAIL_BILLOW_DIR`),
    // so the pennants must stream the same way (downwind) to make sense. At rest (furled /
    // no sails) they hang aft.
    let flag_stream_sign = if dc.sail_state == SailState::Full {
        match SAIL_BILLOW_DIR {
            ShipDir::Bow => 1,
            _ => -1,
        }
    } else {
        -1
    };
    // Deployed-sail billow shape: weighted random per ship (Combined / Curtain / Domed).
    let sail_billow = SailBillow::pick(ctx.rng);
    // Spanker leeward side (which way it bellies in z): **random per ship** for now —
    // `+1` starboard / `-1` port. When a real wind direction feeds the rig later, decide it
    // from that here instead. Kept consistent for the whole ship (only the aftmost mast
    // carries a spanker anyway).
    let spanker_side = if ctx.rng.rand_i32_range(0, 2) == 0 { -1 } else { 1 };
    // The lowest sail must clear the deck **and its railing**. The railing rises
    // `BULWARK_HEIGHT` (solid) + 1 (fence cap) above the weather deck when present, so the
    // foot sits that far up plus `SAIL_FOOT_CLEARANCE` of open air above the rail.
    let rail_h = state.railing.as_ref().map_or(0, |_| BULWARK_HEIGHT + 1);
    let deck_clearance = rail_h + SAIL_FOOT_CLEARANCE;
    let model = build_masts_model(
        dc.hull.length,
        dc.tier.mast_count(),
        dc.mast_lean,
        deck_rise,
        state.top_y,
        has_spanker,
        has_top_nest,
        flag_phase,
        flag_len_seed,
        flag_stream_sign,
        deck_clearance,
    );

    let mast_mat = ctx
        .palette
        .get_material(dc.ship_palette.role(ShipPart::Mast))
        .expect("Mast role missing from base palette")
        .clone();
    let spar_mat = ctx
        .palette
        .get_material(dc.ship_palette.role(ShipPart::Spar))
        .expect("Spar role missing from base palette")
        .clone();
    let mut mast_rng = ctx.rng.derive();
    let mut spar_rng = ctx.rng.derive();
    let mut mast_placer = MaterialPlacer::new(Placer::new(&ctx.data.materials, &mut mast_rng), mast_mat);
    let mut spar_placer = MaterialPlacer::new(Placer::new(&ctx.data.materials, &mut spar_rng), spar_mat);
    let place = dc.placement;

    let axis_y = HashMap::from([("axis".to_string(), "y".to_string())]);
    let bottom_slab = HashMap::from([("type".to_string(), "bottom".to_string())]);
    let top_slab = HashMap::from([("type".to_string(), "top".to_string())]);
    let fence_state: HashMap<String, String> = HashMap::new();

    for mast in &model.masts {
        // Pole (vertical logs).
        for &cell in &mast.cells {
            mast_placer
                .place_block(ctx.editor, place.to_world(cell), BlockForm::Block, Some(&axis_y), None)
                .await;
        }
        // Main yards (the spar — plank slabs), always placed.
        for yard in &mast.yards {
            for z in -yard.half_width..=yard.half_width {
                spar_placer
                    .place_block(ctx.editor, place.to_world(Point3D::new(yard.x, yard.y, z)), BlockForm::Slab, Some(&bottom_slab), None)
                    .await;
            }
        }
        // Furled sail: the rolled-up canvas hangs **just under** the yard and connects to
        // it — alternating quartz stairs (no wool stairs exist) facing the stern; the odd
        // yard span makes the alternation start and end upside-down.
        if dc.sail_state == SailState::Furled {
            let stern_face = place.world_cardinal(ShipDir::Stern).to_string();
            for yard in &mast.yards {
                let hw = yard.half_width;
                for (i, z) in (-hw..=hw).enumerate() {
                    let half = if i % 2 == 0 { "top" } else { "bottom" }; // start upside-down
                    if let Some(b) = string_to_block(&format!(
                        "minecraft:quartz_stairs[facing={stern_face},half={half}]"
                    )) {
                        ctx.editor
                            .place_block(&b, place.to_world(Point3D::new(yard.x, yard.y - 1, z)))
                            .await;
                    }
                }
                // Wide furled sails sag into a fixed, symmetric U-shaped swag at each end,
                // one row below the roll (capped — it never grows toward the centre). Read
                // from the end inward (palindrome about the 5th piece):
                //   3rd (offset 2): top slab                            [width >= 9]
                //   4th (offset 3): right-side-up stern stair           [width >= 13]
                //   5th (offset 4): top slab, or upside-down stair @17+ [width >= 13]
                //   6th (offset 5): right-side-up stern stair           [width >= 17]
                //   7th (offset 6): top slab                            [width >= 17]
                let width = 2 * hw + 1;
                let yb = yard.y - 2;
                let slab = || "minecraft:quartz_slab[type=top]".to_string();
                let rsu = || format!("minecraft:quartz_stairs[facing={stern_face},half=bottom]");
                let mut swag: Vec<(String, i32)> = Vec::new();
                if width >= 9 {
                    for z in [-hw + 2, hw - 2] {
                        swag.push((slab(), z));
                    }
                }
                if width >= 13 {
                    for z in [-hw + 3, hw - 3] {
                        swag.push((rsu(), z));
                    }
                    let fifth = if width >= 17 {
                        format!("minecraft:quartz_stairs[facing={stern_face},half=top]")
                    } else {
                        slab()
                    };
                    for z in [-hw + 4, hw - 4] {
                        swag.push((fifth.clone(), z));
                    }
                }
                if width >= 17 {
                    for z in [-hw + 5, hw - 5] {
                        swag.push((rsu(), z));
                    }
                    for z in [-hw + 6, hw - 6] {
                        swag.push((slab(), z));
                    }
                }
                for (block, z) in &swag {
                    if let Some(b) = string_to_block(block) {
                        ctx.editor.place_block(&b, place.to_world(Point3D::new(yard.x, yb, *z))).await;
                    }
                }
            }
        }
        // Set (deployed) square sails: a billowing white-wool sheet hung from each yard —
        // head pinned just under its own yard, foot just above the next yard down (or, for
        // the lowest, `mast.sail_foot_base`, clear of the deck **and** its rail). The sheet
        // bellies **forward** into the wind: one block per (z, y) at `x = yard.x + bulge`,
        // the bulge peaking at the centre (parabola across the width × a sin profile down
        // the height, both pinned at the edges) with `dc.wind` the depth. The bulge field is
        // then **relaxed to a 1-block gradient** so it never steps >1 between neighbouring
        // cells — the sheet stays a single hole-free surface (every block has a neighbour in
        // its 3×3, no see-through gaps at an angle).
        if dc.sail_state == SailState::Full {
            let sail_block = string_to_block(&format!("minecraft:{SAIL_BLOCK}"))
                .expect("SAIL_BLOCK should parse");
            let sign = match SAIL_BILLOW_DIR {
                ShipDir::Bow => 1,
                _ => -1,
            };
            let yards = &mast.yards;
            for (idx, yard) in yards.iter().enumerate() {
                let top_y = yard.y - 1; // head: just under the yard
                let bottom_y = if idx + 1 < yards.len() {
                    yards[idx + 1].y + 1 // foot: just above the next yard down
                } else {
                    mast.sail_foot_base // lowest: clear of the deck + rail
                };
                if top_y < bottom_y {
                    continue;
                }
                let hw = yard.half_width;
                let (b, ny) = billow_field(hw, bottom_y, top_y, dc.wind, sail_billow);
                for (zi, z) in (-hw..=hw).enumerate() {
                    for yi in 0..ny {
                        let y = bottom_y + yi as i32;
                        let cell = Point3D::new(yard.x + b[zi * ny + yi] * sign, y, z);
                        ctx.editor.place_block(&sail_block, place.to_world(cell)).await;
                    }
                }
            }
        }
        // Fence finial on top.
        for &cell in &mast.top_fence {
            spar_placer
                .place_block(ctx.editor, place.to_world(cell), BlockForm::Fence, Some(&fence_state), None)
                .await;
        }
        // Masthead pennant (rippling wool ribbon, the ship's colours).
        if let Some(flag_block) = string_to_block(&format!("minecraft:{flag_color}_wool")) {
            for &cell in &mast.flag.cells {
                ctx.editor.place_block(&flag_block, place.to_world(cell)).await;
            }
        }
        // Spanker (boom slabs + gaff double-stairs).
        if let Some(spanker) = &mast.spanker {
            for sc in spanker.boom.iter().chain(spanker.gaff.iter()) {
                let half = if sc.top_half { "top" } else { "bottom" };
                match sc.form {
                    BlockForm::Stairs => {
                        let st = HashMap::from([
                            ("facing".to_string(), place.world_cardinal(sc.facing.unwrap_or(ShipDir::Stern)).to_string()),
                            ("half".to_string(), half.to_string()),
                        ]);
                        spar_placer
                            .place_block(ctx.editor, place.to_world(sc.local), BlockForm::Stairs, Some(&st), None)
                            .await;
                    }
                    _ => {
                        let st = HashMap::from([("type".to_string(), half.to_string())]);
                        spar_placer
                            .place_block(ctx.editor, place.to_world(sc.local), BlockForm::Slab, Some(&st), None)
                            .await;
                    }
                }
            }
            // Furled spanker: the canvas stowed on the **boom** — a roll of quartz stairs
            // oriented **sideways**, alternating port/starboard, one block above each boom
            // cell.
            if dc.sail_state == SailState::Furled {
                for (i, sc) in spanker.boom.iter().enumerate() {
                    let face = if i % 2 == 0 { ShipDir::Starboard } else { ShipDir::Port };
                    let facing = place.world_cardinal(face).to_string();
                    if let Some(b) =
                        string_to_block(&format!("minecraft:quartz_stairs[facing={facing},half=bottom]"))
                    {
                        let pos = Point3D::new(sc.local.x, sc.local.y + 1, sc.local.z);
                        ctx.editor.place_block(&b, place.to_world(pos)).await;
                    }
                }
            }
            // Set (deployed) spanker: the canvas is **bent to the gaff (head)** and the
            // **mast (luff)**, with the **leech** the free aft edge, but along the **boom
            // (foot)** it's held only at the **two corners** (tack + clew) — the foot between
            // them billows up off the boom for a more natural, less boxy belly. A fore-and-aft
            // sail, so it bellies **sideways** to leeward in `z`: each cell offset to
            // `z = depth · side`, the depth relaxed hole-free. Boom/gaff spar cells skipped.
            if dc.sail_state == SailState::Full {
                let sail_block = string_to_block(&format!("minecraft:{SAIL_BLOCK}"))
                    .expect("SAIL_BLOCK should parse");
                let side = spanker_side; // random per ship (see roll above)
                let spars: std::collections::HashSet<(i32, i32)> = spanker
                    .boom
                    .iter()
                    .chain(spanker.gaff.iter())
                    .map(|sc| (sc.local.x, sc.local.y))
                    .collect();
                let depth =
                    spanker_billow(&spanker.sail, dc.wind * SAIL_SPANKER_WIND_FACTOR, &spars);
                for c in &spanker.sail {
                    if spars.contains(&(c.x, c.y)) {
                        continue; // leave the boom/gaff spars visible
                    }
                    let z = depth.get(&(c.x, c.y)).copied().unwrap_or(0) * side;
                    ctx.editor
                        .place_block(&sail_block, place.to_world(Point3D::new(c.x, c.y, z)))
                        .await;
                }
            }
        }
        // Platforms / crow's nests (top-slab floor + optional fence basket).
        for nest in &mast.nests {
            for &cell in &nest.floor {
                spar_placer
                    .place_block(ctx.editor, place.to_world(cell), BlockForm::Slab, Some(&top_slab), None)
                    .await;
            }
            for &cell in &nest.rail {
                spar_placer
                    .place_block(ctx.editor, place.to_world(cell), BlockForm::Fence, Some(&fence_state), None)
                    .await;
            }
        }
    }

    // Mast-to-mast stays: on **2+ mast ships**, connect the **top of each mast pole to the next**
    // with a standing-rigging line (chain/fence), `MAST_STAY_THICK` (1) block thick, run as a
    // **4-connected staircase** (`step_line_xy`) so the diagonal connects face-to-face with no
    // corner gaps. It attaches to the **mast pole top, below the fence finial**, and **skips** the
    // poles, finials, **and flag cells**, so it never carves the masts or the masthead pennants.
    if model.masts.len() >= 2 {
        let rig_block: Option<Block> = match dc.rigging {
            RiggingMaterial::Chain => string_to_block("minecraft:chain"),
            RiggingMaterial::Fence => {
                let mut r = ctx.rng.derive();
                ctx.palette
                    .get_block(dc.ship_palette.role(ShipPart::Railing), &BlockForm::Fence, &ctx.data.materials, &mut r)
                    .map(|id| Block::from_id(id.clone()))
            }
        };
        if let Some(ch) = &rig_block {
            // Masthead = top of the mast **pole** (highest `cells`), below the finial.
            let mut tops: Vec<Point3D> = model
                .masts
                .iter()
                .map(|m| m.cells.iter().max_by_key(|p| p.y).copied().unwrap_or(Point3D::new(m.base_x, m.height - 1, 0)))
                .collect();
            tops.sort_by_key(|p| p.x);
            // Never overwrite mast poles, finials, or flag cells.
            let solid: std::collections::HashSet<(i32, i32)> = model
                .masts
                .iter()
                .flat_map(|m| {
                    m.cells
                        .iter()
                        .chain(m.top_fence.iter())
                        .chain(m.flag.cells.iter())
                        .map(|p| (p.x, p.y))
                })
                .collect();
            let half = MAST_STAY_THICK / 2;
            for pair in tops.windows(2) {
                for (x, y) in step_line_xy(pair[0], pair[1]) {
                    for dy in -half..=(MAST_STAY_THICK - 1 - half) {
                        let yy = y + dy;
                        if solid.contains(&(x, yy)) {
                            continue; // don't carve the mast poles/finials/flags
                        }
                        ctx.editor
                            .place_block_forced(ch, place.to_world(Point3D::new(x, yy, 0)))
                            .await;
                    }
                }
            }
        }
    }

    // Jib + forestay. The **forestay stay always exists** when there's a bowsprit + foremast
    // (standing rigging), even with no jib bent on. The jib **canvas** (a triangular headsail
    // between bowsprit start A, tip B, and the foremast head) is drawn only when the jib rolled
    // (`has_jib`, size-gated) **and** the sails are set (`Full`); it bellies to leeward (same side
    // as the spanker).
    if let (Some(bowsprit), Some(fore)) =
        (&state.bowsprit, model.masts.iter().max_by_key(|m| m.base_x))
    {
        // Foot raised so the sail sits over the bowsprit, not on/through it.
        let raise = Point3D::new(0, SAIL_JIB_FOOT_RAISE, 0);
        let a = bowsprit
            .spar
            .iter()
            .map(|sc| sc.local)
            .min_by_key(|p| p.x)
            .unwrap_or(bowsprit.tip)
            + raise; // over the bowsprit start
        let b = bowsprit.tip + raise; // over the bowsprit tip
        let c_mast = *fore.cells.iter().max_by_key(|p| p.y).unwrap(); // foremast head (masthead)

        // The sail's head stops `JIB_HEAD_RIGGING` blocks **below** the masthead; the gap from
        // there up to the head is **pure forestay rigging** (no canvas), so the jib doesn't jam
        // solid sail into the masthead/square sails. `c_sail` is the sail's top corner; the top
        // luff cells (`head_rig`) become the rigging bridge.
        let luff_full = line_xy(b, c_mast);
        let head = (JIB_HEAD_RIGGING as usize).min(luff_full.len().saturating_sub(2));
        let c_sail = {
            let (sx, sy) = luff_full[luff_full.len() - 1 - head];
            Point3D::new(sx, sy, 0)
        };
        let head_rig: Vec<(i32, i32)> = luff_full[luff_full.len() - head..].to_vec();

        // Rigging-line block (chain or palette **fence**, per the ship's `RiggingMaterial`).
        // Chains appear to be dropped by the current live server, so fence is the safe default
        // (see `RiggingMaterial` / `RIGGING_CHAIN_CHANCE`).
        let rig_block: Option<Block> = match dc.rigging {
            RiggingMaterial::Chain => string_to_block("minecraft:chain"),
            RiggingMaterial::Fence => {
                let mut r = ctx.rng.derive();
                ctx.palette
                    .get_block(
                        dc.ship_palette.role(ShipPart::Railing),
                        &BlockForm::Fence,
                        &ctx.data.materials,
                        &mut r,
                    )
                    .map(|id| Block::from_id(id.clone()))
            }
        };
        // Don't lay canvas/rigging over the foremast pole or its yards.
        let mut rig: std::collections::HashSet<(i32, i32)> =
            fore.cells.iter().map(|p| (p.x, p.y)).collect();
        for yd in &fore.yards {
            rig.insert((yd.x, yd.y));
        }
        // Spar top y per x — for the foot hangers (set sail) and the no-canvas stay's foot tie.
        let mut spar_top: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
        for sc in &bowsprit.spar {
            let e = spar_top.entry(sc.local.x).or_insert(sc.local.y);
            *e = (*e).max(sc.local.y);
        }

        // Canvas only when the jib **rolled** and the sails are **set** (`Full`). Otherwise just the
        // stay (no jib, or struck/furled) — a struck jib is just the wire it would hang on.
        let draw_canvas = has_jib && dc.sail_state == SailState::Full;
        if draw_canvas {
            // Roach: the foot (A→B) and leech (A→C) bow outward ~`JIB_CURVE_FRAC` of their length
            // (capped) so the sail reads as cloth; the luff (B→C, on the forestay) stays straight.
            let edge_len = |p: Point3D, q: Point3D| {
                (((q.x - p.x).pow(2) + (q.y - p.y).pow(2)) as f32).sqrt()
            };
            let foot_bulge = (edge_len(a, b) * JIB_CURVE_FRAC).min(JIB_CURVE_MAX);
            let leech_bulge = (edge_len(a, c_sail) * JIB_CURVE_FRAC).min(JIB_CURVE_MAX);
            let tri = curved_sail_xy(a, b, c_sail, foot_bulge, leech_bulge);
            if tri.len() >= 6 {
                let foot: std::collections::HashSet<(i32, i32)> = line_xy(a, b).into_iter().collect();
                let luff: std::collections::HashSet<(i32, i32)> = line_xy(b, c_sail).into_iter().collect();
                // The foot attaches to the bowsprit at only `JIB_FOOT_HANGER_COUNT` columns — the
                // two ends (tack at the tip, clew inboard). Those columns are the **anchors** (the
                // canvas drops to the spar there with a hanger tie); the rest of the foot is left
                // free so it **billows up off** the bowsprit instead of lying along its whole length.
                let mut foot_xs: Vec<i32> = foot.iter().map(|&(x, _)| x).collect();
                foot_xs.sort_unstable();
                foot_xs.dedup();
                let anchor_xs: std::collections::HashSet<i32> =
                    if foot_xs.len() <= JIB_FOOT_HANGER_COUNT || JIB_FOOT_HANGER_COUNT < 2 {
                        foot_xs.iter().copied().collect()
                    } else {
                        (0..JIB_FOOT_HANGER_COUNT)
                            .map(|i| foot_xs[i * (foot_xs.len() - 1) / (JIB_FOOT_HANGER_COUNT - 1)])
                            .collect()
                    };
                let foot_anchor: std::collections::HashSet<(i32, i32)> =
                    foot.iter().copied().filter(|(x, _)| anchor_xs.contains(x)).collect();
                // Pin the forestay (luff) + the two foot anchors; everything else billows free.
                let pinned: std::collections::HashSet<(i32, i32)> =
                    luff.union(&foot_anchor).copied().collect();
                let depth = jib_billow(&tri, dc.wind * SAIL_JIB_WIND_FACTOR, &pinned);
                let sail_block = string_to_block(&format!("minecraft:{SAIL_BLOCK}"))
                    .expect("SAIL_BLOCK should parse");

                for &(x, y) in &tri {
                    let d = depth[&(x, y)];
                    if foot_anchor.contains(&(x, y)) {
                        // Foot anchor (one of the two ends): the canvas corner drapes on the
                        // centreline (z = 0) over the bowsprit, raised clear of the spar, and a
                        // **hanger tie** (chain/fence) drops from just under it to the spar top.
                        ctx.editor.place_block(&sail_block, place.to_world(Point3D::new(x, y, 0))).await;
                        if let (Some(ch), Some(&st)) = (&rig_block, spar_top.get(&x)) {
                            for yy in (st + 1)..y {
                                ctx.editor
                                    .place_block_forced(ch, place.to_world(Point3D::new(x, yy, 0)))
                                    .await;
                            }
                        }
                    } else if !rig.contains(&(x, y)) {
                        // Sail canvas: billow to leeward. The **luff (top edge)** is pinned (d = 0)
                        // so it lands at **z = 0** — the sail's head runs along the bowsprit-tip→
                        // first-mast forestay line, over the bowsprit. The leech and the free foot
                        // between the anchors billow out to z = d*side. Skips the foremast pole/
                        // yards, and **skips any cell already holding a sail** (square sails,
                        // spanker, or any future sail — all placed before the jib) so it never
                        // intersects them.
                        let wp = place.to_world(Point3D::new(x, y, d * spanker_side));
                        let occupied = ctx
                            .editor
                            .get_cached_block(wp)
                            .map_or(false, |b| b.id.as_str().contains("wool"));
                        if !occupied {
                            ctx.editor.place_block(&sail_block, wp).await;
                        }
                    }
                }
            }
        }

        // Forestay/stay rigging — **always** present (chain/fence). With canvas, only the head
        // **bridge** (the top `JIB_HEAD_RIGGING` blocks above the sail head) is bare rigging; with
        // no canvas the **whole stay** (bowsprit tip B → masthead) is rigging, so its shape is fully
        // visible. It **starts one block above the masthead** (ties into the finial) and runs **two
        // blocks tall per column using the cell *above* each step** (`[y, y+1]`) so the stay sits a
        // touch higher and connects face-to-face down the diagonal. It **skips any cell already
        // holding canvas wool**, so its low end rests on **top** of the sail's head wool block (we
        // also push `c_sail` so that on-top block is always placed) rather than carving the sail.
        if let Some(ch) = &rig_block {
            let mut cells: Vec<(i32, i32)> = if draw_canvas {
                let mut h = head_rig;
                h.push((c_sail.x, c_sail.y)); // its y+1 lands the stay on top of the sail head wool
                h
            } else {
                luff_full
            };
            cells.push((c_mast.x, c_mast.y + 1)); // start 1 block higher (into the finial)
            for (x, y) in cells {
                for yy in [y, y + 1] {
                    if rig.contains(&(x, yy)) {
                        continue; // never carve the mast pole/yards
                    }
                    let wp = place.to_world(Point3D::new(x, yy, 0));
                    if ctx.editor.get_cached_block(wp).map_or(false, |b2| b2.id.as_str().contains("wool")) {
                        continue; // sit on **top** of the canvas, don't replace it
                    }
                    ctx.editor.place_block_forced(ch, wp).await;
                }
            }
            // No canvas: tie the stay's forward end down to the bowsprit so it isn't left floating
            // above the spar tip (with canvas, the foot anchor does this).
            if !draw_canvas {
                if let Some(&st) = spar_top.get(&b.x) {
                    for yy in (st + 1)..b.y {
                        ctx.editor
                            .place_block_forced(ch, place.to_world(Point3D::new(b.x, yy, 0)))
                            .await;
                    }
                }
            }
        }
    }

    state.masts = Some(model);
}
