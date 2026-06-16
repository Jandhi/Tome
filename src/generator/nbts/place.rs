use std::io::Read;

use anyhow::Ok;
use flate2::read::GzDecoder;
use log::info;

use crate::{data::to_snbt, editor::Editor, generator::{data::LoadedData, materials::{Palette, PaletteSwapResult, Placer}, nbts::{Rotation, Structure, meta::NBTMeta, nbt::NBTStructure, transform::Transform}}, geometry::{Cardinal, Point3D}, minecraft::Block};

/// Convert a block's stored NBT compound into the SNBT string the editor
/// sends to the server. Empty compounds carry no real data and are dropped.
fn block_nbt_to_snbt(value : &fastnbt::Value) -> Option<String> {
    match value {
        fastnbt::Value::Compound(map) if map.is_empty() => None,
        other => Some(to_snbt(other)),
    }
}


pub async fn place_structure<'materials>(editor: &Editor, placer: Option<&mut Placer<'materials>>, structure: &Structure, offset: Point3D, direction: Cardinal, data: Option<&LoadedData>, palette: Option<&Palette>, mirror_x: bool, mirror_z: bool) -> anyhow::Result<()> {
    let rotation: Rotation = Rotation::from(structure.facing) - Rotation::from(direction);
    
    let mut transform = match rotation {
        Rotation::None => offset.into(),
        Rotation::Once => Transform::new(offset, Rotation::Once),
        Rotation::Twice => Transform::new(offset, Rotation::Twice),
        Rotation::Thrice => Transform::new(offset, Rotation::Thrice),
    };

    // Shift the transform to account for the structure's origin
    transform.shift(rotation.apply_to_point(-structure.origin));

    let input_palette = match (data, &structure.palette) {
        (Some(data), Some(palette)) => data.palettes.get(palette).cloned(),
        _ => None,
    };

    place_nbt(&structure.meta, transform, editor, placer, data, input_palette.as_ref(), palette, 
        if mirror_x { Some(structure.origin.x) } else { None }, 
        if mirror_z { Some(structure.origin.z) } else { None }
    ).await
}

pub async fn place_nbt<'materials>(data: &NBTMeta, transform: Transform, editor: &Editor, placer: Option<&mut Placer<'materials>>, generator_data: Option<&LoadedData>, input_palette: Option<&Palette>, output_palette: Option<&Palette>, mirror_x: Option<i32>, mirror_z: Option<i32>) -> anyhow::Result<()> {
    info!("Placing NBT structure: {}", data.path);

    let nbt_data = std::fs::read(data.path.clone())?;
    
    let structure : Result<NBTStructure, fastnbt::error::Error> = fastnbt::from_bytes(&nbt_data);

    // Try to decode the structure directly, if it fails, try decompressing and decoding
    let structure = match structure {
        Result::Ok(s) => s,
        Err(_) => {
            let mut decoder = GzDecoder::new(nbt_data.as_slice());
            let mut buf = vec![];
            decoder.read_to_end(&mut buf)?;
            fastnbt::from_bytes(&buf)?
        }
    };

    if input_palette.is_none() || output_palette.is_none() {
        for blockdata in structure.blocks {
            let palette_data = structure.palette.get(blockdata.state).expect("The block state index is out of bounds");
            let data = blockdata.nbt.as_ref().and_then(block_nbt_to_snbt);

            if palette_data.name == "air".into() || palette_data.name.is_structure_void() {
                continue; // Skip air, and structure voids (leave existing terrain).
            }

            let mut pos = Point3D::from(blockdata.pos);

            if let Some(mx) = mirror_x {
                pos.x = mx * 2 - pos.x;
            }
            if let Some(mz) = mirror_z {
                pos.z = mz * 2 - pos.z;
            }
            // If no palettes are specified, place the block directly
            let block = (-transform.rotation).apply_to_block(Block{
                id: palette_data.name.clone(),
                state: palette_data.properties.clone(),
                data, // Now contains the SNBT string if data exists
            });
            editor.place_block(&block, transform.apply(Point3D::from(blockdata.pos))).await;
        }
    } else {
        let placer = placer.unwrap();
        let LoadedData { materials, .. } = generator_data.unwrap();
        for blockdata in structure.blocks {
            let palette_data = structure.palette.get(blockdata.state).expect("The block state index is out of bounds");
            let data = blockdata.nbt.as_ref().and_then(block_nbt_to_snbt);

            if palette_data.name == "air".into() || palette_data.name.is_structure_void() {
                continue; // Skip air, and structure voids (leave existing terrain).
            }

            let mut pos = Point3D::from(blockdata.pos);

            if let Some(mx) = mirror_x {
                pos.x = mx * 2 - pos.x;
            }
            if let Some(mz) = mirror_z {
                pos.z = mz * 2 - pos.z;
            }

            let swap = input_palette.unwrap().swap_with(palette_data.name.clone(), output_palette.unwrap(), materials);

            match swap {
                PaletteSwapResult::Block(id) => {
                    let block = (-transform.rotation).apply_to_block(Block{
                        id,
                        state: palette_data.properties.clone(),
                        data, // Now contains the SNBT string if data exists
                    });

                    editor.place_block(&block, transform.apply(pos)).await;
                },
                PaletteSwapResult::Material(material_id, form) => {
                    let block = (-transform.rotation).apply_to_block(Block{
                        id: Default::default(),
                        state: palette_data.properties.clone(),
                        data, // Now contains the SNBT string if data exists
                    });
                    
                    placer.place_block(
                        editor,
                        transform.apply(pos),
                        material_id,
                        form,
                        block.state.as_ref(),
                        block.data.as_ref()
                    ).await
                }
            }
        }
    }

    Ok(())
}