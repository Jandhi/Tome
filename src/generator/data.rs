use std::collections::HashMap;

use crate::{data::Loadable, generator::{buildings::{roofs::Roof, walls::Wall}, materials::{Material, MaterialId, Palette, PaletteId}, nbts::{Structure, StructureId}}};

pub struct LoadedData {
    pub palettes : HashMap<PaletteId, Palette>,
    pub materials : HashMap<MaterialId, Material>,
    pub structures : HashMap<StructureId, Structure>,
    pub walls : HashMap<StructureId, Wall>,
    pub roofs : HashMap<StructureId, Roof>,
}

impl LoadedData {
    pub fn load() -> anyhow::Result<Self> {
        Ok(Self {
            palettes: Palette::load()?,
            materials: Material::load()?,
            structures: Structure::load()?,
            walls: Wall::load()?,
            roofs: Roof::load()?,
        })
    }
}