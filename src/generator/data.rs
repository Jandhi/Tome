use std::collections::HashMap;

use crate::{data::{Loadable, load_yaml}, generator::{districts::{PaintPalette, PaintPaletteId, PaintPalettesFile}, resource_chain::ResourceRegistry, buildings::{roofs::{RoofComponent, RoofSet, RoofSetId}, walls::{WallComponent, WallSet, WallSetId}, BuildingSet, BuildingSetID}, buildings_v2::furnish::data::FurnitureData, materials::{Material, MaterialId, Palette, PaletteId}, nbts::{Structure, StructureType}}};

#[derive(Debug)]
pub struct LoadedData {
    pub palettes : HashMap<PaletteId, Palette>,
    pub materials : HashMap<MaterialId, Material>,
    pub structures : HashMap<StructureType, Structure>,
    pub wall_components : HashMap<StructureType, WallComponent>,
    pub wall_sets : HashMap<WallSetId, WallSet>,
    pub roof_components : HashMap<StructureType, RoofComponent>,
    pub roof_sets : HashMap<RoofSetId, RoofSet>,
    pub building_sets : HashMap<BuildingSetID, BuildingSet>,
    pub resource_registry : ResourceRegistry,
    pub furniture : FurnitureData,
    pub paint_palettes : HashMap<PaintPaletteId, PaintPalette>,
}

impl LoadedData {
    pub fn load() -> anyhow::Result<Self> {
        let structures = Structure::load()?;
        let resource_registry = ResourceRegistry::load()?;
        resource_registry.validate_buildings(&structures)?;

        let paint_palettes = {
            let file: PaintPalettesFile = load_yaml("paint_palettes/palettes.yaml")?;
            file.paint_palettes.into_iter().map(|(k, v)| (PaintPaletteId(k), v)).collect()
        };

        Ok(Self {
            palettes: Palette::load()?,
            materials: Material::load()?,
            structures,
            wall_components: WallComponent::load()?,
            wall_sets: WallSet::load()?,
            roof_components: RoofComponent::load()?,
            roof_sets: RoofSet::load()?,
            building_sets: BuildingSet::load()?,
            resource_registry,
            furniture: FurnitureData::load()?,
            paint_palettes,
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
        assert!(!data.paint_palettes.is_empty(), "no paint palettes loaded");
        assert!(!data.resource_registry.production_painters.is_empty(), "no production painters loaded");
    }
}
