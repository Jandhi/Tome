#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use crate::{editor::{Editor, World}, generator::districts::{district::generate_districts, replace_ground}, geometry::{Point2D, Point3D}, http_mod::{GDMCHTTPProvider, HeightMapType}, minecraft::{Block, BlockID}, noise::{Seed, RNG}, util::init_logger};

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
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::WorldSurface).await.expect("Failed to get heightmap");
        
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
                id: BlockID::Stone_Bricks,
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
                log::info!("Adding road point: ({}, {})", x, z);
                let district_id = world.district_map[x as usize][z as usize];

                if district_id.is_none() {
                    continue;
                }

                let block = &block_vec[(district_id.unwrap().0 % block_vec.len()) as usize];
                let height = height_map[x as usize][z as usize];

                editor.place_block(&block, Point3D::new(x, height - build_area.origin.y, z)).await;
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
        );

        editor.flush_buffer().await;
    }
}