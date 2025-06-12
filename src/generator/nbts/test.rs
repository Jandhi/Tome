
#[cfg(test)]
mod tests {
    use std::{env, path::Path};

    use log::info;

    use crate::{data::Loadable, editor::World, generator::{materials::{Material, Palette}, nbts::place::place_nbt}, http_mod::GDMCHTTPProvider, util::init_logger};


    #[tokio::test]
    async fn test_place_nbt() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        let materials = Material::load().expect("Failed to load materials");

        let palettes = Palette::load().expect("Failed to load palettes");
        let input_palette = palettes.get("test1").expect("Default palette not found");
        let output_palette = palettes.get("test2").expect("Default palette not found");

        // Assuming you have a valid NBT file path
        let path = env::current_dir().expect("Should get current dir")
            .join("data").join("structures").join("bedroom1.nbt");
        
        let midpoint = editor.world().world_rect_2d().size / 2;
        let point = editor.world().add_height(midpoint);

        // Place the NBT structure in the world
        place_nbt(Path::new(&path), point.into(), &mut editor, &materials, input_palette, output_palette)
            .await
            .expect("Failed to place NBT structure");

        info!("NBT structure placed successfully");

        editor.flush_buffer().await;
    }
    
}