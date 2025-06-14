use std::collections::HashMap;

use crate::{data::Loadable, generator::{buildings::walls::Wall, materials::{Material, MaterialId, Palette, PaletteId}, nbts::{Structure, StructureId}}};

pub struct LoadedData {
    pub palettes : HashMap<PaletteId, Palette>,
    pub materials : HashMap<MaterialId, Material>,
    pub structures : HashMap<StructureId, Structure>,
    pub walls : HashMap<StructureId, Wall>,
}

impl LoadedData {
    pub fn load() -> anyhow::Result<Self> {
        Ok(Self {
            palettes: Palette::load()?,
            materials: Material::load()?,
            structures: Structure::load()?,
            walls: Wall::load()?,
        })
    }
}