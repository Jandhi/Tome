use std::collections::HashMap;

use serde_derive::{Serialize, Deserialize};
use strum::IntoEnumIterator;

use crate::{data::Loadable, editor::Editor, generator::{buildings::BuildingData, data::LoadedData, nbts::{place_nbt, place_structure, Structure, StructureId}}, geometry::{Cardinal, Point3D, NORTH, UP, WEST}};

#[derive(Serialize, Deserialize)]
pub struct Roof {
    #[serde(flatten)]
    structure : Structure,

    #[serde(rename = "type")]
    roof_type : RoofType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoofType {
    Gable,
    Hip(HipRoofPart)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HipRoofPart {
    Side,
    Corner,
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

        let coords = building.grid.grid_to_world(*cell + UP);

        let neighbours : HashMap<Cardinal, bool> = Cardinal::iter()
            .map(|direction| {
                let neighbour_cell = *cell + direction.into();
                let has_neighbour = building.shape.cells().iter().any(|other_cell| *other_cell == neighbour_cell);
                (direction, has_neighbour)
            })
            .collect();

        for direction in Cardinal::iter() {
            let offset = match direction {
                Cardinal::North => Point3D::new(0, 0, 0),
                Cardinal::East => Point3D::new(building.grid.cell_size.x / 2, 0, 0),
                Cardinal::South => Point3D::new(building.grid.cell_size.x / 2, 0, building.grid.cell_size.z / 2),
                Cardinal::West => Point3D::new(0, 0, building.grid.cell_size.z / 2),
            };

            // if neighbours[&direction] && neighbours[&direction.turn_left()] {
            //     place_structure(editor, &placer, &inner.structure, offset, editor, placer, generator_data, input_palette, output_palette).await;
            // }

            
        }
    }

    Ok(())
}