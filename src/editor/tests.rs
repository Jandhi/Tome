#[cfg(test)]
mod tests {
    use log::info;
    use log::LevelFilter;
    use simple_logger::SimpleLogger;
    use crate::editor;
    use crate::geometry::Point2D;
    use crate::geometry::Point3D;
    use crate::http_mod::HeightMapType;

    use crate::http_mod::GDMCHTTPProvider;
    use crate::minecraft::Biome;
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
        
        let mut editor = editor::Editor::new(build_area);
        let mut world = editor::World::new();
        world.init(&provider).await.expect("Failed to initialize world");

        let block = Block {
            id: BlockID::Stone,
            data: None,
            states: None,
        };

        for x in 0..build_area.length() {
            for z in 0..build_area.width() {
                let point = world.add_height(Point2D { x, y: z });
                info!("Placing block at: {:?}", point);
                editor.place_block( &block, point).await;
            }
        }         
    }

    #[tokio::test]
    async fn get_surface_biome_at() {
        init_logger();
        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let mut world = editor::World::new();
        world.init(&provider).await.expect("Failed to initialize world");

        for x in 0..build_area.length() {
            for z in 0..build_area.width() {
                let biome = world.get_surface_biome_at(Point2D::new(x, z));
                assert_ne!(biome, Biome::Unknown, "Biome should not be unknown");
            }
        }
    }
}