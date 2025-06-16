#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use crate::{data::Loadable, editor::World, generator::districts::{build_wall, WallType, district::{self, generate_districts}, district_painter::{replace_ground, replace_ground_smooth}, super_district, wall}, geometry::{Point2D, Point3D}, http_mod::{GDMCHTTPProvider, HeightMapType}, minecraft::{Block, BlockID}, noise::{Seed, RNG}, util::init_logger};
    use crate::generator::materials::{Placer, Material, MaterialId};
    use crate::generator::nbts::Structure;

    fn get_block_for_id(id : usize) -> Block {
        use BlockID::*;
        // List of all 16 wool colors in order
        let wool_colors = [
            WhiteWool, OrangeWool, MagentaWool, LightBlueWool,
            YellowWool, LimeWool, PinkWool, GrayWool,
            LightGrayWool, CyanWool, PurpleWool, BlueWool,
            BrownWool, GreenWool, RedWool, BlackWool,
        ];
        Block {
            id: wool_colors[id % wool_colors.len()],
            data: None,
            state: None,
        }
    }

    fn get_block_for_district_type(district_type: district::DistrictType) -> Block {
        use BlockID::*;
        match district_type {
            district::DistrictType::Urban => Block { id: BlueWool, data: None, state: None },
            district::DistrictType::Rural => Block { id: GreenWool, data: None, state: None },
            district::DistrictType::OffLimits => Block { id: RedWool, data: None, state: None },
            _ => Block { id: Bedrock, data: None, state: None }, // Default case for unknown types
        }
    }

    #[tokio::test]
    async fn district_test() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);

        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        println!("Build area: {:?}", build_area);
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::WorldSurface).await.expect("Failed to get heightmap");
        
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _districts = generate_districts(seed, &mut editor).await;
        let glass = Block {
            id: BlockID::Glass,
            data: None,
            state: None,
        };

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                let district_id = editor.world().district_map[x as usize][z as usize];

                let Some(district_id) = district_id else {
                    continue;
                };
                

                let block = get_block_for_id(district_id.0 as usize);
                let height = height_map[x as usize][z as usize] - build_area.origin.y;
                let point = Point3D::new(x, height, z);
                //editor.place_block(&block, Point3D::new(x, height - build_area.origin.y, z)).await;
                if let Some(district) = editor.world().districts.get(&district_id) {
                    
                    if district.data.edges.contains(&point) {
                        editor.place_block(&glass, Point3D::new(x, height , z)).await;
                        editor.place_block(&block, Point3D::new(x, height - 1, z)).await;
                    } else {
                        editor.place_block(&block, Point3D::new(x, height, z)).await;
                    }
                }
            }
        }

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn superdistrict_test() {
        init_logger();
        println!("hello");

        // Initialize the test data
        let seed = Seed(12345);

        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::WorldSurface).await.expect("Failed to get heightmap");
        
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _districts = generate_districts(seed, &mut editor).await;

        let glass = Block {
            id: BlockID::Glass,
            data: None,
            state: None,
        };
        let bedrock = Block {
            id: BlockID::Bedrock,
            data: None,
            state: None,
        };

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                let super_district_id = editor.world().super_district_map[x as usize][z as usize];
                let district_id = editor.world().district_map[x as usize][z as usize];

                let Some(district_id) = district_id else {
                    continue;
                };
                let Some(super_district_id) = super_district_id else {
                    continue;
                };

                let block = get_block_for_id(super_district_id.0 as usize);
                let height = height_map[x as usize][z as usize] - build_area.origin.y;
                let point = Point3D::new(x, height, z);

                let World {districts,super_districts, .. } = editor.world();

                let super_district = super_districts.get(&super_district_id).expect("Failed to get super district");
                let district = districts.get(&district_id).expect("Failed to get district");
                if super_district.data.edges.contains(&point) {
                    editor.place_block(&bedrock, Point3D::new(x, height, z)).await;
                }
                else if district.data.edges.contains(&point) {
                    editor.place_block(&glass, Point3D::new(x, height, z)).await;
                    editor.place_block(&block, Point3D::new(x, height - 1, z)).await;
                }
                else {
                    editor.place_block(&block, Point3D::new(x, height, z)).await;
                }

            }
        }

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn superdistrict_classification() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);

        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        println!("Build area: {:?}", build_area);
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::WorldSurface).await.expect("Failed to get heightmap");
        
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _districts = generate_districts(seed, &mut editor).await;
        let glass = Block {
            id: BlockID::Glass,
            data: None,
            state: None,
        };
        let bedrock  = Block {
            id: BlockID::Bedrock,
            data: None,
            state: None,
        };

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                let super_district_id = editor.world().super_district_map[x as usize][z as usize];
                let district_id = editor.world().district_map[x as usize][z as usize];

                let Some(district_id) = district_id else {
                    continue;
                };
                let Some(super_district_id) = super_district_id else {
                    continue;
                };

                let block = get_block_for_district_type(editor.world().super_districts.get(&super_district_id).expect("Failed to get district").data.district_type);
                let height = height_map[x as usize][z as usize] - build_area.origin.y;
                let point = Point3D::new(x, height, z);

                let World {districts,super_districts, .. } = editor.world();
                let super_district = super_districts.get(&super_district_id).expect("Failed to get super district");
                let district = districts.get(&district_id).expect("Failed to get district");

                if super_district.data.edges.contains(&point) {
                    editor.place_block(&bedrock, Point3D::new(x, height, z)).await;
                }
                else if district.data.edges.contains(&point) {
                    editor.place_block(&glass, Point3D::new(x, height, z)).await;
                    editor.place_block(&block, Point3D::new(x, height - 1, z)).await;
                }
                else {
                    editor.place_block(&block, Point3D::new(x, height, z)).await;
                }

            }
        }

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn district_classification() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);

        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        println!("Build area: {:?}", build_area);
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::MotionBlockingNoPlants).await.expect("Failed to get heightmap");
        
        let mut world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _districts = generate_districts(seed, &mut editor).await;
        let glass = Block {
            id: BlockID::Glass,
            data: None,
            state: None,
        };

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                let district_id = editor.world().district_map[x as usize][z as usize];

                let Some(district_id) = district_id else {
                    continue;
                };
                

                let block = get_block_for_district_type(editor.world().districts.get(&district_id).expect("Failed to get district").data.district_type);
                let height = height_map[x as usize][z as usize] - build_area.origin.y;
                let point = Point3D::new(x, height, z);
                //editor.place_block(&block, Point3D::new(x, height - build_area.origin.y, z)).await;
                if let Some(district) = editor.world().districts.get(&district_id) {
                    
                    if district.data.edges.contains(&point) {
                        editor.place_block(&glass, Point3D::new(x, height , z)).await;
                        editor.place_block(&block, Point3D::new(x, height - 1, z)).await;
                    } else {
                        editor.place_block(&block, Point3D::new(x, height, z)).await;
                    }
                }
            }
        }

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn district_replace_ground() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");

        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _districts = generate_districts(seed, &mut editor).await;

        use BlockID::*;
        let block_vec : Vec<Block> = vec![
            Stone, Cobblestone, StoneBricks, Andesite, Gravel,
        ].into_iter().map(|id| Block { id, data: None, state: None }).collect();

        let block_dict: HashMap<u32, f32> = [
            (0, 3.0),  // Stone
            (1, 2.0),  // Cobblestone
            (2, 8.0),  // Stone Bricks
            (3, 3.0),  // Andesite
            (4, 1.0),  // Gravel
        ].into_iter().collect();

        let mut road_points = HashSet::new();

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                road_points.insert(Point2D::new(x, z));
            }
        }

        replace_ground(
            &road_points,
            &block_dict,
            &block_vec,
            &mut rng,
            &mut editor,
            Some(0),
            None, // No permit blocks
            Some(false), // Ignore water
        ).await;

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn district_replace_ground_smooth() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");

        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _districts = generate_districts(seed, &mut editor).await;

        use BlockID::*;
        let block_vec : Vec<Block> = vec![
            Stone, Cobblestone, StoneBricks, Andesite, Gravel,
            StoneStairs, CobblestoneStairs, StoneBrickStairs, AndesiteStairs,
            StoneSlab, CobblestoneSlab, StoneBrickSlab, AndesiteSlab,
        ].into_iter().map(|id| Block { id, data: None, state: None }).collect();

        let mut blocks_dict: HashMap<u32, HashMap<u32, f32>> = HashMap::new();

        let block_dict = [
            (0, 3.0),  // Stone
            (1, 2.0),  // Cobblestone
            (2, 8.0),  // Stone Bricks
            (3, 3.0),  // Andesite
            (4, 1.0),  // Gravel
        ].into_iter().collect();
        blocks_dict.insert(0, block_dict);

        let stair_dict = [
            (5, 3.0),  // Stone stairs
            (6, 2.0),  // Cobblestone stairs
            (7, 8.0),  // Stone Bricks stairs
            (8, 4.0),  // Andesite stairs
        ].into_iter().collect();
        blocks_dict.insert(1, stair_dict);

        let slab_dict = [
            (9, 3.0),   // Stone slab
            (10, 2.0),  // Cobblestone slab
            (11, 8.0),  // Stone Bricks slab
            (12, 4.0),  // Andesite slab
        ].into_iter().collect();
        blocks_dict.insert(2, slab_dict);


        let mut road_points = HashSet::new();

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                road_points.insert(Point2D::new(x, z));
            }
        }

        replace_ground_smooth(
            &road_points,
            &blocks_dict,
            &block_vec,
            &mut rng,
            &mut editor,
            Some(0),
            None, // No permit blocks
            Some(false), // Ignore water
        ).await;

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn get_wall_points() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        
        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::MotionBlockingNoPlants).await.expect("Failed to get heightmap");

        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        generate_districts(seed, &mut editor).await;

         let glass = Block {
            id: BlockID::Glass,
            data: None,
            state: None,
        };
        let bedrock  = Block {
            id: BlockID::Bedrock,
            data: None,
            state: None,
        };
        let black_wool: Block  = Block {
            id: BlockID::BlackWool,
            data: None,
            state: None,
        };
        let lime_wool: Block  = Block {
            id: BlockID::LimeWool,
            data: None,
            state: None,
        };

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                let super_district_id = editor.world().super_district_map[x as usize][z as usize];
                let district_id = editor.world().district_map[x as usize][z as usize];

                let Some(district_id) = district_id else {
                    continue;
                };
                let Some(super_district_id) = super_district_id else {
                    continue;
                };

                let block = get_block_for_district_type(editor.world().super_districts.get(&super_district_id).expect("Failed to get district").data.district_type);
                let height = height_map[x as usize][z as usize] - build_area.origin.y - 1;
                let point = Point3D::new(x, height + 1, z);

                let World {districts,super_districts, .. } = editor.world();
                let super_district = super_districts.get(&super_district_id).expect("Failed to get super district");
                let district = districts.get(&district_id).expect("Failed to get district");

                if super_district.data.edges.contains(&point) {
                    editor.place_block(&bedrock, Point3D::new(x, height, z)).await;
                }
                else if district.data.edges.contains(&point) {
                    editor.place_block(&glass, Point3D::new(x, height, z)).await;
                    editor.place_block(&block, Point3D::new(x, height - 1, z)).await;
                }
                else {
                    editor.place_block(&block, Point3D::new(x, height, z)).await;
                }

            }
        }
        let wall_points = crate::generator::districts::wall::get_wall_points(&editor.world().get_urban_points(), &mut editor);
        for point in wall_points.clone() {
            let height = height_map[point.x as usize][point.y as usize] - build_area.origin.y;
            editor.place_block(&black_wool, Point3D::new(point.x, height, point.y)).await;
        }
        for point in editor.world().get_urban_points().difference(&wall_points) {
            let height = height_map[point.x as usize][point.y as usize] - build_area.origin.y;
            editor.place_block(&lime_wool, Point3D::new(point.x, height, point.y)).await;
        }


    }

    #[tokio::test]
    async fn palisade() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        
        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::MotionBlockingNoPlants).await.expect("Failed to get heightmap");

        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        generate_districts(seed, &mut editor).await;

        let materials = Material::load().expect("Failed to load materials");
        let material = MaterialId::new("oak_planks".to_string());

        let placer: Placer = Placer::new(
            &materials,
        );

        let glass = Block {
            id: BlockID::Glass,
            data: None,
            state: None,
        };
        let bedrock  = Block {
            id: BlockID::Bedrock,
            data: None,
            state: None,
        };

        let structures = Structure::load().expect("Failed to load structures");

        build_wall(&editor.world().get_urban_points(), &mut editor, &mut rng, &placer, &material, &structures, WallType::Palisade).await;

    }

    #[tokio::test]
    async fn standard_wall() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        
        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::MotionBlockingNoPlants).await.expect("Failed to get heightmap");

        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        generate_districts(seed, &mut editor).await;

        let materials = Material::load().expect("Failed to load materials");
        let material = MaterialId::new("stone_bricks".to_string());

        let placer: Placer = Placer::new(
            &materials,
        );

        let structures = Structure::load().expect("Failed to load structures");

        build_wall(&editor.world().get_urban_points(), &mut editor, &mut rng, &placer, &material, &structures, WallType::Standard).await;

    }
}