use std::collections::HashMap;

use anyhow::Ok;
use serde::Deserialize;
use serde_derive::Serialize;
use strum::IntoEnumIterator;

use crate::{data::Loadable, generator::materials::{role::MaterialRole, Material, MaterialId}, minecraft::{recolor_block, BlockForm, BlockID, Color}, noise::RNG};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaletteId(String);

impl From<String> for PaletteId {
    fn from(id: String) -> Self {
        PaletteId(id)
    }
}

impl From<&str> for PaletteId {
    fn from(id: &str) -> Self {
        PaletteId(id.to_string())
    }
}

impl Into<String> for PaletteId {
    fn into(self) -> String {
        self.0
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Palette {
    pub id : PaletteId,
    
    #[serde(flatten)]
    pub materials : HashMap<MaterialRole, MaterialId>,

    pub primary_color : Option<Color>,
    pub secondary_color : Option<Color>,

    pub tags : Option<Vec<String>>,
}

pub enum PaletteSwapResult<'a> {
    Block(BlockID),
    Material(&'a MaterialId, BlockForm),
}

impl Palette {
    pub fn get_material<'a>(&'a self, mut role : MaterialRole) -> Option<&'a MaterialId> {
        let mut iterations = 0;

        while !self.materials.contains_key(&role) {
            // If the role is not found, we can use the backup role
            // This is useful for roles that might not be defined in the palette
            let new_role = role.backup_role();

            if new_role == role || iterations >= 5 {
                return None;
            }

            role = new_role;

            iterations += 1;
        }

        Some(self.materials.get(&role).expect(&format!("Material role {:?} not found in palette {:?}", role, self.id)))
    }

    pub fn get_block<'a>(&'a self, role : MaterialRole, form : &BlockForm, materials : &'a HashMap<MaterialId, Material>, rng : &mut RNG) -> Option<&'a BlockID> {
        materials.get(self.get_material(role)?).and_then(|material| material.get_block(form, rng))
    }

    pub fn find_role_and_form(&self, block : BlockID, materials : &HashMap<MaterialId, Material>) -> Option<(MaterialRole, BlockForm)> {
        // Iterate through all material roles to find the matching block
        for role in [
            MaterialRole::Flower,
            MaterialRole::Accent,
            MaterialRole::PrimaryWall,
            MaterialRole::SecondaryWall,
            MaterialRole::PrimaryRoof,
            MaterialRole::SecondaryRoof,
            MaterialRole::WoodPillar,
            MaterialRole::StonePillar,
            MaterialRole::SecondaryStone,
            MaterialRole::SecondaryWood,
            MaterialRole::PrimaryStone,
            MaterialRole::PrimaryWood,
        ] {
            let id = self.get_material(role);

            if id.is_none() {
                continue; // Skip if the material role is not defined in the palette
            }

            let material = materials.get(id?).expect(&format!("Material {:?} not found", id)); 
            
            if let Some(form) = material.get_form(block) {
                return Some((role, form));
            }
        }
        
        None
    }

    pub fn swap_with<'palette>(&'palette self, block : BlockID, output_palette : &'palette Palette, materials : &'palette HashMap<MaterialId, Material>) -> PaletteSwapResult<'palette> {
        if let Some((role, form)) = self.find_role_and_form(block, &materials) {
            if let Some(material_id) = output_palette.get_material(role) {
                return PaletteSwapResult::Material(material_id, form);
            }
        }

        PaletteSwapResult::Block(self.recolor_block(block, output_palette))
    }

    pub fn recolor_block(&self, block : BlockID, output_palette : &Palette) -> BlockID {
        let mut recolored = block;
        if let (Some(src), Some(dst)) = (self.primary_color, output_palette.primary_color) {
            recolored = recolor_block(recolored, src, dst);
        }
        if let (Some(src), Some(dst)) = (self.secondary_color, output_palette.secondary_color) {
            recolored = recolor_block(recolored, src, dst);
        }
        recolored
    }
}

impl Loadable<'_, Palette, PaletteId>  for Palette {
    fn get_key(item: &Palette) -> PaletteId {
        item.id.clone()
    }

    fn post_load(_items : &mut HashMap<PaletteId, Palette>) -> anyhow::Result<()> {
        Ok(())
    }

    fn path() -> &'static str {
        "palettes"
    }
}