use std::{collections::HashMap, io::Read};

use anyhow::Ok;
use flate2::read::GzDecoder;
use log::info;

use crate::{data::to_snbt, editor::Editor, generator::{data::LoadedData, materials::{Material, MaterialId, Palette, PaletteId, PaletteSwapResult, Placer}, nbts::{meta::NBTMeta, nbt::NBTStructure, transform::Transform, Rotation, Structure}}, geometry::{Cardinal, Point3D}, minecraft::{Block, BlockID}};


pub async fn place_structure<'materials>(editor: &mut Editor, placer : Option<&mut Placer<'materials>>, structure: &Structure, offset : Point3D, direction : Cardinal, data : Option<&LoadedData>, palette: Option<&PaletteId>,  mirror_x : bool, mirror_z : bool) -> anyhow::Result<()> {
    let rotation: Rotation = Rotation::from(structure.facing) - Rotation::from(direction);
    
    let mut transform = match rotation {
        Rotation::None => offset.into(),
        Rotation::Once => Transform::new(offset, Rotation::Once),
        Rotation::Twice => Transform::new(offset, Rotation::Twice),
        Rotation::Thrice => Transform::new(offset, Rotation::Thrice),
    };

    // Shift the transform to account for the structure's origin
    transform.shift(rotation.apply_to_point(-structure.origin));

    let input_palette = structure.palette.as_ref().map(|p| p.clone());

    place_nbt(&structure.meta, transform, editor, placer, data, input_palette.as_ref(), palette, 
        if mirror_x { Some(structure.origin.x) } else { None }, 
        if mirror_z { Some(structure.origin.z) } else { None }
    ).await
}

pub async fn place_nbt<'materials>(data : &NBTMeta, transform : Transform, editor : &mut Editor, placer : Option<&mut Placer<'materials>>,  generator_data : Option<&LoadedData>, input_palette : Option<&PaletteId>, output_palette : Option<&PaletteId>, mirror_x : Option<i32>, mirror_z : Option<i32>) -> anyhow::Result<()> {
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

    if input_palette.is_none() && output_palette.is_none() {
        for blockdata in structure.blocks {
            let palette_data = structure.palette.get(blockdata.state).expect("The block state index is out of bounds");
            let mut data = blockdata.nbt;

            if data.as_ref().is_some_and(|d| d == "\"{}\"") {
                data = None;
            }

            if palette_data.name == BlockID::Air {
                continue; // Skip air blocks
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
        Ok(());
    } else {
        let mut placer = placer.unwrap();
        let LoadedData { materials, palettes, .. } = generator_data.unwrap();
        let palette = palettes.get(input_palette.unwrap()).expect(&format!("Palette {:?} not found", input_palette)).clone();
        for blockdata in structure.blocks {
            let palette_data = structure.palette.get(blockdata.state).expect("The block state index is out of bounds");
            let mut data = blockdata.nbt;

            if data.as_ref().is_some_and(|d| d == "\"{}\"") {
                data = None;
            }

            if palette_data.name == BlockID::Air {
                continue; // Skip air blocks
            }

            let mut pos = Point3D::from(blockdata.pos);

            if let Some(mx) = mirror_x {
                pos.x = mx * 2 - pos.x;
            }
            if let Some(mz) = mirror_z {
                pos.z = mz * 2 - pos.z;
            }
            
            let swap = palette.swap_with(palette_data.name, palettes.get(output_palette.unwrap()).expect(&format!("Palette {:?} not found", output_palette)), materials);

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
                        id: BlockID::Unknown,
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