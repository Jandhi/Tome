use serde_derive::{Deserialize, Serialize};

use crate::{data::Loadable, generator::nbts::StructureId};


#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WallSetId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallSet {
    pub id: WallSetId,
    pub components: Vec<StructureId>,
}

impl Loadable<'_, WallSet, WallSetId> for WallSet {
    fn get_key(item: &WallSet) -> WallSetId {
        item.id.clone()
    }

    fn post_load(_items: &mut std::collections::HashMap<WallSetId, WallSet>) -> anyhow::Result<()> {
        Ok(())
    }

    fn path() -> &'static str {
        "buildings/walls/sets"
    }
}