//! Deck addition · **Main railing** — bulwark + rail cap around the top weather deck.
//!
//! Required for every ship. Follows the **topmost open deck** ([`DeckState`]): for a
//! ship with no raised deck that's the main deck; once the additional deck(s) stack,
//! it's their inset top outline. Each edge station gets a short solid **bulwark**
//! course capped with a **fence rail**, so the upper deck reads as an enclosed,
//! railed weather deck rather than an open ledge.

use std::collections::HashMap;

use crate::generator::materials::{MaterialPlacer, Placer};
use crate::geometry::Point3D;
use crate::minecraft::BlockForm;

use super::super::palette::ShipPart;
use super::super::tuning::BULWARK_HEIGHT;
use super::super::ShipCtx;
use super::{DeckContext, DeckState};

/// Pure-geometry railing: the bulwark (solid) cells and the fence rail-cap cells, in
/// the local frame, plus the deck floor it stands on.
#[derive(Debug, Clone)]
pub struct RailingModel {
    /// Local Y of the deck floor the railing stands on.
    pub deck_y: i32,
    /// Solid bulwark cells (the lower, solid part of the side).
    pub bulwark: Vec<Point3D>,
    /// Fence rail-cap cells (one above the bulwark).
    pub cap: Vec<Point3D>,
}

/// Perimeter `(x, z)` cells of the filled outline `{ |z| <= outline[x] }` — the deck
/// edge (sides + bow/stern caps).
fn perimeter(outline: &[i32]) -> Vec<(i32, i32)> {
    let inside = |x: i32, z: i32| -> bool {
        x >= 0
            && (x as usize) < outline.len()
            && outline[x as usize] >= 1
            && z.abs() <= outline[x as usize]
    };
    let mut cells = Vec::new();
    for x in 0..outline.len() as i32 {
        let h = outline[x as usize];
        if h < 1 {
            continue;
        }
        for z in -h..=h {
            if inside(x, z)
                && (!inside(x - 1, z) || !inside(x + 1, z) || !inside(x, z - 1) || !inside(x, z + 1))
            {
                cells.push((x, z));
            }
        }
    }
    cells
}

/// Build the railing geometry around the `outline` (half-beam per station) standing on
/// the deck floor at `deck_y`.
pub fn build_railing_model(outline: &[i32], deck_y: i32) -> RailingModel {
    let mut bulwark = Vec::new();
    let mut cap = Vec::new();
    for (x, z) in perimeter(outline) {
        for h in 1..=BULWARK_HEIGHT {
            bulwark.push(Point3D::new(x, deck_y + h, z));
        }
        cap.push(Point3D::new(x, deck_y + BULWARK_HEIGHT + 1, z));
    }
    RailingModel { deck_y, bulwark, cap }
}

/// Place the railing around the current top weather deck and record it in `state`. Uses
/// the deck's **outer rim** outline (`DeckState::rail_outline`) — pure geometry, no block
/// look-ups (those don't survive the offline→live coordinate shift).
pub async fn build(ctx: &mut ShipCtx<'_>, dc: &DeckContext<'_>, state: &mut DeckState) {
    let place = dc.placement;
    let model = build_railing_model(&state.top_outline, state.top_y);

    let material = ctx
        .palette
        .get_material(dc.ship_palette.role(ShipPart::Railing))
        .expect("Railing role missing from base palette")
        .clone();
    let mut placer_rng = ctx.rng.derive();
    let mut placer = MaterialPlacer::new(Placer::new(&ctx.data.materials, &mut placer_rng), material);

    // Solid bulwark course(s).
    for &cell in &model.bulwark {
        placer
            .place_block(ctx.editor, place.to_world(cell), BlockForm::Block, None, None)
            .await;
    }
    // Fence rail cap on top.
    let fence_state: HashMap<String, String> = HashMap::new();
    for &cell in &model.cap {
        placer
            .place_block(ctx.editor, place.to_world(cell), BlockForm::Fence, Some(&fence_state), None)
            .await;
    }

    state.railing = Some(model);
}
