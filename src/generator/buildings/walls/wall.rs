
use std::collections::HashMap;

use log::error;
use serde_derive::{Deserialize, Serialize};
use strum::IntoEnumIterator;

use crate::{data::Loadable, editor::Editor, generator::{buildings::{shape::WallPlacement, walls::WallSetId, BuildingData}, data::LoadedData, materials::{Material, MaterialId, Palette, PaletteId, Placer}, nbts::{Structure, StructureId}}, geometry::{Cardinal, CARDINALS, DOWN, UP}, noise::RNG};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallComponent {
    #[serde(flatten)]
    pub structure : Structure,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_type : Option<WallType>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical_position : Option<VerticalWallPosition>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizontal_position : Option<HorizontalWallPosition>,
}

impl PartialEq for WallComponent {
    fn eq(&self, other: &Self) -> bool {
        self.structure == other.structure
    }
}

impl Eq for WallComponent {}

impl std::hash::Hash for WallComponent {
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
    #[serde(rename = "support")]
    Support 
    /*
        This is for decorating below raised cells like so    XXX
        where X is a cell and ^ is a support                 X^X
    */
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

pub async fn build_walls(editor : &mut Editor, wall_set : &WallSetId, building : &mut BuildingData, data : &LoadedData, rng : &mut RNG) -> anyhow::Result<()> {
    let walls = data.wall_sets.get(wall_set)
        .expect("Wall set not found")
        .components
        .iter()
        .filter_map(|id| data.wall_components.get(id))
        .collect::<Vec<_>>();

    let mut windows = vec![];

    let lowest_level = building.shape.cells().iter()
        .map(|point| point.y)
        .min();
    
    for cell in building.shape.cells().iter() {
        let is_bottom = !building.shape.cells().contains(&(*cell + DOWN));
        let is_top = !building.shape.cells().contains(&(*cell + UP));
        let mut non_window_count = 0;

        for direction in Cardinal::iter() {
            let is_door = building.shape.has_door(*cell, direction);

            if building.shape.cells().iter().any(|other_cell| *other_cell == *cell + direction.into()) {
                continue;
            }

            let walls = walls.iter().filter(|wall| {
                wall.vertical_position.is_none_or(|pos| {
                    match pos {
                        VerticalWallPosition::Top => is_top,
                        VerticalWallPosition::NonBottom => !is_bottom,
                        VerticalWallPosition::Bottom => is_bottom,
                        VerticalWallPosition::Middle => !is_top && !is_bottom,
                        VerticalWallPosition::Single => is_bottom && is_top,
                    }
                }) // <-- close is_none_or
                && ((wall.wall_type == Some(WallType::Door)) == is_door)
                && wall.wall_type != Some(WallType::Support) // Exclude support gates from regular wall placements
            }) // <-- close filter
            .map(|wall| {
                let mut weight = wall.structure.weight;
                if wall.wall_type == Some(WallType::Window) {
                    if cell.y > 0 {
                        weight *= 2.0; // Increase weight for windows on higher floors
                    }

                    weight *= (1 << non_window_count) as f32; // Increase weight based on the number of non-window walls placed
                }
                (*wall, weight)
            })
            .collect::<HashMap<_, _>>();

            if walls.is_empty() {
                error!("No walls available for cell {:?} in direction {:?}", cell, direction);
                continue; // No walls available for this cell and direction
            }

            let wall = *rng.choose_weighted(&walls);

            if wall.wall_type == Some(WallType::Window) {
                windows.push(WallPlacement {
                    cell: *cell,
                    direction,
                });
            } else {
                non_window_count += 1;
            }

            let mut placer = Placer::new(&data.materials, rng);
            building.grid.build_structure(editor, &mut placer, &wall.structure, *cell, direction, data, &building.palette).await?;
        }

        // Foundations
        if lowest_level.is_some_and(|lowest| cell.y != lowest) && !building.shape.has_cell(*cell + DOWN) {
            for direction in Cardinal::iter() {
                if !building.shape.has_cell(*cell + DOWN + direction.into()) {
                    let walls = walls.iter().filter(|wall| {
                        wall.vertical_position.is_none_or(|pos| pos == VerticalWallPosition::Bottom)
                            && wall.wall_type == Some(WallType::Support)
                    }).map(|wall| (*wall, wall.structure.weight))
                    .collect::<HashMap<_, _>>();

                    if walls.is_empty() {
                        error!("No support walls available for cell {:?} in direction {:?}", cell, direction);
                        continue; // No support walls available for this cell and direction
                    }

                    let wall = *rng.choose_weighted(&walls);

                    let mut placer = Placer::new(&data.materials, rng);
                    building.grid.build_structure(editor, &mut placer, &wall.structure, *cell + DOWN, direction, data, &building.palette).await?;
                }
            }
        }
    }

    *building.shape.windows_mut() = Some(windows);
    Ok(())
}

impl Loadable<'_, WallComponent, StructureId> for WallComponent {
    fn get_key(item: &WallComponent) -> StructureId {
        item.structure.id.clone()
    }

    fn post_load(_items: &mut std::collections::HashMap<StructureId, WallComponent>) -> anyhow::Result<()> {
        Ok(())
    }

    fn path() -> &'static str {
        "buildings/walls/components"
    }
}