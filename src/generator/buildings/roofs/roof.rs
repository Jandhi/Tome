use std::collections::HashMap;

use serde_derive::{Serialize, Deserialize};
use strum::IntoEnumIterator;

use crate::{data::Loadable, editor::Editor, generator::{buildings::BuildingData, data::LoadedData, materials::Placer, nbts::{place_nbt, place_structure, Structure, StructureId}, style::Style}, geometry::{Cardinal, Point3D, NORTH, UP, WEST}, minecraft::BlockID, noise::RNG};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoofSetId(pub String);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RoofSet {
    pub id : RoofSetId,
    pub style : Style,
    pub side : StructureId,
    pub corner : StructureId,
    pub inner : StructureId,
}

impl Loadable<'_, RoofSet, RoofSetId> for RoofSet {
    fn get_key(item: &RoofSet) -> RoofSetId {
        item.id.clone()
    }

    fn post_load(_items: &mut std::collections::HashMap<RoofSetId, RoofSet>) -> anyhow::Result<()> {
        Ok(())
    }

    fn path() -> &'static str {
        "buildings/roofs/sets"
    }
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RoofComponent {
    #[serde(flatten)]
    pub structure : Structure,

    pub(crate) roof_type : RoofType,
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

impl Loadable<'_, RoofComponent, StructureId> for RoofComponent {
    fn get_key(item: &RoofComponent) -> StructureId {
        item.structure.id.clone()
    }

    fn post_load(_items: &mut std::collections::HashMap<StructureId, RoofComponent>) -> anyhow::Result<()> {
        Ok(())
    }

    fn path() -> &'static str {
        "buildings/roofs/components"
    }
}

pub async fn build_roof(editor: &mut Editor, data: &LoadedData, building : &BuildingData, rng : &mut RNG) -> anyhow::Result<()> {
    let mut placer_rng = rng.derive();
    let mut placer = Placer::new(&data.materials, &mut placer_rng);

    let sets = data.roof_sets.values().filter(|set| set.style == building.style).collect::<Vec<_>>();
    let roof_set = rng.choose(&sets);

    let side = data.roof_components.get(&roof_set.side).expect("Roof set should have a side component");
    let corner = data.roof_components.get(&roof_set.corner).expect("Roof set should have a corner component");
    let inner = data.roof_components.get(&roof_set.inner).expect("Roof set should have an inner component");

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
            let mut offset = building.grid.get_door_world_position(*cell + UP, direction.rotate_left());
            


            if !neighbours[&direction] && !neighbours[&direction.rotate_left()] {
                offset += Point3D::from(direction) * (match direction {
                    Cardinal::North | Cardinal::South => building.grid.cell_size.z / 2,
                    Cardinal::East | Cardinal::West => building.grid.cell_size.x / 2,
                });
                place_structure(editor, Some(&mut placer), &corner.structure, offset, direction, Some(data), Some(&building.palette), false ,false).await?;
            }
            else if !neighbours[&direction] {
                offset += Point3D::from(direction.rotate_right()) * (match direction {
                    Cardinal::North | Cardinal::South => building.grid.cell_size.z / 2,
                    Cardinal::East | Cardinal::West => building.grid.cell_size.x / 2,
                }) + Point3D::from(direction) * (match direction {
                    Cardinal::North | Cardinal::South => building.grid.cell_size.z / 2,
                    Cardinal::East | Cardinal::West => building.grid.cell_size.x / 2,
                });

                place_structure(editor, Some(&mut placer), &side.structure, offset, direction.rotate_right(), Some(data), Some(&building.palette), false, false).await?;
            }
            else if !neighbours[&direction.rotate_left()] {
                place_structure(editor, Some(&mut placer), &side.structure, offset, direction, Some(data), Some(&building.palette), false, true).await?;
            }
            else {
                offset += Point3D::from(direction) * (match direction {
                    Cardinal::North | Cardinal::South => building.grid.cell_size.z / 2,
                    Cardinal::East | Cardinal::West => building.grid.cell_size.x / 2,
                });
                place_structure(editor, Some(&mut placer), &inner.structure, offset, direction, Some(data), Some(&building.palette), false, false).await?;
            }
        }
    }

    Ok(())
}