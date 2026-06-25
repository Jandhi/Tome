//! Ship's wheel (helm) — a small three-block fitting on the aft weather deck:
//!   - **lectern** base (the binnacle/pedestal),
//!   - a **fence** post on top,
//!   - an **open trapdoor** standing vertical on top as the **wheel**.
//!
//! Placed on the centreline a short way forward of the stern, on the topmost open deck. Blocks are
//! hardcoded oak for now (the ship palette is `ship_oak`), like the quartz/wool sails — a palette
//! role can replace them later. Built for all ships for now (Medium+ gating deferred, like masts).

use crate::geometry::Point3D;
use crate::minecraft::string_to_block;

use super::super::palette::ShipPart;
use super::super::tuning::HELM_STERN_CLEARANCE;
use super::super::{ShipDir, ShipV2Ctx};
use super::{DeckContext, DeckState};

/// Build the helm on the aft weather deck and place it.
pub async fn build(ctx: &mut ShipV2Ctx<'_>, dc: &DeckContext<'_>, state: &mut DeckState) {
    let place = dc.placement;
    let outline = &state.top_outline;
    let n = outline.len() as i32;
    if n < 4 {
        return;
    }
    // Quarterdeck: **halfway between the aftmost mast and the stern**, but never closer to the
    // stern railing than `HELM_STERN_CLEARANCE` clear blocks (the wheel cell is `hx - 1`). Masts
    // lean toward the bow, so any station below the aft mast's base_x is clear of the pole.
    let on_deck = |x: i32| x >= 0 && (x as usize) < outline.len() && outline[x as usize] >= 1;
    let aft_mast = state
        .masts
        .as_ref()
        .and_then(|m| m.masts.iter().map(|mm| mm.base_x).min())
        .unwrap_or(n / 3);
    let stern_x = (0..n).find(|&x| on_deck(x)).unwrap_or(0); // aftmost deck station = stern rail
    let mid = (aft_mast + stern_x) / 2;
    // Wheel at `hx-1` must leave `HELM_STERN_CLEARANCE` clear cells to the rail at `stern_x`:
    // (hx-1) - stern_x - 1 >= clearance  ⇒  hx >= stern_x + 2 + clearance.
    let min_hx = stern_x + 2 + HELM_STERN_CLEARANCE;
    let hx = mid.max(min_hx);
    if hx >= aft_mast || !on_deck(hx) || !on_deck(hx - 1) {
        return; // no room aft of the mast for a helm with the stern clearance
    }
    let base_y = state.top_y; // deck floor; fittings sit at +1 and up

    // The lectern reads toward the stern (the helmsman stands aft of it); the wheel hangs on the
    // **stern (rear) side** of the post. Derive directions from the heading (never hardcode).
    let stern = place.world_cardinal(ShipDir::Stern).to_string();

    // 1) Lectern base.
    if let Some(b) = string_to_block(&format!("minecraft:lectern[facing={stern}]")) {
        ctx.editor.place_block(&b, place.to_world(Point3D::new(hx, base_y + 1, 0))).await;
    }
    // 2) Fence post on top (palette wood — matches the railing).
    let mut rng = ctx.rng.derive();
    if let Some(id) = ctx.palette.get_block(
        dc.ship_palette.role(ShipPart::Railing),
        &crate::minecraft::BlockForm::Fence,
        &ctx.data.materials,
        &mut rng,
    ) {
        let fence = crate::minecraft::Block::from_id(id.clone());
        ctx.editor.place_block(&fence, place.to_world(Point3D::new(hx, base_y + 2, 0))).await;
    }
    // 3) Trapdoor as the wheel — **folded up (open) on the rear/stern side of the post**, in the
    // cell one step toward the stern at the fence-top height. A trapdoor hinges to the block on the
    // side **opposite** its `facing`, so `facing=stern` hinges it onto the **fence** (its bow side)
    // — attached, not floating — with the vertical disc standing on the stern side as the wheel.
    if let Some(b) =
        string_to_block(&format!("minecraft:oak_trapdoor[facing={stern},half=top,open=true]"))
    {
        ctx.editor.place_block(&b, place.to_world(Point3D::new(hx - 1, base_y + 2, 0))).await;
    }
}
