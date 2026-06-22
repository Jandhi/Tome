//! Deck/hull fittings: a stern rudder, and a deck hatch with a ladder down into
//! the (otherwise empty) hold. Phase 2 keeps the hold unfurnished — this is just
//! the access that the future interior system will build on.

use std::collections::HashMap;

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Cardinal, Point2D, Point3D};
use crate::minecraft::{Block, BlockForm};
use crate::noise::RNG;

use super::superstructure::CastleInfo;
use super::{Placement, ShipDir};
use super::hull::HullModel;

/// Choose a hatch position: the deck column with the deepest hold beneath it,
/// biased to the waist (amidships) so it stays clear of the aft quarterdeck. The
/// mast then dodges the hatch column if they coincide.
pub fn plan_hatch(model: &HullModel, length: i32) -> Option<Point2D> {
    model.deepest_hold_column(length / 2)
}

/// Place the rudder, hatch, and hold ladder. `hatch` is the deck cell returned by
/// [`plan_hatch`] (already recorded on the model for invariant checking).
pub async fn place_fittings(
    editor: &Editor,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
    model: &HullModel,
    hatch: Option<Point2D>,
    castle: Option<CastleInfo>,
    placement: &Placement,
) {
    let wood = palette
        .get_material(MaterialRole::PrimaryWood)
        .expect("palette has no wood for fittings")
        .clone();

    let mut placer_rng = rng.derive();
    let mut placer = MaterialPlacer::new(Placer::new(&data.materials, &mut placer_rng), wood);

    place_rudder(editor, &mut placer, model, placement).await;
    place_bowsprit(editor, &mut placer, model, placement).await;
    place_helm(editor, &mut placer, model, castle, placement).await;
    place_stern_lantern(editor, model, castle, placement).await;
    place_stern_flag(editor, &mut placer, model, castle, placement).await;
    place_bow_anchor(editor, model, placement).await;

    if let Some(h) = hatch {
        place_hatch_and_ladder(editor, &mut placer, model, h, placement).await;
    }
}

/// The aftmost deck station (stern), and the helm platform `(top_y, helm_x)` —
/// on the quarterdeck if there is one, else the main deck near the sternpost.
fn helm_platform(model: &HullModel, castle: Option<CastleInfo>) -> (i32, i32, i32) {
    let stern_x = model.deck_cells.iter().map(|p| p.x).min().unwrap_or(0);
    match castle {
        Some(c) => (stern_x, c.top_y, c.front_x - 1),
        None => (stern_x, model.deck_y, stern_x + 2),
    }
}

/// Height of the bulwark rail cap above the deck (mirrors `deck::place_deck`).
fn rail_top(model: &HullModel) -> i32 {
    let bulwark_h = if model.dims.freeboard >= 2 { 2 } else { 1 };
    model.deck_y + bulwark_h + 1
}

/// The bow station (highest x with a deck cell) and the deck half-beam there.
fn bow_station(model: &HullModel) -> (i32, i32) {
    let bow_x = model.deck_cells.iter().map(|p| p.x).max().unwrap_or(model.dims.length - 1);
    let half = model
        .deck_cells
        .iter()
        .filter(|p| p.x == bow_x)
        .map(|p| p.y.abs())
        .max()
        .unwrap_or(0);
    (bow_x, half)
}

/// A bowsprit spar projecting forward over the bow (~⅓ the hull length), angling
/// up from the rail — the guides put it a bit under half the hull length.
async fn place_bowsprit(
    editor: &Editor,
    placer: &mut MaterialPlacer<'_>,
    model: &HullModel,
    placement: &Placement,
) {
    let (bow_x, _) = bow_station(model);
    let reach = (model.dims.length / 3).clamp(3, 12);
    for i in 1..=reach {
        let y = rail_top(model) + (i + 1) / 3; // gentle upward angle
        let local = Point3D::new(bow_x + i, y, 0);
        placer
            .place_block_forced(editor, placement.to_world(local), BlockForm::Block, None, None)
            .await;
    }
}

/// A ship's wheel — a stair on top of a fence post (per the guide) — on the
/// helm platform (quarterdeck if present, else the main deck near the stern).
async fn place_helm(
    editor: &Editor,
    placer: &mut MaterialPlacer<'_>,
    model: &HullModel,
    castle: Option<CastleInfo>,
    placement: &Placement,
) {
    let (_, top_y, helm_x) = helm_platform(model, castle);
    let fence_state = HashMap::new();
    placer
        .place_block_forced(editor, placement.to_world(Point3D::new(helm_x, top_y + 1, 0)), BlockForm::Fence, Some(&fence_state), None)
        .await;
    let stair_state = HashMap::from([
        ("facing".to_string(), placement.world_cardinal(ShipDir::Stern).to_string()),
    ]);
    placer
        .place_block_forced(editor, placement.to_world(Point3D::new(helm_x, top_y + 2, 0)), BlockForm::Stairs, Some(&stair_state), None)
        .await;
}

