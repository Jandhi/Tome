use std::collections::HashMap;

use strum::IntoEnumIterator;

use crate::{editor::Editor, generator::{buildings::BuildingData, data::LoadedData, materials::{MaterialPlacer, MaterialRole, Placer}}, geometry::{get_outer_edge, Cardinal}, minecraft::BlockForm, noise::RNG};

pub async fn build_foundation(
    editor: &Editor,
    building : &BuildingData,
    data : &LoadedData,
    rng : &mut RNG,
) {
    let BuildingData { grid, shape, palette, .. } = building;

    let area = shape.get_footprint(grid);
    let edge = get_outer_edge(&area);

    let mut placer_rng = rng.derive();
    let mut primary_stone_placer : MaterialPlacer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng), 
        palette.get_material(MaterialRole::PrimaryStone).expect("Primary stone material not found").clone()
    );

    let mut second_placer_rng = rng.derive();
    let mut secondary_stone_placer : MaterialPlacer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut second_placer_rng), 
        palette.get_material(MaterialRole::SecondaryStone).expect("Secondary stone material not found").clone()
    );

    for point in area.union(&edge).into_iter() {
        let Some(height) = editor.world().get_ocean_floor_height_at(*point) else {
            continue;
        };
        let grid_height = grid.origin.y;
        if height >= grid_height - 1 {
            let block = editor.world().get_block(point.add_y(height - 1)).expect("Block not found at point");
            editor.place_block(&block, point.add_y(grid_height - 1)).await;
        } else if height < grid_height {
            let relative = *point - grid.origin.drop_y();
            let placer = if (relative.x) % (grid.cell_size.x) == 0 || (relative.y) % (grid.cell_size.z) == 0 {
                // Place secondary stone for the outer edge
                &mut secondary_stone_placer
            } else {
                // Place primary stone for the outer edge
                &mut primary_stone_placer
            };

            if edge.contains(point) {
                let out_direction = Cardinal::iter()
                    .find(|dir| {
                        let neighbour = *point + (*dir).into();
                        !area.contains(&neighbour) && !edge.contains(&neighbour)
                    }).unwrap_or(Cardinal::North);

                let mut state : HashMap<String, String> = HashMap::new();
                state.insert("facing".to_string(), (-out_direction).to_string());
                state.insert("half".to_string(), "top".to_string());



                placer.place_block(editor, point.add_y(grid_height - 1), BlockForm::Stairs, Some(&state), None).await;

                continue;
            }


            for y in height..grid_height {
                placer.place_block(editor, point.add_y(y), BlockForm::Block, None, None).await;
            }
        }
    }
}