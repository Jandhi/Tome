use std::collections::HashMap;

use crate::{data::Loadable, generator::{buildings::{roofs::{RoofComponent, RoofSet, RoofSetId}, walls::{WallComponent, WallSet, WallSetId}, BuildingSet, BuildingSetID}, buildings_v2::furnish::data::FurnitureData, materials::{Material, MaterialId, Palette, PaletteId}, nbts::{Structure, StructureId}}};

#[derive(Debug, Clone)]
pub struct LoadedData {
    pub palettes : HashMap<PaletteId, Palette>,
    pub materials : HashMap<MaterialId, Material>,
    pub structures : HashMap<StructureId, Structure>,
    pub wall_components : HashMap<StructureId, WallComponent>,
    pub wall_sets : HashMap<WallSetId, WallSet>,
    pub roof_components : HashMap<StructureId, RoofComponent>,
    pub roof_sets : HashMap<RoofSetId, RoofSet>,
    pub building_sets : HashMap<BuildingSetID, BuildingSet>,
    pub furniture : FurnitureData,
}

impl LoadedData {
    pub fn load() -> anyhow::Result<Self> {
        Ok(Self {
            palettes: Palette::load()?,
            materials: Material::load()?,
            structures: Structure::load()?,
            wall_components: WallComponent::load()?,
            wall_sets: WallSet::load()?,
            roof_components: RoofComponent::load()?,
            roof_sets: RoofSet::load()?,
            building_sets: BuildingSet::load()?,
            furniture: FurnitureData::load()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sanity check that every data subsystem loads and has at least one
    /// non-empty entry. Individual file deserialization failures are
    /// swallowed as debug logs in `Loadable::load_all_in`, so this test is
    /// the canonical signal that the overall data tree is healthy.
    #[test]
    fn test_data_loads_cleanly() {
        let data = LoadedData::load().expect("LoadedData::load() failed");
        assert!(!data.palettes.is_empty(), "no palettes loaded");
        assert!(!data.materials.is_empty(), "no materials loaded");
        assert!(!data.structures.is_empty(), "no structures loaded");
        assert!(!data.wall_components.is_empty(), "no wall components loaded");
        assert!(!data.wall_sets.is_empty(), "no wall sets loaded");
        assert!(!data.roof_components.is_empty(), "no roof components loaded");
        assert!(!data.roof_sets.is_empty(), "no roof sets loaded");
        assert!(!data.building_sets.is_empty(), "no building sets loaded");
        assert!(!data.furniture.items.is_empty(), "no furniture items loaded");
        assert!(!data.furniture.rooms.is_empty(), "no room furniture lists loaded");
    }
}