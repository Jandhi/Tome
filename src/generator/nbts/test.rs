
#[cfg(test)]
mod tests {
    use std::{env, path::Path};

    use log::info;

    use crate::{data::Loadable, editor::World, generator::{materials::{Material, Palette}, nbts::{place::place_nbt, place_nbt_without_palette, NBTMeta}}, http_mod::GDMCHTTPProvider, util::init_logger};


    #[tokio::test]
    async fn test_place_nbt() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        let data = LoadedData::load().expect("Failed to load generator data");

        // Assuming you have a valid NBT file path
        let path = env::current_dir().expect("Should get current dir")
            .join("data").join("structures").join("bedroom1.nbt");
        
        let midpoint = editor.world().world_rect_2d().size / 2;
        let point = editor.world().add_height(midpoint);

        // Place the NBT structure in the world
        place_nbt(&NBTMeta{ path: path.to_str().expect("Path is not valid unicode").into() }, point.into(), &mut editor, &Placer::new(&data.materials), &data, &"test1".into(), &"test2".into())
            .await
            .expect("Failed to place NBT structure");

        info!("NBT structure placed successfully");

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn test_place_nbt_without_palette() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();

        // Assuming you have a valid NBT file path
        let path = env::current_dir().expect("Should get current dir")
            .join("data").join("structures").join("city_wall").join("basic_palisade_gate.nbt");

        let mut midpoint = editor.world().world_rect_2d().size / 2;
        let mut point = editor.world().add_height(midpoint);
        point.y = point.y - 1; // Adjust height if necessary

        // Place the NBT structure in the world
        place_nbt_without_palette(Path::new(&path), point.into(), &mut editor)
            .await
            .expect("Failed to place NBT structure");

        info!("NBT structure placed successfully");

        editor.flush_buffer().await;
    }
    
}