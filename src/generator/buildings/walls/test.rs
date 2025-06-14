#[cfg(test)]
mod tests {

    use log::info;

    use crate::{data::Loadable, editor::World, generator::{buildings::{shape::BuildingShape, walls::{wall::build_walls, Wall}, BuildingData, Grid}, data::LoadedData, materials::{Material, Palette, PaletteId, Placer}, nbts::Structure}, geometry::{Cardinal, Point3D, NORTH, UP}, http_mod::GDMCHTTPProvider, minecraft::BlockID, util::init_logger};


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

        println!("Placing structure at: {:?}", point);

        let grid = Grid::new(point.into());

        let walls = &data.walls;

        build_walls(&mut editor, &walls.values().collect::<Vec<_>>(), &BuildingData{
            id: 0.into(),
            shape,
            grid,
        }, &data, &palette).await.expect("Failed to build walls");
    }
}