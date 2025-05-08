#[cfg(test)]
mod tests {
    use crate::{editor::Editor, generator::districts::district::generate_districts, geometry::Point3D, http_mod::GDMCHTTPProvider, minecraft::{Block, BlockID}};

    fn init_logger() {
        simple_logger::SimpleLogger::new()
            .with_level(log::LevelFilter::Info)
            .init()
            .unwrap();
    }

    #[tokio::test]
    async fn district_test() {
        init_logger();

        // Initialize the test data
        let seed = 12345;

        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z).await.expect("Failed to get heightmap");
        
        let mut editor = Editor::new(build_area);

        let districts = generate_districts(seed, build_area, &height_map);

        let blocks = vec![
            Block {
                id: BlockID::RedWool,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::GreenWool,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::BlueWool,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::YellowWool,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::MagentaWool,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::LightBlueWool,
                data: None,
                states: None,
            },
            Block {
                id: BlockID::OrangeWool,
                data: None,
                states: None,
            },
        ];

        for district in districts.iter() {
            let block = blocks[district.id.0 as usize % blocks.len()].clone();

            for point in district.points.iter() {
                editor.place_block(block.clone(), Point3D::new(point.x, point.y, point.z)).await;
            }
        }

        editor.flush_buffer().await;
    }
}