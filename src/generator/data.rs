use std::collections::HashMap;

<<<<<<< HEAD
use crate::{data::Loadable, generator::{buildings::{roofs::Roof, walls::Wall}, materials::{Material, MaterialId, Palette, PaletteId}, nbts::{Structure, StructureId}}};

=======
use crate::{data::Loadable, generator::{buildings::{roofs::{RoofComponent, RoofSet, RoofSetId}, walls::Wall}, materials::{Material, MaterialId, Palette, PaletteId}, nbts::{Structure, StructureId}}};

#[derive(Debug, Clone)]
>>>>>>> master
pub struct LoadedData {
    pub palettes : HashMap<PaletteId, Palette>,
    pub materials : HashMap<MaterialId, Material>,
    pub structures : HashMap<StructureId, Structure>,
    pub walls : HashMap<StructureId, Wall>,
<<<<<<< HEAD
    pub roofs : HashMap<StructureId, Roof>,
=======
    pub roof_components : HashMap<StructureId, RoofComponent>,
    pub roof_sets : HashMap<RoofSetId, RoofSet>,
>>>>>>> master
}

impl LoadedData {
    pub fn load() -> anyhow::Result<Self> {
        Ok(Self {
            palettes: Palette::load()?,
            materials: Material::load()?,
            structures: Structure::load()?,
            walls: Wall::load()?,
<<<<<<< HEAD
            roofs: Roof::load()?,
=======
            roof_components: RoofComponent::load()?,
            roof_sets: RoofSet::load()?,
>>>>>>> master
        })
    }
}