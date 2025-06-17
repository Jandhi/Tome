#[cfg(test)]
mod tests {

    use std::env;

    use log::info;

    use crate::{editor::World, generator::{buildings::{build_stairs, floor::build_floor, roofs::build_roof, shape::BuildingShape, stairs::StairPlacement, walls::wall::build_walls, BuildingData, Grid}, data::LoadedData, materials::PaletteId, style::Style}, geometry::{Cardinal, Point3D}, http_mod::GDMCHTTPProvider, minecraft::BlockID, noise::RNG, util::{build_compass, init_logger}};


    #[tokio::test]
    async fn test_build_walls() {
        init_logger();
        
        let world = World::new(&GDMCHTTPProvider::new()).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let data = LoadedData::load().expect("Failed to load generator data");
        let palette : PaletteId = "desert_prismarine".into();

        let shape = BuildingShape::new(
            vec![
            // Base layer
            Point3D::new(0, 0, 0),
            // Point3D::new(1, 0, 0),
            // Point3D::new(0, 0, 1),
            // // Second layer
            // Point3D::new(0, 1, 0),
            // Point3D::new(1, 1, 0),
            // Point3D::new(0, 1, 1),
            ],
            Some(vec![
                // Stairs at the center of the base layer
                // StairPlacement { cell: Point3D::new(1, 0, 0), direction: Cardinal::South, left_to_right: false },
                // StairPlacement { cell: Point3D::new(0, 0, 1), direction: Cardinal::East, left_to_right: false },
            ]),
        );

        let midpoint = editor.world_mut().world_rect_2d().size / 2;
        let point = editor.world_mut().add_height(midpoint);

        let grid = Grid::new(point.into());

        let walls = &data.walls;
        let building = BuildingData{
            id: 0.into(),
            shape,
            grid,
            palette: palette.clone(),
            style: Style::Desert,
        };

        for cell in building.shape.cells().iter() {
            let midpoint = building.grid.grid_to_world(*cell) + building.grid.cell_size / 2;
            editor.place_block(&BlockID::RedMushroomBlock.into(), midpoint).await;
        }

        let mut rng = RNG::new(100.into());

        build_walls(&mut editor, &walls.values().collect::<Vec<_>>(), &building, &data, &mut rng).await.expect("Failed to build walls");
        build_roof(&mut editor, &data, &building, &mut rng).await.expect("Failed to build roof");        
        build_floor(&mut editor, &data, &building, &mut rng).await;
        build_stairs(&mut editor, &building, &data, &mut rng).await;

        build_compass(&mut editor).await;


        editor.flush_buffer().await;
    }
}