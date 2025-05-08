#[cfg(test)]
mod tests {
    use log::LevelFilter;
    use simple_logger::SimpleLogger;
    use crate::editor;
    use crate::geometry::Point3D;
    use crate::http_mod::Coordinate;

    use crate::http_mod::{GDMCHTTPProvider, PositionedBlock};
    use crate::minecraft::{Block, BlockID};

    fn init_logger() {
        SimpleLogger::new()
            .with_level(LevelFilter::Info)
            .init()
            .unwrap();
    }

    #[tokio::test]
    async fn place_blocks() {
        init_logger();
        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z).await.expect("Failed to get heightmap");
        
        let mut editor = editor::Editor::new(build_area);

        let block = Block {
            id: BlockID::Stone,
            data: None,
            states: None,
        };

        for x in 0..build_area.length() {
            for z in 0..build_area.width() {
                editor.place_block( block.clone(), Point3D::new(x, height_map[x as usize][z as usize], z)).await;
            }
        }         
    }
}