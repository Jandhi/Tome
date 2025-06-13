use std::{collections::HashMap, io::Read, path::Path};

use anyhow::Ok;
use flate2::read::GzDecoder;
use log::info;

use crate::{data::to_snbt, editor::Editor, generator::{materials::{Material, MaterialId, Palette}, nbts::{meta::NBTMeta, nbt::NBTStructure, transform::Transform, Structure}}, geometry::Point3D, minecraft::Block};

pub async fn place_nbt(data : &NBTMeta, transform : Transform, editor : &mut Editor,  materials : &HashMap<MaterialId, Material>, input_palette : &Palette, output_palette : &Palette) -> anyhow::Result<()> {
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

    for blockdata in structure.blocks {
        let palette_data = structure.palette.get(blockdata.state).expect("The block state index is out of bounds");
        let data = blockdata.nbt.map(|nbt| to_snbt(&nbt));

        let id = input_palette.swap_with(palette_data.name, &output_palette, materials);
        let block = Block{
            id,
            state: palette_data.properties.clone(),
            data, // Now contains the SNBT string if data exists
        };

        editor.place_block(&(-transform.rotation).apply_to_block(block), transform.apply(Point3D::from(blockdata.pos))).await;
    }

    Ok(())
}

pub async fn place_nbt_without_palette(path : &Path, transform : Transform, editor : &mut Editor) -> anyhow::Result<()> {
    let nbt_data = std::fs::read(path)?;
    
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

    for block in structure.blocks {
        let palette_data = structure.palette.get(block.state).expect("The block state index is out of bounds");
        let data = block.nbt.map(|nbt| to_snbt(&nbt));

        let id = palette_data.name;

        editor.place_block(&Block{
            id,
            state: palette_data.properties.clone(),
            data, // Now contains the SNBT string if data exists
        }, transform.apply(Point3D::from(block.pos))).await;
    }

    Ok(())
}