#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use crate::{editor::{Editor, World}, generator::districts::{district::generate_districts, district_painter::{replace_ground, replace_ground_smooth}}, geometry::{Point2D, Point3D}, http_mod::{GDMCHTTPProvider, HeightMapType}, minecraft::{Block, BlockID}, noise::{Seed, RNG}, util::init_logger};

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
            states: None,
        }
    }

    #[tokio::test]
    async fn district_test() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);

        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::WorldSurface).await.expect("Failed to get heightmap");
        
        let mut editor = Editor::new(build_area);
        let mut world = World::new(&provider).await.expect("Failed to create world");

        let _districts = generate_districts(seed, &mut world).await;

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                let district_id = world.district_map[x as usize][z as usize];

                let Some(district_id) = district_id else {
                    continue;
                };

                let block = get_block_for_id(district_id.0 as usize);
                let height = height_map[x as usize][z as usize];

                editor.place_block(&block, Point3D::new(x, height - build_area.origin.y, z)).await;
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
        
        let mut editor = Editor::new(build_area);
        let mut world = World::new(&provider).await.expect("Failed to create world");

        let _districts = generate_districts(seed, &mut world).await;

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                let district_id = world.super_district_map[x as usize][z as usize];

                let Some(district_id) = district_id else {
                    continue;
                };

                let block = get_block_for_id(district_id.0 as usize);
                let height = height_map[x as usize][z as usize];

                editor.place_block(&block, Point3D::new(x, height - build_area.origin.y, z)).await;
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

        let mut editor = Editor::new(build_area);
        let mut world = World::new(&provider).await.expect("Failed to create world");

        let _districts = generate_districts(seed, &mut world).await;

        let block_vec = vec![
            Block {
                id: BlockID::Stone,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::Cobblestone,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::StoneBricks,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::Andesite,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::Gravel,
                data: None,
                states: None,
            },
        ];

        let mut block_dict: HashMap<u32, f32> = HashMap::new();
        block_dict.insert(0, 3.0); // Stone
        block_dict.insert(1, 2.0); // Cobblestone
        block_dict.insert(2, 8.0); // Stone Bricks
        block_dict.insert(3, 3.0); // Andesite
        block_dict.insert(4, 1.0); // Gravel

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
            &mut world,
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

        let mut editor = Editor::new(build_area);
        let mut world = World::new(&provider).await.expect("Failed to create world");

        let _districts = generate_districts(seed, &mut world).await;

        let block_vec = vec![
            Block {
                id: BlockID::Stone,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::Cobblestone,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::StoneBricks,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::Andesite,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::Gravel,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::StoneStairs,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::CobblestoneStairs,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::StoneBrickStairs,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::AndesiteStairs,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::StoneSlab,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::CobblestoneSlab,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::StoneBrickSlab,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::AndesiteSlab,
                data: None,
                states: None,
            },
        ];

        let mut blocks_dict: HashMap<u32, HashMap<u32, f32>> = HashMap::new();

        let block_dict = [
            (0, 3.0),  // Stone
            (1, 2.0),  // Cobblestone
            (2, 8.0),  // Stone Bricks
            (3, 3.0),  // Andesite
            (4, 1.0),  // Gravel
        ].iter().cloned().collect();
        blocks_dict.insert(0, block_dict);

        let stair_dict = [
            (5, 3.0),  // Stone stairs
            (6, 2.0),  // Cobblestone stairs
            (7, 8.0),  // Stone Bricks stairs
            (8, 4.0),  // Andesite stairs
        ].iter().cloned().collect();
        blocks_dict.insert(1, stair_dict);

        let slab_dict = [
            (9, 3.0),   // Stone slab
            (10, 2.0),  // Cobblestone slab
            (11, 8.0),  // Stone Bricks slab
            (12, 4.0),  // Andesite slab
        ].iter().cloned().collect();
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
            &mut world,
            &mut editor,
            Some(0),
            None, // No permit blocks
            Some(false), // Ignore water
        ).await;

        editor.flush_buffer().await;
    }
}