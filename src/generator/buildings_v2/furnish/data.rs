use std::collections::HashMap;

use serde_derive::Deserialize;

use crate::data::{load_yaml, load_yaml_dir};
use super::{BlockLayer, CellConstraint, FacingMode};

// ---------------------------------------------------------------------------
// Palette swap tag
// ---------------------------------------------------------------------------

/// How a furniture block adapts to the building's palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PaletteSwap {
    /// Literal block — no substitution.
    #[default]
    None,
    /// Replace via palette's PrimaryWood material (stairs, trapdoor, sign, …).
    Wood,
    /// Recolor via palette's primary_color (bed, carpet, banner, …).
    Color,
}

// ---------------------------------------------------------------------------
// YAML data types
// ---------------------------------------------------------------------------

/// All furniture data, loaded from `data/furniture/*.yaml` and `data/rooms.yaml`.
#[derive(Debug, Clone, Deserialize)]
pub struct FurnitureData {
    pub items: HashMap<String, Furniture>,
    pub rooms: HashMap<String, RoomFurnitureList>,
}

impl FurnitureData {
    pub fn load() -> anyhow::Result<Self> {
        let data = Self {
            items: load_yaml_dir("furniture")?,
            rooms: load_yaml("rooms.yaml")?,
        };
        data.validate()?;
        Ok(data)
    }

    /// Every name referenced by a room's required/optional list must resolve
    /// to at least one item — either by name or by being present in some
    /// item's `tags` list. Catches typos and dangling references at load time.
    fn validate(&self) -> anyhow::Result<()> {
        for (room_key, list) in &self.rooms {
            for entry in list.required.iter().chain(list.optional.iter()) {
                let matches = self.items.iter().any(|(name, item)| {
                    name == entry || item.tags.iter().any(|t| t == entry)
                });
                if !matches {
                    anyhow::bail!(
                        "rooms.yaml: room '{}' references '{}' but no furniture item matches by name or tag",
                        room_key, entry,
                    );
                }
            }
        }
        Ok(())
    }
}

/// A furniture piece definition.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Furniture {
    #[serde(default)]
    pub unique: bool,
    /// Group labels this item belongs to. A room reference like `bed` in
    /// rooms.yaml resolves to every item tagged `bed` (plus the implicit
    /// self-tag: an item named `chair` always matches `chair`).
    #[serde(default)]
    pub tags: Vec<String>,
    /// Minimum interior area of the room this item may be placed in.
    #[serde(default)]
    pub min_room_area: Option<i32>,
    /// Maximum interior area of the room this item may be placed in.
    /// Used to exclude small variants from large rooms (e.g. single_bed
    /// capped so big bedrooms always get a double or canopy bed).
    #[serde(default)]
    pub max_room_area: Option<i32>,
    pub blocks: Vec<FurnitureBlock>,
    #[serde(default)]
    pub constraints: Vec<FurnitureConstraint>,
}

/// A block within a furniture piece.
#[derive(Debug, Clone, Deserialize)]
pub struct FurnitureBlock {
    pub block: String,
    pub offset: [i32; 3],
    pub layer: BlockLayer,
    /// Palette substitution mode. Omit in YAML for literal blocks.
    #[serde(default)]
    pub swap: PaletteSwap,
    /// If true, the block still needs an empty cell to place but leaves the
    /// cell walkable afterwards (UnblockedReachable) instead of Blocked.
    /// Used for slabs, carpets, and other half-height or decor blocks that
    /// Minecraft lets the player walk on top of.
    #[serde(default)]
    pub walkable: bool,
}

/// A floor cell constraint within a furniture piece.
#[derive(Debug, Clone, Deserialize)]
pub struct FurnitureConstraint {
    pub offset: [i32; 2],
    pub constraint: CellConstraint,
    #[serde(default)]
    pub facing: FacingMode,
}

/// Which furniture items a room type requires and optionally includes.
#[derive(Debug, Clone, Deserialize)]
pub struct RoomFurnitureList {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
    /// Override the default fill-ratio cap for this room. When set, the
    /// optional list is also retried in passes until no item fits or the
    /// threshold is reached — useful for "as full as possible" rooms like
    /// storage and pantry.
    #[serde(default)]
    pub fill_threshold: Option<f32>,
}
