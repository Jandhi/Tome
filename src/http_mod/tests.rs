
#[cfg(test)]
mod tests {
    use crate::http_mod::{GDMCHTTPProvider, PositionedBlock};
    use crate::minecraft::BlockID;
    use crate::util::init_logger;

    #[tokio::test]
    async fn get_blocks() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area()
            .await
            .expect("Failed to get build area");
        let blocks = provider.get_blocks(build_area.origin.x, build_area.origin.y, build_area.origin.z, build_area.size.x, build_area.size.y, build_area.size.z)
            .await
            .expect("Failed to get blocks");

        assert!(!blocks.is_empty(), "No blocks returned from server");
    }

    #[tokio::test]
    async fn put_blocks() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area()
            .await
            .expect("Failed to get build area");
        let blocks = vec![
            PositionedBlock {
                x: build_area.origin.x.into(),
                y: build_area.origin.y.into(),
                z: build_area.origin.z.into(),
                id: BlockID::Stone,
                data: None,
                state: None,
            },
            PositionedBlock {
                x: (build_area.origin.x + 1).into(),
                y: build_area.origin.y.into(),
                z: build_area.origin.z.into(),
                id: BlockID::Stone,
                data: None,
                state: None,
            },
        ];

        let response = provider.put_blocks(&blocks)
            .await
            .expect("Failed to put blocks");

        assert_eq!(response.len(), 2, "Expected 2 block placement responses");

    }

    #[tokio::test]
    async fn get_biomes() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area()
            .await
            .expect("Failed to get build area");
        let biomes = provider.get_biomes(build_area.origin.x, build_area.origin.y, build_area.origin.z, build_area.size.x, build_area.size.y, build_area.size.z)
            .await
            .expect("Failed to get biomes");

        log::info!("Biomes: {:?}", biomes);
        assert!(!biomes.is_empty(), "No biomes returned from server");
    }

    #[tokio::test]
    async fn get_chunks() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area()
            .await
            .expect("Failed to get build area");
        let chunks = provider.get_chunks(build_area.origin.x, build_area.origin.y, build_area.origin.z, build_area.size.x, build_area.size.y, build_area.size.z)
            .await
            .expect("Failed to get chunks");

        log::info!("a section: {:?}", chunks[0].sections[0]);
    }

    
    #[tokio::test]
    async fn test_give_book() {
        init_logger();
        let provider = GDMCHTTPProvider::new();
        let title = "Test Book";
        let author = "Test Author";
        let pages = vec![
            "This is the first page of the book.",
            "This is the second page of the book.",
            "This is the third page of the book."
        ];

        let book = provider.give_player_book(&pages, title, author)
            .await
            .expect("Failed to give book");

        println!("Book given: {:?}", book);
    }
}