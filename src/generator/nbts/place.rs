use std::{collections::HashMap, path::Path};

use anyhow::Ok;

use crate::generator::materials::{Material, MaterialId};

pub async fn place_nbt(path : &Path, placer : &mut MaterialPlacerWithForm, materials : &HashMap<MaterialId, Material>) -> anyhow::Result<()> {
    let nbt_data = std::fs::read(path)?;
    
    

    Ok(())
}