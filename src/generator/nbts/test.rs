
#[cfg(test)]
mod tests {
    use std::{env, path::Path};

    use log::info;

    use crate::{data::Loadable, editor::World, generator::{data::LoadedData, materials::{Material, Palette, Placer}, nbts::{meta::NBTMeta, place::place_nbt, place_structure, Structure}}, http_mod::GDMCHTTPProvider, util::init_logger};
    use crate::geometry::Cardinal;


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
        place_nbt(&NBTMeta{ path: path.to_str().expect("Path is not valid unicode").into() }, point.into(), &mut editor, Some(&Placer::new(&data.materials)), Some(&data), Some(&"test1".into()), Some(&"test2".into()), None, None)
            .await
            .expect("Failed to place NBT structure");

        info!("NBT structure placed successfully");

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn test_place_structure_without_palette() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();

        // Assuming you have a valid NBT file path
        let path = env::current_dir().expect("Should get current dir")
            .join("data").join("structures").join("city_wall").join("basic_palisade_gate.nbt");

        let mut midpoint = editor.world().world_rect_2d().size / 2;
        let mut point = editor.world().add_height(midpoint);
        //point.y = point.y - 1; // Adjust height if necessary

        let structures = Structure::load().expect("Failed to load structures");
        let structure = structures.get(&"basic_palisade_gate".into()).expect("Structure not found");

        place_structure(&mut editor, None, &structure, point, Cardinal::North, None, None, false ,false).await.expect("Failed to place structure");

        info!("NBT structure placed successfully");

        editor.flush_buffer().await;
    }
    
}