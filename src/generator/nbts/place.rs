use std::{collections::HashMap, io::Read, path::Path};

use anyhow::Ok;
use flate2::read::GzDecoder;
use log::info;

use crate::{data::to_snbt, editor::Editor, generator::{materials::{Material, MaterialId}, nbts::{structure::Structure, transform::{self, Transform}}}, geometry::Point3D, minecraft::Block};

pub async fn place_nbt(path : &Path, transform : Transform, editor : &mut Editor,  materials : &HashMap<MaterialId, Material>) -> anyhow::Result<()> {
    let nbt_data = std::fs::read(path)?;
    
    let structure : Result<Structure, fastnbt::error::Error> = fastnbt::from_bytes(&nbt_data);

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

        editor.place_block(&Block{
            id: palette_data.name,
            state: palette_data.properties.clone(),
            data, // Now contains the SNBT string if data exists
        }, transform.apply(Point3D::from(block.pos))).await;
    }

    Ok(())
}