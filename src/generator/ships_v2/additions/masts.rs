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
use crate::minecraft::{string_to_block, BlockForm};

use super::super::palette::ShipPart;
use super::super::tuning::{
    MAST_GAFF_STAIR_FACE, MAST_HEIGHT_FACTOR, MAST_MAX_YARDS, MAST_NEST_CHANCE, MAST_NEST_HALF,
    MAST_NEST_HEIGHT_FRACTION, MAST_NEST_MIN_HEIGHT, MAST_NEST_PLATFORM_HALF, MAST_NEST_YARD_GAP,
    MAST_SAIL_GROWTH, MAST_SAIL_TOP_HEIGHT,
    MAST_SPANKER_BOOM_CLEARANCE, MAST_SPANKER_BOOM_FRACTION, MAST_SPANKER_CHANCE,
    MAST_SPANKER_GAFF_RUN_FRACTION, MAST_SPANKER_LUFF_FRACTION, MAST_TOP_FENCE,
    MAST_TOP_YARD_DROP_H1, MAST_TOP_YARD_DROP_H2, MAST_YARD_FORWARD, MAST_YARD_HALF_FRACTION,
    MAST_YARD_MIN_CLEARANCE, MAST_YARD_NARROW_MAX, MAST_YARD_SPAN_PER_SAIL,
};
use super::super::{ShipDir, ShipV2Ctx};
use super::{DeckContext, DeckState, SailState};

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
/// centreline (`z = 0`) plane. The sail (a later step) fills between them.
#[derive(Debug, Clone)]
pub struct Spanker {
    pub boom: Vec<SparCell>,
    pub gaff: Vec<SparCell>,
}

/// A crow's nest / mast platform: a slab floor (mast through the centre) ringed by a
/// fence basket.
#[derive(Debug, Clone)]
pub struct Nest {
    pub floor: Vec<Point3D>,
    pub rail: Vec<Point3D>,
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
    /// Gaff-rigged spanker — only on the aftmost mast (and only by chance).
    pub spanker: Option<Spanker>,
    /// Platforms: an unfenced intermediate platform on tall masts + a fenced top crow's
    /// nest at the tallest mast's top.
    pub nests: Vec<Nest>,
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

    Spanker { boom, gaff }
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
) -> MastModel {
    let main_h = ((length as f32) * MAST_HEIGHT_FACTOR).round().max(6.0) as i32;
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

            let cells = (0..height).map(|y| Point3D::new(base_x + dx(y), y, 0)).collect();

            // Main yards distributed **top-down** across the usable span (masthead → min
            // clearance above the deck): a guaranteed top yard, then yards spread down with
            // gaps (sail heights) growing toward the deck so the lowest sail is biggest.
            // First collect `(yard_y, sail_height)`. The top yard drops an extra block
            // below the masthead on taller masts (room for a topgallant above it).
            let top_drop =
                (height > MAST_TOP_YARD_DROP_H1) as i32 + (height > MAST_TOP_YARD_DROP_H2) as i32;
            let mut top = height - 1 - top_drop; // top yard, below the fence finial
            if has_top_nest {
                top = top.min(top_nest_y - 1 - MAST_NEST_YARD_GAP); // keep clear below the nest
            }
            let bottom = (weather_y + MAST_YARD_MIN_CLEARANCE).min(top); // lowest a yard sits
            let span = top - bottom;
            let n = (span / MAST_YARD_SPAN_PER_SAIL + 1).clamp(1, MAST_MAX_YARDS);
            let weights: Vec<f32> = (0..(n - 1).max(0)).map(|i| 1.0 + i as f32 * MAST_SAIL_GROWTH).collect();
            let wsum: f32 = weights.iter().sum::<f32>().max(1.0);
            let mut entries: Vec<(i32, i32)> = Vec::new();
            let mut y = top;
            for i in 0..n {
                if i < n - 1 {
                    // gap to the next yard down = this yard's sail height
                    let gap = (((span as f32) * weights[i as usize] / wsum).round() as i32)
                        .max(MAST_SAIL_TOP_HEIGHT);
                    entries.push((y, gap));
                    y -= gap;
                } else {
                    // lowest yard: sits at (≈) the clearance, sail hangs down toward the deck
                    let yl = y.max(bottom);
                    entries.push((yl, (yl - weather_y).max(MAST_SAIL_TOP_HEIGHT)));
                }
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

            // Fence finial straight on top.
            let xt = base_x + dx(height);
            let top_fence = (0..MAST_TOP_FENCE).map(|k| Point3D::new(xt, height + k, 0)).collect();

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

            Mast { base_x, height, cells, yards, top_fence, spanker: mast_spanker, nests }
        })
        .collect();

    MastModel { masts }
}

/// Place the masts (logs) + spars (slabs/fences/stairs) and record them in `state`.
pub async fn build(ctx: &mut ShipV2Ctx<'_>, dc: &DeckContext<'_>, state: &mut DeckState) {
    let deck_rise = (state.top_y - dc.deck.deck_y).max(0);
    let has_spanker = ctx.rng.rand_i32_range(0, 100) < MAST_SPANKER_CHANCE;
    let has_top_nest = ctx.rng.rand_i32_range(0, 100) < MAST_NEST_CHANCE;
    let model = build_masts_model(
        dc.hull.length,
        dc.tier.mast_count(),
        dc.mast_lean,
        deck_rise,
        state.top_y,
        has_spanker,
        has_top_nest,
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
        // Fence finial on top.
        for &cell in &mast.top_fence {
            spar_placer
                .place_block(ctx.editor, place.to_world(cell), BlockForm::Fence, Some(&fence_state), None)
                .await;
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

    state.masts = Some(model);
}
