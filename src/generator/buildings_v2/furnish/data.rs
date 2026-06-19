use std::collections::HashMap;

use serde_derive::Deserialize;

use crate::data::{load_yaml, load_yaml_dir};
use crate::generator::population::{SceneKind, SlotRole};
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
    /// Recolor via palette's secondary_color, falling back to primary_color
    /// when the palette has none. Use for accent blocks in patterned items
    /// (e.g. carpet borders, checker squares).
    SecondaryColor,
}

// ---------------------------------------------------------------------------
// YAML data types
// ---------------------------------------------------------------------------

/// All furniture data, loaded from `data/furniture/*.yaml` and `data/rooms.yaml`.
#[derive(Debug, Clone, Deserialize)]
pub struct FurnitureData {
    pub items: HashMap<String, Furniture>,
    pub rooms: HashMap<String, RoomFurnitureList>,
    #[serde(default)]
    pub loot: HashMap<String, LootTable>,
}

impl FurnitureData {
    pub fn load() -> anyhow::Result<Self> {
        let data = Self {
            items: load_yaml_dir("furniture/items")?,
            rooms: load_yaml("rooms.yaml")?,
            loot: load_yaml("furniture/loot.yaml")?,
        };
        data.validate()?;
        Ok(data)
    }

    /// Every name referenced by a room's required/optional list must resolve
    /// to at least one item — either by name or by being present in some
    /// item's `tags` list. Every `loot:` tag on a furniture block must
    /// resolve to a known loot table. Catches typos at load time.
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
        for (name, item) in &self.items {
            for block in &item.blocks {
                if let Some(loot) = &block.loot {
                    if !self.loot.contains_key(loot) {
                        anyhow::bail!(
                            "furniture '{}': block references loot table '{}' which is not defined in furniture/loot.yaml",
                            name, loot,
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

/// A furniture piece definition.
#[derive(Debug, Clone, Deserialize)]
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
    /// Selection weight when multiple candidates match the same rooms.yaml
    /// entry (e.g. all `carpet`-tagged items). Higher = more likely to be
    /// tried first. Defaults to 1.0.
    #[serde(default = "default_weight")]
    pub weight: f32,
    pub blocks: Vec<FurnitureBlock>,
    #[serde(default)]
    pub constraints: Vec<FurnitureConstraint>,
    /// NPC standing-spot scenes this item offers (a worker in front of a
    /// furnace, two diners across a table, …). Offsets are in the same
    /// `[along, away]` local frame as `constraints`, resolved against the
    /// item's placed orientation. Empty for furniture nobody stands at.
    #[serde(default)]
    pub anchors: Vec<AnchorSpec>,
}

impl Default for Furniture {
    fn default() -> Self {
        Self {
            unique: false,
            tags: Vec::new(),
            min_room_area: None,
            max_room_area: None,
            weight: 1.0,
            blocks: Vec::new(),
            constraints: Vec::new(),
            anchors: Vec::new(),
        }
    }
}

/// One NPC scene a furniture item offers. v1 emits these as solo or two-person
/// scenes; the `kind` carries through to the population pass's scene weighting.
#[derive(Debug, Clone, Deserialize)]
pub struct AnchorSpec {
    /// Scene type. Defaults to `solo`.
    #[serde(default = "default_scene_kind")]
    pub kind: SceneKind,
    /// Default dialogue key for this scene's slots (e.g. `crafting`,
    /// `conversation`), indexing a pool in `npcs.yaml`. A slot's own `dialogue`
    /// overrides it. Keys are generic activities reusable across many items.
    #[serde(default)]
    pub dialogue: Option<String>,
    pub slots: Vec<AnchorSlotSpec>,
}

/// One person's spot in an [`AnchorSpec`]. The NPC stands on `offset` (local
/// `[along, away]`, resolved by the item's placed orientation) and faces the
/// item's origin `[0, 0]`.
#[derive(Debug, Clone, Deserialize)]
pub struct AnchorSlotSpec {
    pub offset: [i32; 2],
    /// Role label (default `resident`). v1 doesn't bind professions, so this is
    /// informational until the workplace pass.
    #[serde(default = "default_role")]
    pub role: SlotRole,
    /// If false, this slot drops out individually when its cell isn't usable;
    /// if true (default), an unusable cell drops the whole scene.
    #[serde(default = "default_true")]
    pub required: bool,
    /// Per-slot dialogue key, overriding the scene's `dialogue`. `None` inherits
    /// the scene key.
    #[serde(default)]
    pub dialogue: Option<String>,
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
    /// If false, the block is NOT written to the world but still claims its
    /// cell (the cell must be open and ends up Blocked / UR per `walkable`).
    /// Use for cells that will be filled by Minecraft side-effects — e.g. a
    /// bed head auto-spawned by setPlacedBy when the foot is placed — so the
    /// constraint map reflects the real post-placement world without us
    /// double-placing the block.
    #[serde(default = "default_place")]
    pub place: bool,
    /// Name of a loot table in `data/furniture/loot.yaml`. When set, the
    /// furnisher rolls items from that table and writes them as the
    /// block entity's `Items` SNBT. Intended for chests, barrels,
    /// furnaces, and smokers.
    #[serde(default)]
    pub loot: Option<String>,
}

impl Default for FurnitureBlock {
    fn default() -> Self {
        Self {
            block: String::new(),
            offset: [0, 0, 0],
            layer: BlockLayer::default(),
            swap: PaletteSwap::default(),
            walkable: false,
            place: true,
            loot: None,
        }
    }
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

// ---------------------------------------------------------------------------
// Loot tables
// ---------------------------------------------------------------------------

/// A randomized container's contents. Two mutually-exclusive modes:
///   - `items` + `count` + `capacity` → roll N stacks into random slot indices
///     (chests, barrels).
///   - `fixed` → assign specific slots directly (furnaces: slot 0 input,
///     slot 1 fuel, slot 2 output).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LootTable {
    /// How many stacks to roll (inclusive range, e.g. [2, 6]). Only used
    /// with the random-slot strategy.
    #[serde(default)]
    pub count: Option<[i32; 2]>,
    /// Total slot count of the container — defaults to 27 (chest/barrel).
    /// Override to 3 for furnaces/smokers if using random strategy (unusual).
    #[serde(default)]
    pub capacity: Option<i32>,
    /// Weighted pool drawn from by the random-slot strategy.
    #[serde(default)]
    pub items: Vec<LootItem>,
    /// Fixed-slot entries. When non-empty, this table uses the fixed strategy
    /// and `count` / `items` are ignored.
    #[serde(default)]
    pub fixed: Vec<FixedSlot>,
}

/// One item in a weighted pool.
#[derive(Debug, Clone, Deserialize)]
pub struct LootItem {
    pub id: String,
    /// Stack size range (inclusive, e.g. [1, 6]).
    pub count: [i32; 2],
    #[serde(default = "default_weight")]
    pub weight: f32,
    /// Optional custom display name — sets the item's `minecraft:custom_name`
    /// component. Used for flavour on display shelves (e.g. a healing potion
    /// shown as "Wine"). Plain text only.
    #[serde(default)]
    pub name: Option<String>,
    /// Optional extra component entries inserted verbatim inside the item's
    /// `components:{…}` SNBT — e.g.
    /// `"minecraft:potion_contents":{potion:"minecraft:healing"}` to colour a
    /// potion. Author the inner entries only (no outer braces); combine several
    /// with commas.
    #[serde(default)]
    pub components: Option<String>,
}

/// A fixed slot assignment (furnace-style).
#[derive(Debug, Clone, Deserialize)]
pub struct FixedSlot {
    pub slot: i32,
    /// Probability (0..=1) that this slot is populated at all.
    #[serde(default = "default_chance")]
    pub chance: f32,
    pub items: Vec<LootItem>,
}

fn default_weight() -> f32 { 1.0 }
fn default_chance() -> f32 { 1.0 }
fn default_place() -> bool { true }
fn default_true() -> bool { true }
fn default_scene_kind() -> SceneKind { SceneKind::Solo }
fn default_role() -> SlotRole { SlotRole::Resident }
