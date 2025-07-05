use std::collections::HashMap;

use strum::IntoEnumIterator;

use crate::{editor::{self, Editor}, generator::{buildings::BuildingData, data::LoadedData, materials::{MaterialPlacer, MaterialRole, Palette, Placer}}, geometry::{get_outer_edge, Cardinal, UP}, minecraft::{BlockForm, BlockID}, noise::RNG};

pub async fn build_foundation(
    editor : &mut Editor,
    building : &BuildingData,
    data : &LoadedData,
    rng : &mut RNG,
) {
    let BuildingData { grid, shape, palette, .. } = building;

    let palette = data.palettes.get(palette)
        .expect("Palette not found")
        .clone();

    let area = shape.get_footprint(grid);
    let edge = get_outer_edge(&area);

    let mut placer_rng = rng.derive();
    let mut primary_stone_placer : MaterialPlacer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng), 
        palette.get_material(MaterialRole::PrimaryStone).expect("Primary stone material not found").clone()
    );

    for point in area.union(&edge).into_iter() {
        let height = editor.world().get_ocean_floor_height_at(*point);
        let grid_height = grid.origin.y; 
        if height >= grid_height - 1 {
            let block = editor.world().get_block(point.add_y(height - 1)).expect("Block not found at point");
            editor.place_block(&block, point.add_y(grid_height - 1)).await;
        } else if height < grid_height {
            if edge.contains(point) {
                let out_direction = Cardinal::iter()
                    .find(|dir| {
                        let neighbour = *point + (*dir).into();
                        !area.contains(&neighbour) && !edge.contains(&neighbour)
                    }).unwrap_or(Cardinal::North);

                let mut state : HashMap<String, String> = HashMap::new();
                state.insert("facing".to_string(), (-out_direction).to_string());
                state.insert("half".to_string(), "top".to_string());
                primary_stone_placer.place_block(editor, point.add_y(grid_height - 1), BlockForm::Stairs, Some(&state), None).await;

                continue;
            }


            for y in height..grid_height {
                primary_stone_placer.place_block(editor, point.add_y(y), BlockForm::Block, None, None).await;
            }
        }
    }
}