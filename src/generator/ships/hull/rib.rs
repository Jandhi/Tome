//! Per-station cross-section profiles. A `Rib` gives, at one point along the
//! keel, the hull's bottom height and its **half-width at each height level** —
//! so the cross-section can curve. Per the reference guides, the section is a
//! teardrop: a narrow keel, the **widest beam around the waterline**, then a
//! gentle **tumblehome** drawing back in toward the deck. The shape strategy
//! lives here; [`super::build_model`] turns ribs into cells.

use super::super::HullShape;
use super::super::dimensions::ShipDimensions;

/// One transverse station of the hull.
#[derive(Debug, Clone)]
pub struct Rib {
    /// Station position along the length (local x), stern = 0.
    pub x: i32,
    /// Local y of the hull bottom at this station (rises toward bow/stern).
    pub bottom_y: i32,
    /// Half-beam at each height, indexed by `y - bottom_y` (so index 0 is the
    /// keel level, the last entry is the deck level). Cells at height `y` extend
    /// `z ∈ [-half_widths[i], half_widths[i]]`.
    pub half_widths: Vec<i32>,
}

impl Rib {
    /// Half-beam at the deck (the top of the section, after tumblehome).
    pub fn deck_half_width(&self) -> i32 {
        *self.half_widths.last().unwrap_or(&0)
    }
}

/// Build one rib per length station for the given shape.
pub fn build_ribs(shape: HullShape, dims: &ShipDimensions) -> Vec<Rib> {
    match shape {
        HullShape::RowboatHull => rowboat_ribs(dims),
        HullShape::RoundCog => round_cog_ribs(dims),
        HullShape::SleekCaravel => caravel_ribs(dims),
        HullShape::Longship => longship_ribs(dims),
    }
}

/// Double-ended skiff: elliptical waterplane (pointed bow and stern), a gentle
/// rocker lifting the ends, and a soft bilge so it doesn't read as a box.
fn rowboat_ribs(dims: &ShipDimensions) -> Vec<Rib> {
    let full_half = dims.beam / 2;
    let deck_y = dims.depth;
    let waterline_y = (deck_y - dims.freeboard).max(0);
    let rocker_top = (deck_y - 1).max(0);
    let mid = (dims.length - 1) as f32 / 2.0;

    (0..dims.length)
        .map(|x| {
            let frac = if mid > 0.0 { (x as f32 - mid) / mid } else { 0.0 };
            let taper = (1.0 - frac * frac).max(0.0).sqrt();
            let mid_half = (full_half as f32 * taper).round() as i32;
            let bottom_y = ((rocker_top as f32 * frac * frac).round() as i32).clamp(0, rocker_top);

            let floor_half = (mid_half - 1).max(0);
            let deck_half = mid_half; // too small to tumblehome
            let half_widths = teardrop(floor_half, mid_half, deck_half, bottom_y, waterline_y, deck_y, 0.5);
            Rib { x, bottom_y, half_widths }
        })
        .collect()
}

/// Beamy, round-bilged cog: full amidships with fuller ends, a flat floor lifting
/// hard into raised stem/sternposts, a low round bilge and clear tumblehome.
fn round_cog_ribs(dims: &ShipDimensions) -> Vec<Rib> {
    let full_half = dims.beam / 2;
    let deck_y = dims.depth;
    let waterline_y = (deck_y - dims.freeboard).max(0);
    let rocker_top = (deck_y - 1).max(0);
    let mid = (dims.length - 1) as f32 / 2.0;

    (0..dims.length)
        .map(|x| {
            let frac = if mid > 0.0 { (x as f32 - mid) / mid } else { 0.0 };
            let taper = (1.0 - (0.85 * frac) * (0.85 * frac)).max(0.0).sqrt();
            let mid_half = (full_half as f32 * taper).round().max(1.0) as i32;
            let lift = frac * frac * frac * frac;
            let bottom_y = ((rocker_top as f32 * lift).round() as i32).clamp(0, rocker_top);

            let floor_half = ((mid_half as f32 * 0.4).round() as i32).max(1).min(mid_half);
            let deck_half = ((mid_half as f32 * 0.8).round() as i32).max(1).min(mid_half);
            let half_widths = teardrop(floor_half, mid_half, deck_half, bottom_y, waterline_y, deck_y, 0.5);
            Rib { x, bottom_y, half_widths }
        })
        .collect()
}

