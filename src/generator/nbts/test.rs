
#[cfg(test)]
mod tests {
    use std::{cell::RefCell, env, fs::write};

    use log::info;

    use crate::{data::Loadable, editor::World, generator::{buildings::{roofs::{HipRoofPart, RoofComponent, RoofType}, walls::{VerticalWallPosition, WallComponent, WallType}}, data::LoadedData, materials::Placer, nbts::{nbt::NBTStructure, place::place_nbt, place::place_structure, NBTMeta, Structure, StructureType}, style::Style}, geometry::{Cardinal, Point3D}, http_mod::{Coordinate, GDMCHTTPProvider}, minecraft::Block, noise::RNG, util::init_logger};
    use std::fs::File;
    use fastnbt::to_writer;


    /// Authoring aid: for each industrial NBT, propose standable cells next to
    /// ground-floor workstations so anchor coords can be hand-curated into the
    /// JSON sidecars. A "stand" cell is non-solid with a solid floor below and
    /// headroom above. Run: `cargo test propose_industrial_anchors -- --nocapture`.
    #[test]
    fn propose_industrial_anchors() {
        use std::collections::HashSet;
        use std::io::Read;
        use flate2::read::GzDecoder;

        fn load(path: &str) -> NBTStructure {
            let raw = std::fs::read(path).expect("read nbt");
            match fastnbt::from_bytes::<NBTStructure>(&raw) {
                Ok(s) => s,
                Err(_) => {
                    let mut d = GzDecoder::new(raw.as_slice());
                    let mut buf = vec![];
                    d.read_to_end(&mut buf).expect("gunzip");
                    fastnbt::from_bytes(&buf).expect("parse nbt")
                }
            }
        }

        // Per building, the "primary" job-site blocks worth standing at (the
        // outfit follows the building, not the block).
        let primary: &[(&str, &[&str])] = &[
            ("smithy", &["smithing_table", "anvil", "furnace", "grindstone"]),
            ("mill", &["stonecutter", "grindstone"]),
            ("bakery", &["furnace", "smoker", "barrel"]),
            ("carpenter", &["stonecutter", "fletching_table"]),
            ("tannery", &["cauldron", "smithing_table", "anvil", "fletching_table"]),
            ("weaver", &["loom"]),
        ];

        for (name, stations) in primary {
            let s = load(&format!("data/structures/resource_buildings/{name}.nbt"));
            let short = |b: &crate::generator::nbts::nbt::BlockData| {
                let id = s.palette[b.state].name.as_str().to_string();
                id.strip_prefix("minecraft:").unwrap_or(&id).to_string()
            };
            let solid: HashSet<(i32, i32, i32)> = s.blocks.iter()
                .filter(|b| { let n = short(b); n != "air" && n != "cave_air" && n != "void_air" })
                .map(|b| (b.pos[0], b.pos[1], b.pos[2]))
                .collect();
            let standable = |p: (i32, i32, i32)| {
                solid.contains(&(p.0, p.1 - 1, p.2)) // floor below
                    && !solid.contains(&p)            // feet clear
                    && !solid.contains(&(p.0, p.1 + 1, p.2)) // headroom
            };
            println!("\n===== {name} =====");
            let mut used: HashSet<(i32, i32, i32)> = HashSet::new();
            for b in &s.blocks {
                if b.pos[1] != 1 { continue; } // ground floor only
                let n = short(b);
                if !stations.iter().any(|st| n == *st) { continue; }
                let st = (b.pos[0], b.pos[1], b.pos[2]);
                // First cardinally-adjacent standable cell, preferring an unused one.
                let cands = [(1, 0), (-1, 0), (0, 1), (0, -1)];
                if let Some((dx, dz)) = cands.iter().copied()
                    .find(|(dx, dz)| { let c = (st.0 + dx, st.1, st.2 + dz); standable(c) && !used.contains(&c) })
                    .or_else(|| cands.iter().copied().find(|(dx, dz)| standable((st.0 + dx, st.1, st.2 + dz))))
                {
                    let stand = (st.0 + dx, st.1, st.2 + dz);
                    used.insert(stand);
                    println!("  {n}@{:?}  -> stand [{},{},{}] look [{},{},{}]",
                        st, stand.0, stand.1, stand.2, st.0, st.1, st.2);
                }
            }
            if used.is_empty() {
                println!("  (no station-adjacent stand cells; standable ground-floor cells:)");
                for b in &s.blocks {
                    let p = (b.pos[0], b.pos[1], b.pos[2]);
                    if p.1 == 1 && standable(p) {
                        println!("    stand [{},{},{}]", p.0, p.1, p.2);
                    }
                }
            }
        }
    }

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
                Coordinate::AbsoluteF(x) => x.round() as i32,
                Coordinate::Relative(x) => build_area.origin.x + x,
            },
            y: match b.y {
                Coordinate::Absolute(y) => y,
                Coordinate::AbsoluteF(y) => y.round() as i32,
                Coordinate::Relative(y) => build_area.origin.y + y,
            },
            z: match b.z {
                Coordinate::Absolute(z) => z,
                Coordinate::AbsoluteF(z) => z.round() as i32,
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
                staffing: None,
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
                Coordinate::AbsoluteF(x) => x.round() as i32,
                Coordinate::Relative(x) => build_area.origin.x + x,
            },
            y: match b.y {
                Coordinate::Absolute(y) => y,
                Coordinate::AbsoluteF(y) => y.round() as i32,
                Coordinate::Relative(y) => build_area.origin.y + y,
            },
            z: match b.z {
                Coordinate::Absolute(z) => z,
                Coordinate::AbsoluteF(z) => z.round() as i32,
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
                staffing: None,
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