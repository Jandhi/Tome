
#[cfg(test)]
mod tests {
    use std::{cell::RefCell, env, fs::write};

    use log::info;

    use crate::{data::Loadable, editor::World, generator::{buildings::{roofs::{HipRoofPart, RoofComponent, RoofType}, walls::{VerticalWallPosition, WallComponent, WallType}}, data::LoadedData, materials::Placer, nbts::{nbt::NBTStructure, place::place_nbt, place::place_structure, NBTMeta, Structure, StructureType}, style::Style}, geometry::{Cardinal, Point3D}, http_mod::{Coordinate, GDMCHTTPProvider}, minecraft::Block, noise::RNG, util::init_logger};
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

        let data = RefCell::new(data);
        let materials = &data.borrow().materials;
        let data_ref = &data.borrow();
        let input_palette = data_ref.palettes.get(&"test1".into());
        let output_palette = data_ref.palettes.get(&"test2".into());

        // Place the NBT structure in the world
        place_nbt(
            &NBTMeta{ path: path.to_str().expect("Path is not valid unicode").into() }, 
            point.into(), 
            &mut editor, 
            Some(&mut Placer::new(materials, &mut RNG::new(42))), 
            Some(data_ref),
            input_palette,
            output_palette, 
            None, 
            None)
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

        let midpoint = editor.world().world_rect_2d().size / 2;
        let point = editor.world().add_height(midpoint);
        //point.y = point.y - 1; // Adjust height if necessary

        let structures = Structure::load().expect("Failed to load structures");
        let structure = structures.get(&"basic_palisade_gate".into()).expect("Structure not found");

        place_structure(&mut editor, None, &structure, point, Cardinal::North, None, None, false ,false).await.expect("Failed to place structure");

        info!("NBT structure placed successfully");

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn test_save_wall() {
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

        let folder = "data/buildings/walls/components/medieval/bottom";
        let name = "medieval_bottom_arch_supports";
        
        let nbt_structure = NBTStructure::from_blocks(blocks);
        let path = env::current_dir().expect("Should get current dir")
            .join(folder).join(format!("{}.nbt", name));

        let file = File::create(&path).expect("Failed to create NBT file");
        to_writer(file, &nbt_structure).expect("Failed to write NBT structure to file");

        let wall = WallComponent {
            structure: Structure {
                id: name.into(),
                meta: NBTMeta { path: (folder.to_owned() + "/" + name + ".nbt") },
                facing: Cardinal::East,
                origin: Point3D { x: -6, y: 1, z: 0 },
                palette: Some("medieval_spruce".into()),
                tags: None,
                mirror_x: false,
                mirror_z: false,
                style: Some(Style::Medieval),
                weight: 1.0,
                size_xz: (0, 0),
                y_offset: 0,
                allow_steep: false,
            },
            wall_type: Some(WallType::Support),
            vertical_position: Some(VerticalWallPosition::Bottom),
            horizontal_position: None,
        };

        let wall_json = serde_json::to_string_pretty(&wall).expect("Failed to serialize wall to JSON");
        let json_path = env::current_dir().expect("Should get current dir")
            .join(folder).join(format!("{}.json", name));
        write(&json_path, wall_json).expect("Failed to write wall JSON to file");
    }

    #[tokio::test]
    async fn test_save_roof() {
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

        let folder = "data/buildings/roofs/desert";
        let name = "desert_roof_dome_inner";
        
        let nbt_structure = NBTStructure::from_blocks(blocks);
        let path = env::current_dir().expect("Should get current dir")
            .join(folder).join(format!("{}.nbt", name));

        let file = File::create(&path).expect("Failed to create NBT file");
        to_writer(file, &nbt_structure).expect("Failed to write NBT structure to file");

        let roof = RoofComponent {
            structure: Structure {
                id: name.into(),
                meta: NBTMeta { path: (folder.to_owned() + "/" + name + ".nbt") },
                facing: Cardinal::North,
                origin: Point3D { x: 1, y: 1, z: if name.ends_with("side") { 0 } else { 1 } },
                palette: Some("medieval_spruce".into()),
                tags: None,
                mirror_x: false,
                mirror_z: false,
                style: Some(Style::Desert),
                weight: 1.0,
                size_xz: (0, 0),
                y_offset: 0,
                allow_steep: false,
            },
            roof_type: RoofType::Hip(HipRoofPart::Inner),
        };
        

        let roof_json = serde_json::to_string_pretty(&roof).expect("Failed to serialize wall to JSON");
        let json_path = env::current_dir().expect("Should get current dir")
            .join(folder).join(format!("{}.json", name));
        write(&json_path, roof_json).expect("Failed to write wall JSON to file");
    }

    /// One-shot maintenance: reads each NBT under `data/structures/resource_buildings/`,
    /// computes its bounding box and subgrade depth, and patches `size_xz` /
    /// `y_offset` into the sidecar JSON. Run with `cargo test
    /// migrate_resource_building_metadata -- --ignored --nocapture` whenever a
    /// resource-building NBT is added or replaced.
    #[test]
    #[ignore]
    fn migrate_resource_building_metadata() {
        let structures = Structure::load().expect("Failed to load structures");
        let dir = std::path::Path::new("data/structures/resource_buildings");
        for entry in std::fs::read_dir(dir).expect("Failed to read resource_buildings dir") {
            let path = entry.expect("Failed to read entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("Filename was not valid unicode")
                .to_string();
            let key = StructureType(id.clone());
            let structure = structures
                .get(&key)
                .unwrap_or_else(|| panic!("No loaded structure with id '{}'", id));

            let raw = std::fs::read_to_string(&path).expect("Failed to read JSON");
            let mut value: serde_json::Value =
                serde_json::from_str(&raw).expect("Failed to parse JSON");
            let obj = value.as_object_mut().expect("Expected top-level JSON object");
            obj.insert(
                "size_xz".to_string(),
                serde_json::json!([structure.size_xz.0, structure.size_xz.1]),
            );
            obj.insert(
                "y_offset".to_string(),
                serde_json::json!(structure.y_offset),
            );

            let updated = serde_json::to_string_pretty(&value)
                .expect("Failed to serialise updated JSON");
            std::fs::write(&path, updated + "\n").expect("Failed to write JSON");
            println!(
                "Updated {}: size_xz=({}, {}), y_offset={}",
                id, structure.size_xz.0, structure.size_xz.1, structure.y_offset
            );
        }
    }
}