#[cfg(test)]
mod tests {

    use log::info;

    use crate::{editor::World, generator::{buildings::{roofs::build_roof, shape::BuildingShape, walls::wall::build_walls, BuildingData, Grid}, data::LoadedData, materials::PaletteId}, geometry::Point3D, http_mod::GDMCHTTPProvider, minecraft::BlockID, util::{build_compass, init_logger}};


    #[tokio::test]
    async fn test_build_walls() {
        init_logger();
        
        let world = World::new(&GDMCHTTPProvider::new()).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let data = LoadedData::load().expect("Failed to load generator data");
        let palette : PaletteId = "test2".into();

        let shape = BuildingShape::new(
            vec![Point3D::new(0, 0, 0), Point3D::new(1, 0, 0), Point3D::new(2, 0, 0), Point3D::new(0, 0, 1)]
        );

        let midpoint = editor.world().world_rect_2d().size / 2;
        let point = editor.world().add_height(midpoint);

        info!("Placing structure at: {:?}", point);

        let grid = Grid::new(point.into());

        let walls = &data.walls;
        let building = BuildingData{
            id: 0.into(),
            shape,
            grid,
            palette: palette.clone()
        };

        for cell in building.shape.cells().iter() {
            let midpoint = building.grid.grid_to_world(*cell) + building.grid.cell_size / 2;
            editor.place_block(&BlockID::RedMushroomBlock.into(), midpoint).await;
        }

        build_walls(&mut editor, &walls.values().collect::<Vec<_>>(), &building, &data).await.expect("Failed to build walls");

        build_roof(&mut editor, &data, &building).await.expect("Failed to build roof");        

        build_compass(&mut editor).await;
    }
}