//! Mast assembly: a keel-stepped pole that thins to a fenced topmast, the yards
//! (shortening with height), and a crow's nest platform near the top.

use std::collections::HashMap;

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::Point3D;
use crate::minecraft::BlockForm;
use crate::noise::RNG;

use super::super::Placement;
use super::RigModel;

/// Raise each mast: keel-stepped pole, thinning topmast, yards, and crow's nest.
pub async fn place_masts(
    editor: &Editor,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
    rig: &RigModel,
    placement: &Placement,
) {
    let pole_mat = palette
        .get_material(MaterialRole::WoodPillar)
        .or_else(|| palette.get_material(MaterialRole::PrimaryWood))
        .expect("palette has no wood for mast")
        .clone();
    let deck_mat = palette
        .get_material(MaterialRole::SecondaryWood)
        .unwrap_or(&pole_mat)
        .clone();

    let mut pole_rng = rng.derive();
    let mut deck_rng = rng.derive();
    let mut pole = MaterialPlacer::new(Placer::new(&data.materials, &mut pole_rng), pole_mat);
    let mut nest = MaterialPlacer::new(Placer::new(&data.materials, &mut deck_rng), deck_mat);
    let fence_state = HashMap::new();
    let top_slab = HashMap::from([("type".to_string(), "top".to_string())]);

    for mast in &rig.masts {
        let x = mast.base.x;

        // Lower mast: solid logs from the keel-stepped foot up to the nest.
        for y in mast.foot_y..=mast.nest_y {
            let local = Point3D::new(x, y, 0);
            pole.place_block_forced(editor, placement.to_world(local), BlockForm::Block, None, None).await;
        }
        // Topmast: a thinner fenced spar above the nest.
        for y in (mast.nest_y + 1)..=mast.top_y {
            let local = Point3D::new(x, y, 0);
            pole.place_block(editor, placement.to_world(local), BlockForm::Fence, Some(&fence_state), None).await;
        }

        // Yards across the beam at each tier.
        for yard in &mast.yards {
            for z in -yard.half..=yard.half {
                let local = Point3D::new(x, yard.y, z);
                pole.place_block_forced(editor, placement.to_world(local), BlockForm::Block, None, None).await;
            }
        }

        // Crow's nest: a 3×3 slab platform around the mast at nest height, with a
        // fence rim — the "shooting platform" the guides describe.
        for dx in -1..=1 {
            for dz in -1..=1 {
                if dx == 0 && dz == 0 {
                    continue; // mast passes through
                }
                let floor = Point3D::new(x + dx, mast.nest_y, dz);
                nest.place_block_forced(editor, placement.to_world(floor), BlockForm::Slab, Some(&top_slab), None).await;
                // Rim fence on the outer ring.
                if dx.abs() == 1 || dz.abs() == 1 {
                    let rim = Point3D::new(x + dx, mast.nest_y + 1, dz);
                    nest.place_block(editor, placement.to_world(rim), BlockForm::Fence, Some(&fence_state), None).await;
                }
            }
        }
    }
}
