use std::collections::HashMap;

use serde_derive::{Serialize, Deserialize};
use strum::IntoEnumIterator;

use crate::{data::Loadable, editor::Editor, generator::{buildings::BuildingData, data::LoadedData, nbts::{place_nbt, place_structure, Structure, StructureId}}, geometry::{Cardinal, Point3D, NORTH, UP, WEST}, minecraft::BlockID};

#[derive(Serialize, Deserialize)]
pub struct Roof {
    #[serde(flatten)]
    structure : Structure,

    roof_type : RoofType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "shape")]
pub enum RoofType {
    #[serde(rename = "gable")]
    Gable,
    #[serde(rename = "hip")]
    Hip(HipRoofPart)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HipRoofPart {
    #[serde(rename = "side")]
    Side,
    #[serde(rename = "corner")]
    Corner,
    #[serde(rename = "inner")]
    Inner,
}

impl Loadable<'_, Roof, StructureId> for Roof {
    fn get_key(item: &Roof) -> StructureId {
        item.structure.id.clone()
    }

    fn post_load(_items: &mut std::collections::HashMap<StructureId, Roof>) -> anyhow::Result<()> {
        Ok(())
    }

    fn path() -> &'static str {
        "buildings/roofs"
    }
}

pub async fn build_roof(editor: &mut Editor, data: &LoadedData, building : &BuildingData) -> anyhow::Result<()> {
    let placer = crate::generator::materials::Placer::new(&data.materials);
    
    let side = data.roofs.values().find(|roof| roof.roof_type == RoofType::Hip(HipRoofPart::Side)).expect("No side roof found");
    let corner = data.roofs.values().find(|roof| roof.roof_type == RoofType::Hip(HipRoofPart::Corner)).expect("No corner roof found");
    let inner = data.roofs.values().find(|roof| roof.roof_type == RoofType::Hip(HipRoofPart::Inner)).expect("No inner roof found");

    for cell in building.shape.cells().iter() {
        if building.shape.cells().iter().any(|other_cell| *other_cell == *cell + UP) {
            continue; // skip cells that have a roof above them
        }

        let neighbours : HashMap<Cardinal, bool> = Cardinal::iter()
            .map(|direction| {
                let neighbour_cell = *cell + direction.into();
                let has_neighbour = building.shape.cells().iter().any(|other_cell| *other_cell == neighbour_cell);
                (direction, has_neighbour)
            })
            .collect();

        for direction in Cardinal::iter() {
            let mut offset = building.grid.get_door_world_position(*cell + UP, direction.turn_left());
            


            if !neighbours[&direction] && !neighbours[&direction.turn_left()] {
                offset += Point3D::from(direction) * (match direction {
                    Cardinal::North | Cardinal::South => building.grid.cell_size.z / 2,
                    Cardinal::East | Cardinal::West => building.grid.cell_size.x / 2,
                });
                place_structure(editor, Some(&placer), &corner.structure, offset, direction, Some(data), Some(&building.palette), false, false).await?;
            }
            else if !neighbours[&direction] {
                offset += Point3D::from(direction.turn_right()) * (match direction {
                    Cardinal::North | Cardinal::South => building.grid.cell_size.z / 2,
                    Cardinal::East | Cardinal::West => building.grid.cell_size.x / 2,
                }) + Point3D::from(direction) * (match direction {
                    Cardinal::North | Cardinal::South => building.grid.cell_size.z / 2,
                    Cardinal::East | Cardinal::West => building.grid.cell_size.x / 2,
                });

                place_structure(editor, Some(&placer), &side.structure, offset, direction.turn_right(), Some(data), Some(&building.palette), false, false).await?;
            }
            else if !neighbours[&direction.turn_left()] {
                place_structure(editor, Some(&placer), &side.structure, offset, direction, Some(data), Some(&building.palette), false, true).await?;
            }
            else {
                offset += Point3D::from(direction) * (match direction {
                    Cardinal::North | Cardinal::South => building.grid.cell_size.z / 2,
                    Cardinal::East | Cardinal::West => building.grid.cell_size.x / 2,
                });
                place_structure(editor, Some(&placer), &inner.structure, offset, direction, Some(data), Some(&building.palette), false, false).await?;
            }
        }
    }

    Ok(())
}