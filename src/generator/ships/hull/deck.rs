//! Deck planking, bulwarks, and gun ports.
//!
//! The guides call for solid **bulwarks** rising 2–3 blocks above the deck (not a
//! bare fence), with the deck recessed inside them and a rail on top, plus **gun
//! ports** (trapdoors) spaced along the sides. The deck floor sits at `deck_y`;
//! the rim (gunwale cells) carries the bulwark.

use std::collections::HashMap;

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Point2D, Point3D};
use crate::minecraft::BlockForm;
use crate::noise::RNG;

use super::super::{Placement, ShipDir};
use super::HullModel;

/// Place the deck floor, the bulwark walls + rail on the rim, and gun ports.
pub async fn place_deck(
    editor: &Editor,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
    model: &HullModel,
    placement: &Placement,
) {
    let deck_material = palette
        .get_material(MaterialRole::GroundFloor)
        .or_else(|| palette.get_material(MaterialRole::PrimaryWood))
        .expect("palette has no deck material")
        .clone();
    let hull_material = palette
        .get_material(MaterialRole::PrimaryWood)
        .expect("palette has no primary wood for bulwark")
        .clone();

    let mut deck_rng = rng.derive();
    let mut hull_rng = rng.derive();

    // Bulwark rises 2 above the deck where there's freeboard for it, else 1.
    let bulwark_h = if model.dims.freeboard >= 2 { 2 } else { 1 };

    // Deck planks at deck_y across the whole deck.
    {
        let mut placer =
            MaterialPlacer::new(Placer::new(&data.materials, &mut deck_rng), deck_material);
        for &cell in &model.deck_cells {
            let local = Point3D::new(cell.x, model.deck_y, cell.y);
            placer.place_block(editor, placement.to_world(local), BlockForm::Block, None, None).await;
        }
    }

    // Bulwark walls + rail on the rim; gun ports punched into the lower course.
    let rim: std::collections::HashSet<Point2D> = model.gunwale.iter().copied().collect();
    let fence_state = HashMap::new();
    {
        let mut placer =
            MaterialPlacer::new(Placer::new(&data.materials, &mut hull_rng), hull_material);
        for &edge in &model.gunwale {
            let is_port = is_gun_port(edge, &rim, model.dims.length);
            for h in 1..=bulwark_h {
                let local = Point3D::new(edge.x, model.deck_y + h, edge.y);
                // Lowest bulwark course on a side run becomes a gun port.
                if h == 1 && is_port {
                    let side = if edge.y > 0 { ShipDir::Starboard } else { ShipDir::Port };
                    let state = HashMap::from([
                        ("facing".to_string(), placement.world_cardinal(side).to_string()),
                        ("half".to_string(), "bottom".to_string()),
                        ("open".to_string(), "true".to_string()),
                    ]);
                    placer.place_block_forced(editor, placement.to_world(local), BlockForm::Trapdoor, Some(&state), None).await;
                } else {
                    placer.place_block(editor, placement.to_world(local), BlockForm::Block, None, None).await;
                }
            }
            // Rail cap on top of the bulwark.
            let cap = Point3D::new(edge.x, model.deck_y + bulwark_h + 1, edge.y);
            placer.place_block(editor, placement.to_world(cap), BlockForm::Fence, Some(&fence_state), None).await;
        }
    }
}

/// A rim cell is a gun-port candidate when it runs fore-and-aft along a side
/// (rim neighbours at x±1, same z), away from the bow/stern caps, every 3rd
/// station — matching the guides' even ~3-block port spacing.
fn is_gun_port(edge: Point2D, rim: &std::collections::HashSet<Point2D>, length: i32) -> bool {
    if edge.x <= 1 || edge.x >= length - 2 || edge.x % 3 != 0 {
        return false;
    }
    rim.contains(&Point2D::new(edge.x - 1, edge.y)) && rim.contains(&Point2D::new(edge.x + 1, edge.y))
}