/// Caravel: fine entry forward, fuller aft (asymmetric waterplane), a sharp
/// V-bottom, a lifting bow, and pronounced tumblehome up to a narrow deck.
fn caravel_ribs(dims: &ShipDimensions) -> Vec<Rib> {
    let full_half = dims.beam / 2;
    let deck_y = dims.depth;
    let waterline_y = (deck_y - dims.freeboard).max(0);
    let rocker_top = (deck_y - 1).max(0);
    let len = (dims.length - 1).max(1) as f32;
    let widest = 0.42;

    (0..dims.length)
        .map(|x| {
            let p = x as f32 / len; // 0 = stern, 1 = bow
            let factor = if p <= widest {
                let t = (widest - p) / widest;
                (1.0 - t * t).max(0.0).sqrt()
            } else {
                let t = (p - widest) / (1.0 - widest);
                (1.0 - t).max(0.0).powf(0.85)
            };
            let mid_half = (full_half as f32 * factor).round().max(0.0) as i32;
            let lift = if p > widest {
                ((p - widest) / (1.0 - widest)).powi(2)
            } else {
                ((widest - p) / widest).powi(3)
            };
            let bottom_y = ((rocker_top as f32 * lift).round() as i32).clamp(0, rocker_top);

            let floor_half = 1.min(mid_half);
            let deck_half = ((mid_half as f32 * 0.85).round() as i32).min(mid_half);
            let half_widths = teardrop(floor_half, mid_half, deck_half, bottom_y, waterline_y, deck_y, 1.3);
            Rib { x, bottom_y, half_widths }
        })
        .collect()
}

/// Longship: long, slender, symmetric and double-ended, shallow near-flat bottom
/// with sharply lifting stem/stern posts (the dragon-prow silhouette).
fn longship_ribs(dims: &ShipDimensions) -> Vec<Rib> {
    let full_half = dims.beam / 2;
    let deck_y = dims.depth;
    let waterline_y = (deck_y - dims.freeboard).max(0);
    let rocker_top = (deck_y - 1).max(0);
    let mid = (dims.length - 1) as f32 / 2.0;

    (0..dims.length)
        .map(|x| {
            let frac = if mid > 0.0 { (x as f32 - mid) / mid } else { 0.0 };
            let mid_half = (full_half as f32 * (1.0 - frac * frac).max(0.0).powf(0.6)).round() as i32;
            let lift = frac.abs().powi(3);
            let bottom_y = ((rocker_top as f32 * lift).round() as i32).clamp(0, rocker_top);

            let floor_half = (mid_half - 1).max(0);
            let deck_half = ((mid_half as f32 * 0.9).round() as i32).min(mid_half);
            let half_widths = teardrop(floor_half, mid_half, deck_half, bottom_y, waterline_y, deck_y, 0.5);
            Rib { x, bottom_y, half_widths }
        })
        .collect()
}

/// Teardrop half-width per height: widen from `floor_half` at the keel to
/// `mid_half` at the waterline (round bilge, controlled by `exp_low` — `0.5`
/// round, `>1` a sharp V), then draw back in to `deck_half` above the waterline
/// (tumblehome).
fn teardrop(
    floor_half: i32,
    mid_half: i32,
    deck_half: i32,
    bottom_y: i32,
    waterline_y: i32,
    deck_y: i32,
    exp_low: f32,
) -> Vec<i32> {
    let levels = (deck_y - bottom_y).max(0);
    if levels == 0 {
        return vec![deck_half.max(mid_half)];
    }
    let wl = (waterline_y - bottom_y).clamp(0, levels);

    (0..=levels)
        .map(|i| {
            if i <= wl {
                if wl == 0 {
                    return mid_half;
                }
                let t = i as f32 / wl as f32;
                (floor_half as f32 + (mid_half - floor_half) as f32 * t.powf(exp_low)).round() as i32
            } else {
                let t = (i - wl) as f32 / (levels - wl).max(1) as f32;
                (mid_half as f32 - (mid_half - deck_half) as f32 * t).round() as i32
            }
        })
        .collect()
}
