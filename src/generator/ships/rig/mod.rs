//! Masts, yards, sails, and rigging. Pure plan first ([`build_plan`] →
//! [`RigModel`]), then placement ([`raise`]). Mirrors the hull split.
//!
//! Phase 2 implements `SingleMast`: one amidships mast carrying a square sail on
//! a yard, with shroud lines down to the gunwale. Multi-mast plans fall back to
//! it until Phase 3.

pub mod mast;
pub mod sail;
pub mod rigging;

use std::collections::HashSet;

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::Palette;
use crate::geometry::{Point2D, Point3D};
use crate::noise::RNG;

use super::dimensions::ShipDimensions;
use super::hull::HullModel;
use super::{Placement, RigPlan};

/// A horizontal spar carrying one sail. Yards shrink the higher they sit.
#[derive(Debug, Clone, Copy)]
pub struct Yard {
    pub y: i32,
    /// Half-length across the beam (z extent).
    pub half: i32,
}

/// One mast and the spars/sails hung on it. All local coords.
#[derive(Debug, Clone)]
pub struct Mast {
    /// Deck cell the mast steps on (y = deck_y, z = 0).
    pub base: Point3D,
    /// Keel-stepped foot: the hull bottom at this station (mast runs down to it).
    pub foot_y: i32,
    /// Local y of the masthead.
    pub top_y: i32,
    /// Local y where the topmast thins to a fence (and the crow's nest sits).
    pub nest_y: i32,
    /// Yards, course (lowest, widest) to topgallant (highest, narrowest).
    pub yards: Vec<Yard>,
}

/// Everything the rig placement stage needs.
#[derive(Debug, Clone)]
pub struct RigModel {
    pub masts: Vec<Mast>,
    /// Sail surface cells (local), excluding the mast column.
    pub sail_cells: Vec<Point3D>,
}

/// Plan the rig for `plan` over the built hull. Mast count comes from the plan;
/// stations are spread along the length, the mainmast tallest.
pub fn build_plan(plan: RigPlan, model: &HullModel, dims: &ShipDimensions) -> RigModel {
    // (length fraction along stern→bow, height fraction of main) per mast. The
    // foremast sits toward the bow (high fraction), the mizzen toward the stern,
    // the mainmast amidships and tallest.
    let layout: &[(f32, f32)] = match plan {
        RigPlan::Oars => return RigModel { masts: Vec::new(), sail_cells: Vec::new() },
        RigPlan::SingleMast => &[(0.5, 1.0)],
        RigPlan::TwoMast => &[(0.45, 1.0), (0.72, 0.85)], // main + foremast (bow-ward)
        RigPlan::ThreeMast => &[(0.5, 1.0), (0.76, 0.85), (0.26, 0.72)], // main, fore, mizzen
    };

    // Mast height ~ 0.6 × length (the reference frigate's mast is a bit over half
    // the hull length), with a floor so small craft still carry a real mast.
    let main_height = ((dims.length as f32 * 0.6).round() as i32).max(dims.depth + 6);
    let mut masts = Vec::new();
    let mut sail_cells = Vec::new();

    for &(lf, hf) in layout {
        let mut mast_x = (lf * (dims.length - 1) as f32).round() as i32;
        // Keep the mast off the hatch column so the ladder stays clear.
        if model.hatch.map_or(false, |h| h.x == mast_x) {
            mast_x = (mast_x + 1).min(dims.length - 1);
        }
        let height = ((main_height as f32 * hf).round() as i32).max(3);
        let (mast, mut sails) = build_mast(model, dims, mast_x, height);
        masts.push(mast);
        sail_cells.append(&mut sails);
    }

    // Spanker: a fore-aft driver behind the aftmost mast (the guides' lateen/gaff
    // rear sail), at the centerline so it reads from the side.
    if let Some(aft) = masts.iter().min_by_key(|m| m.base.x) {
        add_spanker(aft, model, &mut sail_cells);
    }

    sail_cells.sort_by_key(|p| (p.x, p.y, p.z));
    sail_cells.dedup();
    RigModel { masts, sail_cells }
}

/// A triangular fore-aft sail trailing aft of `mast` on the centerline: tall at
/// the mast, tapering down toward the stern.
fn add_spanker(mast: &Mast, model: &HullModel, out: &mut Vec<Point3D>) {
    let deck_y = model.deck_y;
    let head = mast.yards.first().map(|y| y.y).unwrap_or(deck_y + 4); // gaff height
    let len = (head - (deck_y + 1)).clamp(2, 8);
    for dx in 1..=len {
        let top = head - dx; // slopes down toward the stern
        for y in (deck_y + 1)..=top {
            out.push(Point3D::new(mast.base.x - dx, y, 0));
        }
    }
}

