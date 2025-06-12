use std::collections::HashMap;

use anyhow::Ok;
use serde::Deserialize;
use serde_derive::Serialize;
use strum::IntoEnumIterator;

use crate::{data::Loadable, generator::materials::{role::MaterialRole, Material, MaterialId}, minecraft::{recolor_block, BlockForm, BlockID, Color}};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Palette {
    pub name : String,
    pub primary_stone : MaterialId,
    pub secondary_stone : MaterialId,
    pub primary_wood : MaterialId,
    pub secondary_wood : MaterialId,
    pub accent : MaterialId,

    pub primary_color : Color,
    pub secondary_color : Color,
}

impl Palette {
    pub fn get_material<'a>(&'a self, role : MaterialRole) -> &'a MaterialId {
        match role {
            MaterialRole::PrimaryStone => &self.primary_stone,
            MaterialRole::SecondaryStone => &self.secondary_stone,
            MaterialRole::PrimaryWood => &self.primary_wood,
            MaterialRole::SecondaryWood => &self.secondary_wood,
            MaterialRole::Accent => &self.accent,
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

    pub fn swap_with(&self, block : BlockID, output_palette : &Palette, materials : &HashMap<MaterialId, Material>) -> BlockID {
        if let Some((role, form)) = self.find_role_and_form(block, &materials) {
            if let Some(block_id) = output_palette.get_block(role, &form, materials) {
                return self.recolor_block(*block_id, output_palette);
            }
        }

        self.recolor_block(block, output_palette)
    }

    pub fn recolor_block(&self, block : BlockID, output_palette : &Palette) -> BlockID {
        recolor_block(recolor_block(block, self.primary_color, output_palette.primary_color), self.secondary_color, output_palette.secondary_color)
    }
}

impl Loadable<'_, Palette, String>  for Palette {
    fn get_key(item: &Palette) -> String {
        item.name.clone()
    }

    fn post_load(_items : &mut HashMap<String, Palette>) -> anyhow::Result<()> {
        Ok(())
    }

    fn path() -> &'static str {
        "palettes"
    }
}