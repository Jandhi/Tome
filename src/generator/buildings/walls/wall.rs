use std::collections::HashMap;

use serde_derive::{Deserialize, Serialize};
use strum::IntoEnumIterator;

use crate::{data::Loadable, editor::Editor, generator::{buildings::BuildingData, data::LoadedData, materials::{Material, MaterialId, Palette, PaletteId, Placer}, nbts::{Structure, StructureId}}, geometry::{Cardinal, CARDINALS, DOWN, UP}, noise::RNG};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wall {
    #[serde(flatten)]
    pub structure : Structure,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_type : Option<WallType>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical_position : Option<VerticalWallPosition>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizontal_position : Option<HorizontalWallPosition>,
}

impl PartialEq for Wall {
    fn eq(&self, other: &Self) -> bool {
        self.structure == other.structure
    }
}

impl Eq for Wall {}

impl std::hash::Hash for Wall {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.structure.hash(state)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WallType {
    #[serde(rename = "window")]
    Window,
    #[serde(rename = "door")]
    Door,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerticalWallPosition {
    #[serde(rename = "top")]
    Top, // Can only be on the top layer of a building
    #[serde(rename = "non_bottom")]
    NonBottom, // Can be on any layer except the bottom layer of a building
    #[serde(rename = "bottom")]
    Bottom, // Can only be on the bottom layer of a building
    #[serde(rename = "middle")]
    Middle, // Cannot be on the top or bottom layer of a building
    #[serde(rename = "single")]
    Single, // Has nothing above or below it
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HorizontalWallPosition {
    #[serde(rename = "end")]
    End, // Has no neighbours,
}

pub async fn build_walls(editor : &mut Editor, walls : &[&Wall], building : &BuildingData, data : &LoadedData, rng : &mut RNG) -> anyhow::Result<()> {
    
    
    for cell in building.shape.cells().iter() {
        let is_bottom = !building.shape.cells().contains(&(*cell + DOWN));
        let is_top = !building.shape.cells().contains(&(*cell + UP));

        for direction in Cardinal::iter() {

            if building.shape.cells().iter().any(|other_cell| *other_cell == *cell + direction.into()) {
                continue;
            }

            let walls = walls.iter().filter(|wall| {
                wall.structure.style.is_some_and(|style| style == building.style) &&
                wall.vertical_position.is_none_or(|pos| {
                    match pos {
                        VerticalWallPosition::Top => is_top && !is_bottom,
                        VerticalWallPosition::NonBottom => !is_bottom,
                        VerticalWallPosition::Bottom => is_bottom && !is_top,
                        VerticalWallPosition::Middle => !is_top && !is_bottom,
                        VerticalWallPosition::Single => is_bottom && is_top,
                    }
                })
            }).map(|wall| (*wall, wall.structure.weight))
            .collect::<HashMap<_, _>>();

            let wall = *rng.choose_weighted(&walls);

            let mut placer = Placer::new(&data.materials, rng);
            building.grid.build_structure(editor, &mut placer, &wall.structure, *cell, direction, data, &building.palette).await?;
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