/// One keel-stepped mast at `mast_x` rising `height` above the deck. Carries
/// stacked square sails (course → topsail → topgallant) that shrink upward, on
/// yards that shorten with height, with a crow's nest near the top.
fn build_mast(model: &HullModel, dims: &ShipDimensions, mast_x: i32, height: i32) -> (Mast, Vec<Point3D>) {
    let deck_y = model.deck_y;
    let top_y = deck_y + height;
    // Keel-stepped: the mast runs down to the hull bottom at its station.
    let foot_y = model.ribs.iter().find(|r| r.x == mast_x).map(|r| r.bottom_y).unwrap_or(0);
    let deck_half = model
        .ribs
        .iter()
        .find(|r| r.x == mast_x)
        .map(|r| r.deck_half_width())
        .unwrap_or(dims.beam / 2);

    // Taller masts carry more tiers of sail.
    let tiers = if height >= 18 { 3 } else if height >= 11 { 2 } else { 1 };
    let course_y = deck_y + (height as f32 * 0.42).round() as i32;
    let top_yard_y = top_y - 2;
    let nest_y = top_y - 1;

    let mut yards: Vec<Yard> = Vec::new();
    for k in 0..tiers {
        let t = if tiers == 1 { 0.0 } else { k as f32 / (tiers - 1) as f32 };
        let y = course_y + ((top_yard_y - course_y) as f32 * t).round() as i32;
        let half = (((deck_half + 1) as f32) * (1.0 - 0.5 * t)).round().max(1.0) as i32;
        yards.push(Yard { y, half });
    }

    // Each tier's sail hangs from its yard down to the next yard below (or the
    // deck for the course), billowing forward on a pillow hump.
    let mut sail_cells = Vec::new();
    for (k, yard) in yards.iter().enumerate() {
        let sail_top = yard.y - 1;
        let sail_bottom = if k == 0 { deck_y + 1 } else { yards[k - 1].y + 1 };
        if sail_top < sail_bottom {
            continue;
        }
        let v_span = (sail_top - sail_bottom).max(1) as f32;
        let h_span = (2 * yard.half).max(1) as f32;
        let max_bulge = yard.half / 2 + 1;
        for y in sail_bottom..=sail_top {
            let ty = (y - sail_bottom) as f32 / v_span;
            for z in -yard.half..=yard.half {
                if z == 0 {
                    continue; // leave the mast column clear
                }
                let tz = (z + yard.half) as f32 / h_span;
                let hump = (std::f32::consts::PI * ty).sin() * (std::f32::consts::PI * tz).sin();
                let dx = (max_bulge as f32 * hump).round() as i32;
                sail_cells.push(Point3D::new(mast_x + dx, y, z));
            }
        }
    }

    (Mast { base: Point3D::new(mast_x, deck_y, 0), foot_y, top_y, nest_y, yards }, sail_cells)
}

/// Place the planned rig into the world.
pub async fn raise(
    editor: &Editor,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
    rig: &RigModel,
    placement: &Placement,
) {
    mast::place_masts(editor, data, palette, rng, rig, placement).await;
    sail::place_sails(editor, rig, placement).await;
    rigging::place_rigging(editor, rig, placement).await;
}

/// Rig invariants: each mast steps on a real deck cell, and no sail cell collides
/// with the mast column. Run from the pipeline alongside the hull invariants.
pub fn check_rig_invariants(model: &HullModel, rig: &RigModel) -> Result<(), String> {
    let deck: HashSet<Point2D> = model.deck_cells.iter().copied().collect();
    for mast in &rig.masts {
        if !deck.contains(&Point2D::new(mast.base.x, mast.base.z)) {
            return Err(format!("mast base {:?} is not on a deck cell", mast.base));
        }
        if mast.top_y <= model.deck_y {
            return Err(format!("mast at {:?} has no height above deck", mast.base));
        }
    }
    // Square sails leave the mast column clear; only the fore-aft spanker may sit
    // on the centerline, and only aft of (not on) a mast.
    let mast_xs: HashSet<i32> = rig.masts.iter().map(|m| m.base.x).collect();
    if rig.sail_cells.iter().any(|c| c.z == 0 && mast_xs.contains(&c.x)) {
        return Err("sail overlaps a mast column".to_string());
    }
    Ok(())
}
