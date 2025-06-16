use std::hash::Hash;

use serde_derive::{Deserialize, Serialize};
use crate::{data::Loadable, generator::{materials::PaletteId, nbts::NBTMeta, style::Style}, geometry::{Cardinal, Point3D}};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StructureId(pub String);

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Structure {
    pub id : StructureId,
    #[serde(flatten)]
    pub meta : NBTMeta,
    #[serde(default)]
    pub facing : Cardinal,
    #[serde(default)]
    pub origin : Point3D,
    pub palette : PaletteId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags : Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mirror_x : bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mirror_z : bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style : Option<Style>,
    #[serde(default = "default_weight")]
    pub weight : f32,
}

fn default_weight() -> f32 {
    1.0
}

impl PartialEq for Structure {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Structure {}

impl Hash for Structure {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
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