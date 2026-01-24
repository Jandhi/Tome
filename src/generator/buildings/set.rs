use serde::{Serialize, Deserialize};

use crate::{data::Loadable, generator::{buildings::{roofs::RoofSetId, shape::BuildingShape, walls::WallSetId}, style::Style}};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BuildingSetID(pub String);

impl From<String> for BuildingSetID {
    fn from(value: String) -> Self {
        BuildingSetID(value)
    }
}

impl From<&str> for BuildingSetID {
    fn from(value: &str) -> Self {
        BuildingSetID(value.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingSet {
    pub id : BuildingSetID,
    pub style : Style,
    pub roof_sets : Vec<RoofSetId>,
    pub wall_sets : Vec<WallSetId>,
    pub shapes : Vec<BuildingShape>,
}

impl<'a> Loadable<'a, BuildingSet, BuildingSetID> for BuildingSet {
    fn get_key(item: &BuildingSet) -> BuildingSetID {
        item.id.clone()
    }

    fn post_load(_items : &mut std::collections::HashMap<BuildingSetID, BuildingSet>) -> anyhow::Result<()> {
        Ok(())
    }

    fn path() -> &'static str {
        "buildings/sets"
    }
}