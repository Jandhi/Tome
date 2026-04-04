use std::collections::HashMap;

use serde_derive::Deserialize;

use crate::data::Loadable;
use crate::minecraft::{Block, string_to_block};
use super::{BlockLayer, CellConstraint, FacingMode, FurnitureItem, PlacedBlock, PlacedConstraint, FurnitureList};

// ---------------------------------------------------------------------------
// JSON deserialization structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct FurnitureItemDef {
    pub name: String,
    #[serde(default)]
    pub unique: bool,
    pub blocks: Vec<PlacedBlockDef>,
    #[serde(default)]
    pub constraints: Vec<PlacedConstraintDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlacedBlockDef {
    pub block: String,
    pub offset: [i32; 3],
    pub layer: BlockLayer,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlacedConstraintDef {
    pub offset: [i32; 2],
    pub constraint: CellConstraint,
    #[serde(default)]
    pub facing: FacingMode,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoomFurnitureDef {
    pub room_type: String,
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
}

// ---------------------------------------------------------------------------
// Loadable implementations
// ---------------------------------------------------------------------------

impl Loadable<'_, FurnitureItemDef, String> for FurnitureItemDef {
    fn get_key(item: &FurnitureItemDef) -> String {
        item.name.clone()
    }

    fn path() -> &'static str {
        "furniture/items"
    }
}

impl Loadable<'_, RoomFurnitureDef, String> for RoomFurnitureDef {
    fn get_key(item: &RoomFurnitureDef) -> String {
        item.room_type.clone()
    }

    fn path() -> &'static str {
        "furniture/rooms"
    }
}

// ---------------------------------------------------------------------------
// Conversion to runtime types
// ---------------------------------------------------------------------------

impl FurnitureItemDef {
    pub fn to_item(&self) -> FurnitureItem {
        FurnitureItem {
            name: self.name.clone(),
            unique: self.unique,
            blocks: self.blocks.iter().map(|b| PlacedBlock {
                block: string_to_block(&b.block)
                    .unwrap_or_else(|| Block::from_id(b.block.as_str().into())),
                offset: (b.offset[0], b.offset[1], b.offset[2]),
                layer: b.layer,
            }).collect(),
            constraints: self.constraints.iter().map(|c| PlacedConstraint {
                offset: (c.offset[0], c.offset[1]),
                constraint: c.constraint,
                facing: c.facing,
            }).collect(),
        }
    }
}

/// Resolve a room type's furniture list from loaded data.
pub fn resolve_furniture_list(
    room_type_key: &str,
    room_defs: &HashMap<String, RoomFurnitureDef>,
    item_defs: &HashMap<String, FurnitureItemDef>,
) -> FurnitureList {
    let room_def = match room_defs.get(room_type_key) {
        Some(def) => def,
        None => return FurnitureList { required: vec![], optional: vec![] },
    };

    let resolve_names = |names: &[String]| -> Vec<FurnitureItem> {
        names.iter()
            .filter_map(|name| item_defs.get(name))
            .map(|def| def.to_item())
            .collect()
    };

    FurnitureList {
        required: resolve_names(&room_def.required),
        optional: resolve_names(&room_def.optional),
    }
}
