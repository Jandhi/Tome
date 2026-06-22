//! Raised aft quarterdeck (poop) for the larger classes — the stern castle the
//! guides describe. Built as a solid raised block over the aft section with a
//! railed top and a ladder up its forward face; the helm/lantern/flag then sit on
//! it (placed by `fittings`). Purely additive: it sits on top of the finished
//! deck and doesn't touch the hull/hold geometry the invariants check.

use std::collections::{HashMap, HashSet};

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Point2D, Point3D};
use crate::minecraft::{Block, BlockForm};
use crate::noise::RNG;

use super::hull::HullModel;
use super::{Placement, ShipClass, ShipDir};

/// Where the quarterdeck ended up, so fittings can put the helm on it.
#[derive(Debug, Clone, Copy)]
pub struct CastleInfo {
    /// Local y of the poop's walking surface.
    pub top_y: i32,
    /// Forward edge station of the poop (local x).
    pub front_x: i32,
}

/// Build a raised quarterdeck over the aft section for eligible classes.
pub async fn maybe_quarterdeck(
    editor: &Editor,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
    model: &HullModel,
    placement: &Placement,
    class: ShipClass,
) -> Option<CastleInfo> {
    if !matches!(class, ShipClass::Cog | ShipClass::Caravel | ShipClass::Galleon) {
        return None;
    }
    let dims = model.dims;
    let deck_y = model.deck_y;
    let rise = dims.freeboard.clamp(2, 3);
    let qd_y = deck_y + rise;
    let q = ((dims.length as f32 * 0.26).round() as i32).clamp(4, dims.length / 3);
    if q < 3 {
        return None;
    }

    let hull_mat = palette
        .get_material(MaterialRole::PrimaryWood)
        .expect("palette has no primary wood for quarterdeck")
        .clone();
    let deck_mat = palette
        .get_material(MaterialRole::GroundFloor)
        .unwrap_or(&hull_mat)
        .clone();

    let mut hull_rng = rng.derive();
    let mut deck_rng = rng.derive();
    let mut hull = MaterialPlacer::new(Placer::new(&data.materials, &mut hull_rng), hull_mat);
    let mut deckp = MaterialPlacer::new(Placer::new(&data.materials, &mut deck_rng), deck_mat);
    let fence = HashMap::new();

    let rim: HashSet<Point2D> = model.gunwale.iter().copied().collect();

    // Solid raised block over the aft stations, railed bulwark on the rim.
    for cell in model.deck_cells.iter().filter(|c| c.x >= 1 && c.x <= q) {
        let (x, z) = (cell.x, cell.y);
        if rim.contains(cell) {
            for y in (deck_y + 1)..=qd_y {
                hull.place_block_forced(editor, placement.to_world(Point3D::new(x, y, z)), BlockForm::Block, None, None).await;
            }
            hull.place_block(editor, placement.to_world(Point3D::new(x, qd_y + 1, z)), BlockForm::Fence, Some(&fence), None).await;
        } else {
            for y in (deck_y + 1)..qd_y {
                hull.place_block_forced(editor, placement.to_world(Point3D::new(x, y, z)), BlockForm::Block, None, None).await;
            }
            deckp.place_block_forced(editor, placement.to_world(Point3D::new(x, qd_y, z)), BlockForm::Block, None, None).await;
        }
    }

    // Front rail along the poop's forward edge, with a centre gap for the ladder.
    for cell in model.deck_cells.iter().filter(|c| c.x == q && !rim.contains(c)) {
        if cell.y == 0 {
            continue;
        }
        hull.place_block(editor, placement.to_world(Point3D::new(q, qd_y + 1, cell.y)), BlockForm::Fence, Some(&fence), None).await;
    }

    // Ladder up the forward face (x = q+1), backed by the solid poop at x = q.
    let mut ladder = Block::from_id("minecraft:ladder".into());
    ladder.state = Some(HashMap::from([
        ("facing".to_string(), placement.world_cardinal(ShipDir::Bow).to_string()),
    ]));
    for y in (deck_y + 1)..=qd_y {
        editor.place_block_forced(&ladder, placement.to_world(Point3D::new(q + 1, y, 0))).await;
    }

    Some(CastleInfo { top_y: qd_y, front_x: q })
}
