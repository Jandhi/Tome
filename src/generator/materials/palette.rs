use std::collections::HashMap;

use anyhow::Ok;
use serde::Deserialize;
use serde_derive::Serialize;
use strum::IntoEnumIterator;

use crate::{data::Loadable, generator::materials::{role::MaterialRole, Material, MaterialId}, minecraft::{recolor_block, BlockForm, BlockID, Color}};

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

    pub primary_stone : MaterialId,
    pub primary_wood : MaterialId,
    
    #[serde(flatten)]
    pub materials : HashMap<MaterialRole, MaterialId>,

    pub primary_color : Color,
    pub secondary_color : Color,

    pub tags : Option<Vec<String>>,
}

pub enum PaletteSwapResult<'a> {
    Block(BlockID),
    Material(&'a MaterialId, BlockForm),
}

impl Palette {
    pub fn get_material<'a>(&'a self, mut role : MaterialRole) -> &'a MaterialId {
        let mut iterations = 0;

        match role {
            MaterialRole::PrimaryStone => &self.primary_stone,
            MaterialRole::PrimaryWood => &self.primary_wood,
            _ => {
                while !self.materials.contains_key(&role) {
                    // If the role is not found, we can use the backup role
                    // This is useful for roles that might not be defined in the palette
                    role = role.backup_role();

                    if role == MaterialRole::PrimaryStone {
                        return &self.primary_stone;
                    }
                    
                    if role == MaterialRole::PrimaryWood {
                        return &self.primary_wood;
                    }

                    if iterations > 10 {
                        panic!("Infinite loop detected while trying to find material role {:?} in palette {:?}", role, self.id);
                    }

                    iterations += 1;
                }

                self.materials.get(&role).expect(&format!("Material role {:?} not found in palette {:?}", role, self.id))
            }
        }
    }

    pub fn get_block<'a>(&'a self, role : MaterialRole, form : &BlockForm, materials : &'a HashMap<MaterialId, Material>) -> Option<&'a BlockID> {
        materials.get(self.get_material(role)).and_then(|material| material.get_block(form))
    }

    pub fn find_role_and_form(&self, block : BlockID, materials : &HashMap<MaterialId, Material>) -> Option<(MaterialRole, BlockForm)> {
        // Iterate through all material roles to find the matching block
        for role in MaterialRole::iter() {
            let id = self.get_material(role);
            let material = materials.get(id).expect(&format!("Material {:?} not found", id)); 
            
            if let Some(form) = material.get_form(block) {
                return Some((role, form));
            }
        }
        
        None
    }

    pub fn swap_with<'palette>(&'palette self, block : BlockID, output_palette : &'palette Palette, materials : &'palette HashMap<MaterialId, Material>) -> PaletteSwapResult<'palette> {
        if let Some((role, form)) = self.find_role_and_form(block, &materials) {
            return PaletteSwapResult::Material(
                output_palette.get_material(role),
                form,
            )
        }

        PaletteSwapResult::Block(self.recolor_block(block, output_palette))
    }

    pub fn recolor_block(&self, block : BlockID, output_palette : &Palette) -> BlockID {
        recolor_block(recolor_block(block, self.primary_color, output_palette.primary_color), self.secondary_color, output_palette.secondary_color)
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