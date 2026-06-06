#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use crate::{data::Loadable, editor::World, generator::districts::{WallType, build_wall, district::{self, generate_districts}, district_painter::{replace_ground, replace_ground_smooth}}, geometry::{Point2D, Point3D}, http_mod::{GDMCHTTPProvider, HeightMapType}, minecraft::Block, noise::{RNG, Seed}, util::init_logger};
    use crate::generator::materials::{Placer, Material, MaterialId};
    use crate::generator::nbts::Structure;

    fn get_block_for_id(id : usize) -> Block {
        // List of all 16 wool colors in order
        let wool_colors = [
            "white_wool", "orange_wool", "magenta_wool", "light_blue_wool",
            "yellow_wool", "lime_wool", "pink_wool", "gray_wool",
            "light_gray_wool", "cyan_wool", "purple_wool", "blue_wool",
            "brown_wool", "green_wool", "red_wool", "black_wool",
        ];
        Block {
            id: wool_colors[id % wool_colors.len()].into(),
            data: None,
            state: None,
        }
    }

    fn get_block_for_district_type(district_type: district::DistrictType) -> Block {
        match district_type {
            district::DistrictType::Urban => Block { id: "blue_wool".into(), data: None, state: None },
            district::DistrictType::Rural => Block { id: "green_wool".into(), data: None, state: None },
            district::DistrictType::OffLimits => Block { id: "red_wool".into(), data: None, state: None },
            _ => Block { id: "bedrock".into(), data: None, state: None }, // Default case for unknown types
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
            id: "glass".into(),
            data: None,
            state: None,
        };

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                let district_id = editor.world_mut().district_map[x as usize][z as usize];

                let Some(district_id) = district_id else {
                    continue;
                };
                

                let block = get_block_for_id(district_id.0 as usize);
                let height = height_map[x as usize][z as usize] - build_area.origin.y;
                let point = Point3D::new(x, height, z);
                //editor.place_block(&block, Point3D::new(x, height - build_area.origin.y, z)).await;
                if let Some(district) = editor.world_mut().districts.get(&district_id) {
                    
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
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::MotionBlockingNoPlants).await.expect("Failed to get heightmap");
        
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _districts = generate_districts(seed, &mut editor).await;

        let glass = Block {
            id: "glass".into(),
            data: None,
            state: None,
        };
        let bedrock = Block {
            id: "bedrock".into(),
            data: None,
            state: None,
        };

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                let super_district_id = editor.world_mut().super_district_map[x as usize][z as usize];
                let district_id = editor.world_mut().district_map[x as usize][z as usize];

                let Some(district_id) = district_id else {
                    continue;
                };
                let Some(super_district_id) = super_district_id else {
                    continue;
                };

                let block = get_block_for_id(super_district_id.0 as usize);
                let height = height_map[x as usize][z as usize] - build_area.origin.y;
                let point = Point3D::new(x, height, z);

                let World {districts,super_districts, .. } = editor.world_mut();

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
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::MotionBlockingNoPlants).await.expect("Failed to get heightmap");
        
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _districts = generate_districts(seed, &mut editor).await;
        let glass = Block {
            id: "glass".into(),
            data: None,
            state: None,
        };
        let bedrock  = Block {
            id: "bedrock".into(),
            data: None,
            state: None,
        };

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                let super_district_id = editor.world_mut().super_district_map[x as usize][z as usize];
                let district_id = editor.world_mut().district_map[x as usize][z as usize];

                let Some(district_id) = district_id else {
                    continue;
                };
                let Some(super_district_id) = super_district_id else {
                    continue;
                };

                let block = get_block_for_district_type(editor.world_mut().super_districts.get(&super_district_id).expect("Failed to get district").data.district_type);
                let height = height_map[x as usize][z as usize] - build_area.origin.y;
                let point = Point3D::new(x, height, z);

                let World {districts,super_districts, .. } = editor.world_mut();
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
        
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _districts = generate_districts(seed, &mut editor).await;
        let glass = Block {
            id: "glass".into(),
            data: None,
            state: None,
        };

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                let district_id = editor.world_mut().district_map[x as usize][z as usize];

                let Some(district_id) = district_id else {
                    continue;
                };
                

                let block = get_block_for_district_type(editor.world_mut().districts.get(&district_id).expect("Failed to get district").data.district_type);
                let height = height_map[x as usize][z as usize] - build_area.origin.y;
                let point = Point3D::new(x, height, z);
                //editor.place_block(&block, Point3D::new(x, height - build_area.origin.y, z)).await;
                if let Some(district) = editor.world_mut().districts.get(&district_id) {
                    
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
    async fn district_classification_district_points() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);

        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        println!("Build area: {:?}", build_area);
        
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _districts = generate_districts(seed, &mut editor).await;
        let glass = Block {
            id: "glass".into(),
            data: None,
            state: None,
        };

        // Collect district ids and their points to avoid multiple mutable borrows
        let district_points: Vec<_> = {
            let world = editor.world_mut();
            world.districts.iter().map(|(district_id, district)| {
                (*district_id, district.data.district_type, district.data.points.clone(), district.data.edges.clone())
            }).collect()
        };

        for (_district_id, district_type, points, edges) in district_points {
            let block = get_block_for_district_type(district_type);
            for point in points.iter() {
                if edges.contains(point) {
                    editor.place_block(&glass, *point).await;
                    editor.place_block(&block, Point3D::new(point.x, point.y - 1, point.z)).await;
                } else {
                    editor.place_block(&block, *point).await;
                }
            }
        }

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn district_resource_production_report() {
        init_logger();

        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        generate_districts(seed, &mut editor).await;

        let registry = crate::generator::resource_chain::ResourceRegistry::load()
            .expect("Failed to load resource registry");

        // Only Rural super-districts produce raw resources.
        let rural_analysis: HashMap<_, _> = editor.world().super_district_analysis_data.iter()
            .filter(|(id, _)| {
                editor.world().super_districts.get(id)
                    .map(|sd| sd.data.district_type == crate::generator::districts::DistrictType::Rural)
                    .unwrap_or(false)
            })
            .map(|(id, analysis)| (*id, analysis.clone()))
            .collect();

        let result = registry.resolve_for_districts(&rural_analysis, &mut rng);

        // Sort producing super-district IDs for display.
        let mut producing_ids: Vec<_> = result.district_assignments.keys().cloned().collect();
        producing_ids.sort_by_key(|id| id.0);

        println!("\n╔══ District Resource Production Report ════════════╗");

        println!("║ Producing Super-Districts ({} rural of {} total):", producing_ids.len(), editor.world().super_district_analysis_data.len());
        for id in &producing_ids {
            let analysis = &editor.world().super_district_analysis_data[id];
            let biome_names = {
                let mut names: Vec<&str> = analysis.major_biomes().iter()
                    .map(|b| b.as_str().strip_prefix("minecraft:").unwrap_or(b.as_str()))
                    .collect();
                names.sort();
                names.join("+")
            };
            let a = &result.district_assignments[id];
            println!("║   Super-District {:>3} ({:<25}) → {} x2 [{}]",
                id.0, biome_names, a.primary_resource, a.building);
        }

        println!("║");
        println!("║ Resource Supply:");
        let mut supply_sorted: Vec<(&String, &u32)> = result.supply.iter().collect();
        supply_sorted.sort_by_key(|(r, _)| r.as_str());
        for (resource, qty) in supply_sorted {
            println!("║   {:<20} x{}", resource, qty);
        }

        println!("║");
        println!("║ Goods Produced:");
        if result.finished_goods.is_empty() && result.leftover_goods.is_empty() {
            println!("║   (none)");
        }
        for (good, qty) in &result.finished_goods {
            println!("║   {:<20} x{}", good, qty);
        }
        for (good, qty) in &result.leftover_goods {
            println!("║   {:<20} x{}  (unused)", good, qty);
        }

        println!("║");
        println!("║ Gathering Buildings:");
        let mut gb_sorted: Vec<(&String, &u32)> = result.gather_buildings.iter().collect();
        gb_sorted.sort_by_key(|(b, _)| b.as_str());
        for (building, count) in gb_sorted {
            println!("║   {:<20} x{}", building, count);
        }

        println!("║");
        println!("║ Processing Buildings Required:");
        if result.processing_buildings.is_empty() {
            println!("║   (none)");
        }
        let mut pb_sorted: Vec<(&String, &u32)> = result.processing_buildings.iter().collect();
        pb_sorted.sort_by(|(a, ac), (b, bc)| bc.cmp(ac).then(a.cmp(b)));
        for (building, count) in pb_sorted {
            println!("║   {:<20} x{}", building, count);
        }

        println!("╚═══════════════════════════════════════════════════╝\n");
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

        let block_vec : Vec<Block> = vec![
            "stone".into(), "cobblestone".into(), "stone_bricks".into(), "andesite".into(), "gravel".into(),
        ];

        let block_dict: HashMap<usize, f32> = [
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

        let block_vec : Vec<Block> = vec![
            "stone".into(), "cobblestone".into(), "stone_bricks".into(), "andesite".into(), "gravel".into(),
            "stone_stairs".into(), "cobblestone_stairs".into(), "stone_brick_stairs".into(), "andesite_stairs".into(),
            "stone_slab".into(), "cobblestone_slab".into(), "stone_brick_slab".into(), "andesite_slab".into(),
        ];

        let mut blocks_dict: HashMap<usize, HashMap<usize, f32>> = HashMap::new();

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

        
        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::MotionBlockingNoPlants).await.expect("Failed to get heightmap");

        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        generate_districts(seed, &mut editor).await;

         let glass = Block {
            id: "glass".into(),
            data: None,
            state: None,
        };
        let bedrock  = Block {
            id: "bedrock".into(),
            data: None,
            state: None,
        };
        let black_wool: Block  = Block {
            id: "black_wool".into(),
            data: None,
            state: None,
        };
        let lime_wool: Block  = Block {
            id: "lime_wool".into(),
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
        editor.flush_buffer().await;

    }

    #[tokio::test]
    async fn palisade() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);
        let mut rng = RNG::new(seed);
        let mut rng2 = RNG::new(seed);
        
        let provider = GDMCHTTPProvider::new();
        
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        generate_districts(seed, &mut editor).await;

        let materials = Material::load().expect("Failed to load materials");
        let material = MaterialId::new("oak_planks".to_string());

        let mut placer: Placer = Placer::new(
            &materials,
            &mut rng,
        );

        let structures = Structure::load().expect("Failed to load structures");

        build_wall(&editor.world().get_urban_points(), &mut editor, &mut rng2, &mut placer, &material, &structures, WallType::Palisade).await;

    }

    #[tokio::test]
    async fn standard_wall() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);
        let mut rng = RNG::new(seed);
        let mut rng2 = RNG::new(seed);
        
        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        generate_districts(seed, &mut editor).await;

        let materials = Material::load().expect("Failed to load materials");
        let material = MaterialId::new("stone_bricks".to_string());

        let mut placer: Placer = Placer::new(
            &materials,
            &mut rng,
        );

        let structures = Structure::load().expect("Failed to load structures");

        build_wall(&editor.world().get_urban_points(), &mut editor, &mut rng2, &mut placer, &material, &structures, WallType::Standard).await;

    }

    #[tokio::test]
    async fn standard_wall_with_inner() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);
        let mut rng = RNG::new(seed);
        let mut rng2 = RNG::new(seed);
        
        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        generate_districts(seed, &mut editor).await;

        let materials = Material::load().expect("Failed to load materials");
        let material = MaterialId::new("stone_bricks".to_string());

        let mut placer: Placer = Placer::new(
            &materials,
            &mut rng,
        );

        let structures = Structure::load().expect("Failed to load structures");
        println!("Structures: {:?}", structures.keys());

        build_wall(&editor.world().get_urban_points(), &mut editor, &mut rng2, &mut placer, &material, &structures, WallType::StandardWithInner).await;

    }
}