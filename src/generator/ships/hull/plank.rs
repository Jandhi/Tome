//! Hull planking: place the shell cells from a [`HullModel`], plus the slab keel
//! and internal rib posts.
//!
//! Reuse note (plan decision #4): `buildings_v2::roof::blocks::place_roof_blocks`
//! is tightly coupled to `RoofHeightmap` + `GablePitch`, so it can't be called for
//! a hull shell. We instead mirror its *technique* — stairs on sloped surfaces,
//! full blocks on vertical ones — driven by the per-cell classification the hull
//! model computed (`HullPlank::cut`). The stair's solid side faces inboard, so
//! beveling an outward corner keeps the hull watertight toward the hold.

use std::collections::HashMap;

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::minecraft::BlockForm;
use crate::noise::RNG;

use super::super::Placement;
use super::HullModel;

/// Place every below-deck shell cell as hull planking (blocks + stair bevels).
/// The top strake (`deck_y - 1`) uses the secondary wood as a contrasting wale.
pub async fn plank_hull(
    editor: &Editor,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
    model: &HullModel,
    placement: &Placement,
) {
    let primary = palette
        .get_material(MaterialRole::PrimaryWood)
        .expect("palette has no primary wood for hull")
        .clone();
    let secondary = palette
        .get_material(MaterialRole::SecondaryWood)
        .unwrap_or(&primary)
        .clone();

    let mut primary_rng = rng.derive();
    let mut secondary_rng = rng.derive();
    let mut primary_placer =
        MaterialPlacer::new(Placer::new(&data.materials, &mut primary_rng), primary);
    let mut secondary_placer =
        MaterialPlacer::new(Placer::new(&data.materials, &mut secondary_rng), secondary);

    let wale_y = model.deck_y - 1;
    for plank in &model.hull_cells {
        let world = placement.to_world(plank.local);
        let placer = if plank.local.y == wale_y { &mut secondary_placer } else { &mut primary_placer };
        match plank.cut {
            Some((out, top_half)) => {
                let facing = placement.world_cardinal(out.opposite());
                let state = HashMap::from([
                    ("facing".to_string(), facing.to_string()),
                    ("half".to_string(), if top_half { "top" } else { "bottom" }.to_string()),
                ]);
                placer.place_block(editor, world, BlockForm::Stairs, Some(&state), None).await;
            }
            None => {
                placer.place_block(editor, world, BlockForm::Block, None, None).await;
            }
        }
    }
}

/// Place the keel: a centerline line of top slabs just under the hull bottom.
pub async fn place_keel(
    editor: &Editor,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
    model: &HullModel,
    placement: &Placement,
) {
    let material = palette
        .get_material(MaterialRole::SecondaryWood)
        .or_else(|| palette.get_material(MaterialRole::PrimaryWood))
        .expect("palette has no wood for keel")
        .clone();
    let mut placer_rng = rng.derive();
    let mut placer = MaterialPlacer::new(Placer::new(&data.materials, &mut placer_rng), material);

    let top_slab = HashMap::from([("type".to_string(), "top".to_string())]);
    for &cell in &model.keel_slabs {
        placer
            .place_block(editor, placement.to_world(cell), BlockForm::Slab, Some(&top_slab), None)
            .await;
    }
}

/// Place the internal rib posts as vertical logs hugging the hull inside.
pub async fn place_frames(
    editor: &Editor,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
    model: &HullModel,
    placement: &Placement,
) {
    let material = palette
        .get_material(MaterialRole::WoodPillar)
        .or_else(|| palette.get_material(MaterialRole::PrimaryWood))
        .expect("palette has no wood for frames")
        .clone();
    let mut placer_rng = rng.derive();
    let mut placer = MaterialPlacer::new(Placer::new(&data.materials, &mut placer_rng), material);

    for &post in &model.frame_posts {
        placer
            .place_block(editor, placement.to_world(post), BlockForm::Block, None, None)
            .await;
    }
}
