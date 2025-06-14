use serde_derive::{Deserialize, Serialize};
use crate::{data::Loadable, generator::nbts::NBTMeta, geometry::{Cardinal, Point3D}};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StructureId(String);

impl From<String> for StructureId {
    fn from(id: String) -> Self {
        StructureId(id)
    }
}

impl From<&str> for StructureId {
    fn from(id: &str) -> Self {
        StructureId(id.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Structure {
    pub id : StructureId,
    #[serde(flatten)]
    pub meta : NBTMeta,
    #[serde(default)]
    pub facing : Cardinal,
    #[serde(default)]
    pub origin : Point3D
}

impl Loadable<'_, Structure, StructureId> for Structure {
    fn get_key(item: &Structure) -> StructureId {
        item.id.clone()
    }

    fn post_load(items : &mut std::collections::HashMap<StructureId, Structure>) -> anyhow::Result<()> {
        for item in items.values_mut() {
            item.meta.path = item.meta.path.replace('\\', "/");
        }
        Ok(())
    }

    fn path() -> &'static str {
        "structures"
    }
}