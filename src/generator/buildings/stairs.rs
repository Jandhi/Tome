use std::collections::HashMap;

use serde_derive::{Deserialize, Serialize};
use crate::{editor::{self, Editor}, generator::{buildings::BuildingData, data::LoadedData, materials::{MaterialPlacer, MaterialRole, Placer}}, geometry::{Cardinal, Point3D, UP}, minecraft::{BlockForm, BlockID}, noise::RNG};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StairPlacement {
    pub cell : Point3D,
    pub direction : Cardinal,
    #[serde(default)]
    pub left_to_right : bool,
}

pub async fn build_stairs(editor: &mut Editor, building: &BuildingData, data: &LoadedData, rng: &mut RNG) {
    let stairs = building.shape.stairs().expect("Building shape must have stairs defined");

    for stair in stairs {
        build_stair(editor, building, data, stair.cell, stair.direction, rng, stair.left_to_right).await;
    }
}

pub async fn build_stair(editor : &mut Editor, building : &BuildingData, data : &LoadedData, cell : Point3D, direction : Cardinal, rng : &mut RNG, left_to_right : bool) {
    let wood_id = data.palettes.get(&building.palette)
        .and_then(|palette| palette.get_material(MaterialRole::SecondaryWood))
        .expect("Wood block not found in palette");

    let mut placer = MaterialPlacer::new(Placer::new(&data.materials, rng), wood_id.clone());

    let ref_point = building.grid.get_door_world_position(cell, direction);

    let inner_vec : Point3D = (-direction).into();

    let left_vec = if left_to_right {
        inner_vec.rotate_left()
    } else {
        inner_vec.rotate_right()
    };

    let facing : String = if left_to_right {
        direction.rotate_right().to_string()
    } else {
        direction.rotate_left().to_string()
    };

    let facing_away = if left_to_right {
        direction.rotate_left().to_string()
    } else {
        direction.rotate_right().to_string()
    };

    // First air block
    editor.place_block_forced(&BlockID::Air.into(), ref_point + inner_vec + left_vec * 2 + UP * (building.grid.cell_size.y - 2)).await;

    for i in 0..building.grid.cell_size.y - 1 {
        // Clear air
        editor.place_block_forced(&BlockID::Air.into(), ref_point + inner_vec + left_vec * (1 - i) + UP * (building.grid.cell_size.y - 2)).await;

        // Stairs
        placer.place_block_forced(
            editor,
            ref_point + inner_vec + left_vec * (1 - i) + UP * i,
            BlockForm::Stairs,
            Some(&HashMap::from([("facing".to_string(), facing.clone())])),
            None
        ).await;

        // Underside- not forced so we don't overwrite the bottom floor
        placer.place_block(
            editor,
            ref_point + inner_vec + left_vec * (1 - i) + UP * (i - 1),
            BlockForm::Stairs,
            Some(&HashMap::from([("facing".to_string(), facing_away.clone()), ("half".to_string(), "top".to_string())])),
            None
        ).await;
    }

    
}