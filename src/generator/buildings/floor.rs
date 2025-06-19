use strum::IntoEnumIterator;

use crate::{editor::Editor, generator::{buildings::BuildingData, data::LoadedData, materials::{MaterialPlacer, MaterialRole, Placer}}, geometry::{Cardinal, Point2D, DOWN, X_PLUS, Z_PLUS}, minecraft::BlockForm, noise::RNG};

pub async fn build_floor(editor: &mut Editor, data: &LoadedData, building: &BuildingData, rng: &mut RNG) {

    let wood_id = data.palettes.get(&building.palette).expect("Palette not found").get_material(MaterialRole::SecondaryWood).expect("Secondary wood material not found");
    let mut placer = MaterialPlacer::new(Placer::new(&data.materials, rng), 
        wood_id.clone()
    );

    for cell in building.shape.cells().iter() {
        let mut min : Point2D = Point2D::new(1, 1);
        let mut max : Point2D = building.grid.cell_size.drop_y() - Point2D::new(2, 2);

        for direction in Cardinal::iter() {
            if building.shape.cells().iter().any(|other_cell| *other_cell == *cell + direction.into()) {
                match direction {
                    Cardinal::North => min.y -= 1,
                    Cardinal::East => max.x += 1,
                    Cardinal::South => max.y += 1,
                    Cardinal::West => min.x -= 1,
                }
            }
        }

        for x in min.x..=max.x {
            for z in min.y..=max.y {
                let point = building.grid.grid_to_world(*cell) + X_PLUS * x + Z_PLUS * z + DOWN;
                placer.place_block(editor, point, BlockForm::Block, None, None).await;
            }
        }
    }
}