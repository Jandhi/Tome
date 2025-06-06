
#[cfg(test)]
mod tests {
    use std::{env, path::Path};

    use log::info;

    use crate::{data::Loadable, editor::World, generator::{materials::Material, nbts::place::place_nbt}, geometry::Point2D, http_mod::GDMCHTTPProvider, util::init_logger};


    #[tokio::test]
    async fn test_place_nbt() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        let materials = Material::load().expect("Failed to load materials");

        // Assuming you have a valid NBT file path
        let path = env::current_dir().expect("Should get current dir")
            .join("data").join("structures").join("well.nbt");
        
        let midpoint = editor.world().world_rect_2d().size / 2;
        let point = editor.world().add_height(midpoint);

        // Place the NBT structure in the world
        place_nbt(Path::new(&path), point, &mut editor, &materials)
            .await
            .expect("Failed to place NBT structure");

        info!("NBT structure placed successfully");

        editor.flush_buffer().await;
    }
    
}