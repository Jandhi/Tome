use std::collections::HashMap;

use serde_derive::{Deserialize, Serialize};
use strum::IntoEnumIterator;

use crate::{data::Loadable, editor::Editor, generator::{buildings::BuildingData, data::LoadedData, materials::{Material, MaterialId, Palette, PaletteId, Placer}, nbts::{Structure, StructureId}}, geometry::{Cardinal, CARDINALS}};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wall {
    #[serde(flatten)]
    pub structure : Structure
}

pub async fn build_walls(editor : &mut Editor, walls : &[&Wall], building : &BuildingData, data : &LoadedData, palette : &PaletteId) -> anyhow::Result<()> {
    for cell in building.shape.cells().iter() {
        for direction in Cardinal::iter() {
            if building.shape.cells().iter().any(|other_cell| *other_cell == *cell + direction.into()) {
                continue;
            }

            let wall = &walls[0]; // todo: handle multiple walls

            let placer = Placer::new(&data.materials);
            building.grid.build_structure(editor, &placer, &wall.structure, *cell, direction, data, palette).await?;
        }
    }

    Ok(())
}

impl Loadable<'_, Wall, StructureId> for Wall {
    fn get_key(item: &Wall) -> StructureId {
        item.structure.id.clone()
    }

    fn post_load(_items: &mut std::collections::HashMap<StructureId, Wall>) -> anyhow::Result<()> {
        Ok(())
    }

    fn path() -> &'static str {
        "buildings/walls"
    }
}