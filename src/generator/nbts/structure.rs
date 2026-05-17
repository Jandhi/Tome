
use std::hash::Hash;

use serde_derive::{Deserialize, Serialize};
use crate::{data::Loadable, generator::{materials::PaletteId, nbts::NBTMeta, style::Style}, geometry::{Cardinal, Point3D}};

/// Identifies a *kind* of structure (e.g. `"woodcutter"`). Used as the key
/// into the loaded template registry — every loaded NBT template has exactly
/// one `StructureType`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StructureType(pub String);
impl From<String> for StructureType {
    fn from(id: String) -> Self {
        StructureType(id)
    }
}

impl From<&str> for StructureType {
    fn from(id: &str) -> Self {
        StructureType(id.to_string())
    }
}

/// Unique runtime identifier for a *placed instance* of a structure, paired
/// with the type so callers can ask both "which one?" and "what kind?". Minted
/// at placement time from a counter on `World`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructureID {
    pub id: u32,
    pub structure_type: StructureType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Structure {
    pub id : StructureType,
    #[serde(flatten)]
    pub meta : NBTMeta,
    #[serde(default)]
    pub facing : Cardinal,
    #[serde(default)]
    pub origin : Point3D,
    pub palette : Option<PaletteId>,
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

    /// (size_x, size_z) of the NBT bounding box. Read from the JSON sidecar
    /// (defaults to (0, 0) when absent). Placement rejects structures with
    /// non-positive size, so structures that go through placement must
    /// declare this in their JSON.
    #[serde(default)]
    pub size_xz : (i32, i32),

    /// Number of blocks the structure extends below `origin.y` (i.e. the
    /// depth of foundation/cellar). Placement adds this to the target ground
    /// height so the lowest block embeds at ground level.
    #[serde(default)]
    pub y_offset : i32,
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

impl Loadable<'_, Structure, StructureType> for Structure {
    fn get_key(item: &Structure) -> StructureType {
        item.id.clone()
    }

    fn post_load(items : &mut std::collections::HashMap<StructureType, Structure>) -> anyhow::Result<()> {
        for item in items.values_mut() {
            item.meta.path = item.meta.path.replace('\\', "/");
        }
        Ok(())
    }

    fn path() -> &'static str {
        "structures"
    }
}
