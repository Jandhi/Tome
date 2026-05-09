use std::collections::HashMap;

use crate::{data::Loadable, generator::{resource_chain::ResourceRegistry,buildings::{roofs::{RoofComponent, RoofSet, RoofSetId}, walls::{WallComponent, WallSet, WallSetId}, BuildingSet, BuildingSetID}, buildings_v2::furnish::data::{FurnitureItemDef, RoomFurnitureDef}, materials::{Material, MaterialId, Palette, PaletteId}, nbts::{Structure, StructureId}}};

#[derive(Debug)]
pub struct LoadedData {
    pub palettes : HashMap<PaletteId, Palette>,
    pub materials : HashMap<MaterialId, Material>,
    pub structures : HashMap<StructureId, Structure>,
    pub wall_components : HashMap<StructureId, WallComponent>,
    pub wall_sets : HashMap<WallSetId, WallSet>,
    pub roof_components : HashMap<StructureId, RoofComponent>,
    pub roof_sets : HashMap<RoofSetId, RoofSet>,
    pub building_sets : HashMap<BuildingSetID, BuildingSet>,
    pub resource_registry : ResourceRegistry,
    pub furniture_items : HashMap<String, FurnitureItemDef>,
    pub room_furniture : HashMap<String, RoomFurnitureDef>,
}

impl LoadedData {
    pub fn load() -> anyhow::Result<Self> {
        let structures = Structure::load()?;
        let resource_registry = ResourceRegistry::load()?;
        resource_registry.validate_buildings(&structures)?;

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
            furniture_items: FurnitureItemDef::load()?,
            room_furniture: RoomFurnitureDef::load()?,
        })
    }
}