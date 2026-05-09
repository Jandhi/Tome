
use std::hash::Hash;
use std::io::Read;

use flate2::read::GzDecoder;
use log::warn;
use serde_derive::{Deserialize, Serialize};
use crate::{data::Loadable, generator::{materials::PaletteId, nbts::{NBTMeta, NBTStructure}, style::Style}, geometry::{Cardinal, Point3D}};

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

    /// (size_x, size_z) cached from the NBT bounding box at load time. Defaults
    /// to (0, 0) if the NBT could not be parsed — placement will reject such
    /// structures during candidate scoring.
    #[serde(skip)]
    pub size_xz : (i32, i32),

    /// True iff the NBT contains any block whose y-position is below
    /// `origin.y` (i.e. a foundation or cellar). Placement uses this to
    /// decide whether to embed the building one block into the ground.
    #[serde(skip)]
    pub has_subgrade : bool,
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

            match read_nbt_metadata(&item.meta.path, item.origin.y) {
                Ok((size_xz, has_subgrade)) => {
                    item.size_xz = size_xz;
                    item.has_subgrade = has_subgrade;
                }
                Err(e) => {
                    warn!(
                        "Failed to read NBT metadata for structure '{}' at {}: {}",
                        item.id.0, item.meta.path, e
                    );
                }
            }
        }
        Ok(())
    }

    fn path() -> &'static str {
        "structures"
    }
}

/// Parse just enough of an NBT structure file to extract its (size_x, size_z)
/// bounding box and whether any block sits below the structure's origin
/// (indicating a foundation/cellar).
fn read_nbt_metadata(path: &str, origin_y: i32) -> anyhow::Result<((i32, i32), bool)> {
    let nbt_data = std::fs::read(path)?;
    let parsed: Result<NBTStructure, fastnbt::error::Error> = fastnbt::from_bytes(&nbt_data);
    let structure = match parsed {
        Ok(s) => s,
        Err(_) => {
            let mut decoder = GzDecoder::new(nbt_data.as_slice());
            let mut buf = vec![];
            decoder.read_to_end(&mut buf)?;
            fastnbt::from_bytes(&buf)?
        }
    };

    let size_xz = (structure.size[0], structure.size[2]);
    let has_subgrade = structure.blocks.iter().any(|b| b.pos[1] < origin_y);
    Ok((size_xz, has_subgrade))
}
