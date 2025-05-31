#[cfg(test)]
mod tests {
    use crate::{editor::{Editor, World}, generator::districts::district::generate_districts, geometry::Point3D, http_mod::{GDMCHTTPProvider, HeightMapType}, minecraft::{Block, BlockID}, noise::Seed, util::init_logger};

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
}