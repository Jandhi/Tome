
#[cfg(test)]
mod tests {
    use std::{env, path::Path};

    use log::info;

    use crate::{data::Loadable, editor::World, generator::{data::LoadedData, materials::{Material, Palette, Placer}, nbts::{nbt::NBTStructure, place::place_nbt, place_nbt_without_palette, NBTMeta}}, geometry::{Point3D, Rect3D}, http_mod::{Coordinate, GDMCHTTPProvider}, minecraft::{Block, BlockID}, util::init_logger};
    use std::fs::File;
    use fastnbt::to_writer;


    #[tokio::test]
    async fn test_place_nbt() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        let data = LoadedData::load().expect("Failed to load generator data");

        // Assuming you have a valid NBT file path
        let path = env::current_dir().expect("Should get current dir")
            .join("data").join("structures").join("test_save.nbt");
        
        let midpoint = editor.world_mut().world_rect_2d().size / 2;
        let point = editor.world_mut().add_height(midpoint);

        // Place the NBT structure in the world
        place_nbt(&NBTMeta{ path: path.to_str().expect("Path is not valid unicode").into() }, point.into(), &mut editor, &Placer::new(&data.materials), &data, &"test1".into(), &"test2".into(), None, None)
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

    #[tokio::test]
    async fn test_save_nbt() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area().await.expect("Failed to get build area");

        let blocks = provider.get_blocks(
            build_area.origin.x, 
            build_area.origin.y, 
            build_area.origin.z, 
            build_area.size.x, 
            build_area.size.y, 
            build_area.size.z
        ).await.expect("Failed to get blocks").iter().map(|b| (Block{
            id: b.id.clone(),
            state: b.state.clone(),
            data: b.data.clone(),
        }, Point3D{
            x: match b.x {
                Coordinate::Absolute(x) => x,
                Coordinate::Relative(x) => build_area.origin.x + x,
            },
            y: match b.y {
                Coordinate::Absolute(y) => y,
                Coordinate::Relative(y) => build_area.origin.y + y,
            },
            z: match b.z {
                Coordinate::Absolute(z) => z,
                Coordinate::Relative(z) => build_area.origin.z + z,
            },
        } - build_area.origin)).collect::<Vec<_>>();
        
        let nbt_structure = NBTStructure::from_blocks(blocks);
        let path = env::current_dir().expect("Should get current dir")
            .join("data").join("structures").join("test_save.nbt");

        let file = File::create(&path).expect("Failed to create NBT file");
        to_writer(file, &nbt_structure).expect("Failed to write NBT structure to file");
    }


    
}