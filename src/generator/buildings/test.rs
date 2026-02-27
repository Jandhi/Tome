
#[cfg(test)]
mod tests {

    use log::info;

    use crate::{data::Loadable, editor::World, generator::{buildings::{Grid, placement::{place_building, place_buildings}, shape::{BuildingShape, WallPlacement}, stairs::StairPlacement, walls::WallComponent}, chronicle::{SettlementInfo, generate_chronicle}, data::LoadedData, districts::{WallType, build_wall, generate_districts}, materials::{Material, MaterialId, Palette, Placer}, nbts::Structure, style::Style, terrain::log_trees}, geometry::{Cardinal, NORTH, Point3D, UP}, http_mod::GDMCHTTPProvider, noise::RNG, util::{build_compass, init_logger}};


    #[tokio::test]
    async fn grid_placement() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();


        let data = LoadedData::load().expect("Failed to load generator data");

        let palette = data.palettes.get(&"test2".into()).expect("Palette not found").clone();

        let midpoint = editor.world_mut().world_rect_2d().size / 2;
        let point = editor.world_mut().add_height(midpoint);

        println!("Placing structure at: {:?}", point);

        let grid = Grid::new(point.into());

        let structures = Structure::load().expect("Failed to load structures");
        let structure = structures.get(&"rotation_test".into()).expect("Structure not found");

        let mut rng = RNG::new(42);
        let mut placer = Placer::new(&data.materials, &mut rng);

        grid.build_structure(&mut editor, &mut placer, &structure, Point3D::new(0, 0, 0), Cardinal::North, &data, &palette).await
            .expect("Failed to build structure");
        grid.build_structure(&mut editor, &mut placer, &structure, Point3D::new(0, 1, 0), Cardinal::East, &data, &palette).await
            .expect("Failed to build structure");
        grid.build_structure(&mut editor, &mut placer, &structure, Point3D::new(0, 2, 0), Cardinal::South, &data, &palette).await
            .expect("Failed to build structure");
        grid.build_structure(&mut editor, &mut placer, &structure, Point3D::new(0, 3, 0), Cardinal::West, &data, &palette).await
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

        let walls = WallComponent::load().expect("Failed to load structures");
        let wall = walls.get(&"japanese_wall_single_plain".into()).expect("Structure not found");
        let door_wall = walls.get(&"japanese_wall_single_plain_door".into()).expect("Structure not found");
        
        let mut rng = RNG::new(42);
        let mut placer = Placer::new(&data.materials, &mut rng);

        let palette = data.palettes.get(&"test1".into()).expect("Palette not found").clone();

        grid.build_structure(&mut editor, &mut placer, &door_wall.structure, Point3D::new(0, 0, 0), Cardinal::North, &data, &palette).await
            .expect("Failed to build structure");
        grid.build_structure(&mut editor, &mut placer, &wall.structure, Point3D::new(0, 0, 0), Cardinal::South, &data, &palette).await
            .expect("Failed to build structure");
        grid.build_structure(&mut editor, &mut placer, &wall.structure, Point3D::new(0, 0, 0), Cardinal::East, &data, &palette).await
            .expect("Failed to build structure");
        grid.build_structure(&mut editor, &mut placer, &wall.structure, Point3D::new(0, 0, 0), Cardinal::West, &data, &palette).await
            .expect("Failed to build structure");

        info!("NBT structure placed successfully");

        editor.place_block(&"red_wool".into(), point + NORTH * 10 + UP * 5).await;
        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn placement() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();

        let midpoint = editor.world_mut().world_rect_2d().size / 2;
        let point = editor.world_mut().add_height(midpoint);

        let shape = BuildingShape::new( 
            vec![
                Point3D::new(0, 0, 0), 
                Point3D::new(0, 1, 0),
                Point3D::new(1, 1, 0),
                Point3D::new(2, 1, 0),
                Point3D::new(2, 0, 0),
            ],
            Some(vec![
                StairPlacement {
                    cell: Point3D::new(0, 0, 0),
                    direction: Cardinal::North,
                    left_to_right: true,
                },
                StairPlacement {
                    cell: Point3D::new(2, 0, 0),
                    direction: Cardinal::North,
                    left_to_right: false,
                },
            ]),
            Some(vec![WallPlacement {
                cell: Point3D::new(0, 0, 0),
                direction: Cardinal::South,
            }])
        );

        let grid = Grid::new(point.into());

        let set = "medieval_stone_tudor".into();
        let data = LoadedData::load().expect("Failed to load generator data");
        let rng = &mut RNG::new(65);

        place_building(&mut editor, &shape, grid, &set, &data, Style::Medieval, rng, data.palettes.get(&"medieval_spruce".into()).expect("")).await;

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn placement_in_districts() {
        println!("Running placement_in_districts test");
        dotenv::dotenv().ok();
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();

        let mut rng = RNG::new(32);

        generate_districts(rng.next_i64().into(), &mut editor).await;
        let mut info = SettlementInfo::new(editor.world());

        let data = LoadedData::load().expect("Failed to load generator data");

        let materials = Material::load().expect("Failed to load materials");
        let material = MaterialId::new("spruce_planks".to_string());

        let mut placer_rng = rng.derive();
        let mut placer: Placer = Placer::new(
            &materials,
            &mut placer_rng,
        );
        let urban_points = &editor.world().get_urban_points();
        log_trees(&mut editor, urban_points.clone()).await;

        place_buildings(&mut editor, &mut rng.derive(), &data, Style::Medieval, vec![&"medieval_spruce".into()], &info).await;
        info = SettlementInfo::new(editor.world());
        build_wall(urban_points, &mut editor, &mut rng.derive(), &mut placer, &material, &data.structures, WallType::Palisade).await;
        let _ = generate_chronicle(&mut editor, &mut info).await;
        editor.flush_buffer().await;
    }
    
}