
#[cfg(test)]
mod tests {

    use log::info;

    use crate::{data::Loadable, editor::World, generator::{buildings::{walls::Wall, Grid}, data::LoadedData, materials::{Material, Palette, Placer}, nbts::Structure}, geometry::{Cardinal, Point3D, NORTH, UP}, http_mod::GDMCHTTPProvider, minecraft::BlockID, util::init_logger};


    #[tokio::test]
    async fn grid_placement() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();

        let data = LoadedData::load().expect("Failed to load generator data");

        let palette = "test2".into();

        let midpoint = editor.world_mut().world_rect_2d().size / 2;
        let point = editor.world_mut().add_height(midpoint);

        println!("Placing structure at: {:?}", point);

        let grid = Grid::new(point.into());

        let structures = Structure::load().expect("Failed to load structures");
        let structure = structures.get(&"rotation_test".into()).expect("Structure not found");

        let placer = Placer::new(&data.materials);

        grid.build_structure(&mut editor, &placer, &structure, Point3D::new(0, 0, 0), Cardinal::North, &data, &palette).await
            .expect("Failed to build structure");
        grid.build_structure(&mut editor, &placer, &structure, Point3D::new(0, 1, 0), Cardinal::East, &data, &palette).await
            .expect("Failed to build structure");
        grid.build_structure(&mut editor, &placer, &structure, Point3D::new(0, 2, 0), Cardinal::South, &data, &palette).await
            .expect("Failed to build structure");
        grid.build_structure(&mut editor, &placer, &structure, Point3D::new(0, 3, 0), Cardinal::West, &data, &palette).await
            .expect("Failed to build structure");

        info!("NBT structure placed successfully");

        editor.flush_buffer().await;
    }


    #[tokio::test]
    async fn grid_placement_wall() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();

        let midpoint = editor.world_mut().world_rect_2d().size / 2;
        let point = editor.world_mut().add_height(midpoint);

        println!("Placing structure at: {:?}", point);

        let grid = Grid::new(point.into());

        let data = LoadedData::load().expect("Failed to load generator data");

        let walls = Wall::load().expect("Failed to load structures");
        let wall = walls.get(&"japanese_wall_single_plain".into()).expect("Structure not found");
        let door_wall = walls.get(&"japanese_wall_single_plain_door".into()).expect("Structure not found");
        
        let placer = Placer::new(&data.materials);

        grid.build_structure(&mut editor, &placer, &door_wall.structure, Point3D::new(0, 0, 0), Cardinal::North, &data, &"test1".into()).await
            .expect("Failed to build structure");
        grid.build_structure(&mut editor, &placer, &wall.structure, Point3D::new(0, 0, 0), Cardinal::South, &data, &"test1".into()).await
            .expect("Failed to build structure");
        grid.build_structure(&mut editor, &placer, &wall.structure, Point3D::new(0, 0, 0), Cardinal::East, &data, &"test1".into()).await
            .expect("Failed to build structure");
        grid.build_structure(&mut editor, &placer, &wall.structure, Point3D::new(0, 0, 0), Cardinal::West, &data, &"test1".into()).await
            .expect("Failed to build structure");

        info!("NBT structure placed successfully");

        editor.place_block(&BlockID::RedWool.into(), point + NORTH * 10 + UP * 5).await;
        editor.flush_buffer().await;
    }
    
}