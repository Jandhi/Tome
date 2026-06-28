//! Stage 2 · **Deck** — cover the hull's open top with a slab deck.
//! See `docs/plans/ship-builder.md` (Stage 2 → Initial deck).
//!
//! The hull is a hollow shell open at the top (the waterline, `y = depth`). The
//! deck simply caps that opening with **top slabs** — the floor that further
//! superstructure is built upon. Deck cells are the hull's top-layer interior cells.

use std::collections::HashMap;

use crate::generator::materials::{MaterialPlacer, Placer};
use crate::geometry::Point3D;
use crate::minecraft::BlockForm;

use super::hull::HullModel;
use super::palette::{ShipPalette, ShipPart};
use super::{Placement, ShipCtx};

/// Pure-geometry deck: the slab cells (local frame) and the deck level.
#[derive(Debug, Clone)]
pub struct DeckModel {
    /// Local Y of the deck (the hull's top / waterline).
    pub deck_y: i32,
    /// Deck slab cells.
    pub cells: Vec<Point3D>,
}

/// Build the deck from the hull: the interior cells at the hull's top layer.
pub fn build_deck_model(hull: &HullModel) -> DeckModel {
    let deck_y = hull.depth; // top of the hull / waterline
    let cells = hull
        .interior
        .iter()
        .filter(|c| c.y == deck_y)
        .copied()
        .collect();
    DeckModel { deck_y, cells }
}

/// Place the deck as top slabs (from the `Deck` palette role). Not waterlogged —
/// the deck sits at/above the surface.
pub async fn place_deck(
    ctx: &mut ShipCtx<'_>,
    model: &DeckModel,
    placement: &Placement,
    ship_palette: &ShipPalette,
) {
    let role = ship_palette.role(ShipPart::Deck);
    let material = ctx
        .palette
        .get_material(role)
        .unwrap_or_else(|| panic!("ship palette role {role:?} missing from base palette"))
        .clone();

    let mut placer_rng = ctx.rng.derive();
    let mut placer = MaterialPlacer::new(Placer::new(&ctx.data.materials, &mut placer_rng), material);

    let top_slab = HashMap::from([("type".to_string(), "top".to_string())]);
    for &cell in &model.cells {
        placer
            .place_block(ctx.editor, placement.to_world(cell), BlockForm::Slab, Some(&top_slab), None)
            .await;
    }
}