/// A lantern standing proud at the stern (atop the quarterdeck rail if present).
async fn place_stern_lantern(editor: &Editor, model: &HullModel, castle: Option<CastleInfo>, placement: &Placement) {
    let (stern_x, top_y, _) = helm_platform(model, castle);
    let y = if castle.is_some() { top_y + 2 } else { rail_top(model) + 1 };
    let local = Point3D::new(stern_x, y, 0);
    let lantern: Block = "minecraft:lantern".into();
    editor.place_block_forced(&lantern, placement.to_world(local)).await;
}

/// A flag on a pole at the very stern, streaming aft (perpendicular to the square
/// sails, as the guide advises).
async fn place_stern_flag(
    editor: &Editor,
    placer: &mut MaterialPlacer<'_>,
    model: &HullModel,
    castle: Option<CastleInfo>,
    placement: &Placement,
) {
    let (stern_x, top_y, _) = helm_platform(model, castle);
    let base_y = if castle.is_some() { top_y + 1 } else { rail_top(model) };
    let fence_state = HashMap::new();
    for h in 0..3 {
        placer
            .place_block_forced(editor, placement.to_world(Point3D::new(stern_x, base_y + h, 0)), BlockForm::Fence, Some(&fence_state), None)
            .await;
    }
    // A small banner of wool trailing aft (toward -x) at the masthead of the pole.
    let flag: Block = "minecraft:red_wool".into();
    let top = base_y + 2;
    for dx in 1..=2 {
        for dy in 0..=1 {
            editor.place_block_forced(&flag, placement.to_world(Point3D::new(stern_x - dx, top - dy, 0))).await;
        }
    }
}

/// An anchor chain hanging off the bow into the water.
async fn place_bow_anchor(editor: &Editor, model: &HullModel, placement: &Placement) {
    let (bow_x, half) = bow_station(model);
    let mut chain = Block::from_id("minecraft:chain".into());
    chain.state = Some(HashMap::from([("axis".to_string(), "y".to_string())]));
    for y in (model.waterline_y - 1).max(0)..=model.deck_y {
        let local = Point3D::new(bow_x, y, half + 1);
        editor.place_block_forced(&chain, placement.to_world(local)).await;
    }
}

/// A flat blade hung off the sternpost (local x = -1, centerline), from just
/// below the waterline up to the deck.
async fn place_rudder(
    editor: &Editor,
    placer: &mut MaterialPlacer<'_>,
    model: &HullModel,
    placement: &Placement,
) {
    let top = (model.deck_y - 1).max(model.waterline_y);
    let bottom = (model.waterline_y - 1).max(0);
    for y in bottom..=top {
        let local = Point3D::new(-1, y, 0);
        placer
            .place_block_forced(editor, placement.to_world(local), BlockForm::Block, None, None)
            .await;
    }
}

/// Replace the hatch deck cell with a trapdoor and drop a ladder to the hold
/// floor, backing each rung with a plank so the ladder has support.
async fn place_hatch_and_ladder(
    editor: &Editor,
    placer: &mut MaterialPlacer<'_>,
    model: &HullModel,
    hatch: Point2D,
    placement: &Placement,
) {
    // Trapdoor flush with the deck (top half), so the deck stays walkable.
    let trapdoor_state = HashMap::from([
        ("half".to_string(), "top".to_string()),
        ("facing".to_string(), placement.heading.to_string()),
        ("open".to_string(), "false".to_string()),
    ]);
    let trapdoor_local = Point3D::new(hatch.x, model.deck_y, hatch.y);
    placer
        .place_block_forced(editor, placement.to_world(trapdoor_local), BlockForm::Trapdoor, Some(&trapdoor_state), None)
        .await;

    let floor = match model.hold_floor(hatch.x, hatch.y) {
        Some(f) => f,
        None => return,
    };

    // Back the ladder toward the stern where possible (else bow); the ladder
    // faces away from its backing. Both directions are rotated into world space
    // by the heading so the block state matches the placed backing.
    let (back_dx, facing) = if hatch.x - 1 >= 0 {
        (-1, placement.heading) // backing toward stern, ladder faces the bow
    } else {
        (1, placement.heading.opposite())
    };

    for y in floor..=(model.deck_y - 1) {
        let back_local = Point3D::new(hatch.x + back_dx, y, hatch.y);
        placer
            .place_block_forced(editor, placement.to_world(back_local), BlockForm::Block, None, None)
            .await;

        let ladder = ladder_block(facing);
        let rung_local = Point3D::new(hatch.x, y, hatch.y);
        editor.place_block_forced(&ladder, placement.to_world(rung_local)).await;
    }
}

fn ladder_block(facing: Cardinal) -> Block {
    let mut ladder = Block::from_id("minecraft:ladder".into());
    ladder.state = Some(HashMap::from([("facing".to_string(), facing.to_string())]));
    ladder
}
