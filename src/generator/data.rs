use std::collections::HashMap;

use crate::{data::Loadable, generator::{buildings::{roofs::{RoofComponent, RoofSet, RoofSetId}, walls::Wall}, materials::{Material, MaterialId, Palette, PaletteId}, nbts::{Structure, StructureId}}};

#[derive(Debug, Clone)]
pub struct LoadedData {
    pub palettes : HashMap<PaletteId, Palette>,
    pub materials : HashMap<MaterialId, Material>,
    pub structures : HashMap<StructureId, Structure>,
    pub walls : HashMap<StructureId, Wall>,
    pub roof_components : HashMap<StructureId, RoofComponent>,
    pub roof_sets : HashMap<RoofSetId, RoofSet>,
}

impl LoadedData {
    pub fn load() -> anyhow::Result<Self> {
        Ok(Self {
            palettes: Palette::load()?,
            materials: Material::load()?,
            structures: Structure::load()?,
            walls: Wall::load()?,
            roof_components: RoofComponent::load()?,
            roof_sets: RoofSet::load()?,
        })
    }
}