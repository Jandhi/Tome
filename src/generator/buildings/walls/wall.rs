use serde_derive::{Deserialize, Serialize};

use crate::{data::Loadable, generator::nbts::{Structure, StructureId}};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wall {
    #[serde(flatten)]
    pub structure : Structure
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