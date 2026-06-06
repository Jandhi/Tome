pub mod blueprint;
pub mod cellar;
pub mod door_ramp;
pub mod floors;
pub mod footprint;
pub mod foundation;
pub mod frame;
pub mod furnish;
pub mod pipeline;
pub mod roof;
pub mod rooms;
pub mod walls;

pub use pipeline::{BuildCtx, HouseOutput, build_house};
pub use self::walls::{TimberPattern, WindowFill};

use crate::generator::materials::PaletteId;
use footprint::SizeClass;
use roof::RoofStyle;
use roof::gable::GablePitch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildingType {
    House,
}

/// Cultural style that drives palette selection, roof/window/floor defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Culture {
    Medieval,
    Desert,
    Japanese,
}

impl Culture {
    /// Default palette ID for this culture.
    pub fn palette_id(&self) -> PaletteId {
        match self {
            Culture::Medieval => "medieval_spruce".into(),
            Culture::Desert => "desert_sandstone".into(),
            Culture::Japanese => "japanese_dark_blackstone".into(),
        }
    }

    /// Roof styles to pick from for this culture.
    pub fn roof_styles(&self) -> Vec<RoofStyle> {
        match self {
            Culture::Medieval => vec![
                RoofStyle::Gable(GablePitch::Slab),
                RoofStyle::Gable(GablePitch::Stairs),
                RoofStyle::Gable(GablePitch::Double),
            ],
            Culture::Desert => vec![RoofStyle::Flat],
            Culture::Japanese => vec![
                RoofStyle::Gable(GablePitch::Stairs),
                RoofStyle::Gable(GablePitch::Double),
            ],
        }
    }

    /// Window fill style for this culture.
    pub fn window_fill(&self) -> WindowFill {
        match self {
            Culture::Desert => WindowFill::Open,
            _ => WindowFill::Glass,
        }
    }

    /// Probability (num, denom) that a multi-floor building of this culture
    /// jetties its upper floor over the ground. Eligibility (shape, plot
    /// bounds, floor count) is enforced at frame generation; this is just the
    /// cultural taste filter. Medieval timber-frame jetties are the iconic case.
    pub fn jetty_chance(&self) -> (u32, u32) {
        match self {
            Culture::Medieval => (2, 3),
            Culture::Desert => (0, 1),
            Culture::Japanese => (0, 1),
        }
    }
}

/// Per-building context threaded through the pipeline. Bundles culture, size,
/// and any per-building overrides so downstream code can make style decisions
/// without a growing parameter list.
pub struct BuildingContext {
    pub culture: Culture,
    pub size_class: SizeClass,
    pub roof_style: RoofStyle,
    pub window_fill: WindowFill,
    /// Per-building timber override. `None` means `build_house` auto-rolls one
    /// once the frame is known (so it can filter to patterns that actually fit
    /// the longest wall). Set to `Some(...)` to pin a pattern for tests/debug.
    pub timber_pattern: Option<TimberPattern>,
    /// Whether to jetty upper floors over the ground (each upper rect grows by
    /// 1 on each side, where the plot allows). Frame generation enforces the
    /// shape/floor/plot eligibility gates and silently no-ops when ineligible,
    /// so it's safe to set this true unconditionally for testing.
    pub jetty: bool,
}

impl BuildingContext {
    /// Create a context with culture defaults for roof and window style.
    /// Timber pattern is left unset (auto-pick during `build_house`).
    /// Jetty is left off; callers wanting it should set the field after.
    pub fn new(culture: Culture, size_class: SizeClass, roof_style: RoofStyle) -> Self {
        Self {
            culture,
            size_class,
            roof_style,
            window_fill: culture.window_fill(),
            timber_pattern: None,
            jetty: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomType {
    /// Single-room house: bed, furnace, crafting table, chest.
    Common,
    /// Ground floor main room in multi-room houses: living + kitchen.
    Hearth,
    /// Large living/dining area. Ground floor core in halls.
    GreatRoom,
    /// Sleeping quarters on upper floors.
    Bedroom,
    /// Upper core subdivided into hallway + smaller bedrooms.
    MultiBedroom,
    /// Larger private bedroom in a wing.
    MasterBedroom,
    /// Bookshelves, desk, enchanting table.
    Study,
    /// Chests, barrels — filler for extra rooms.
    Storage,
    /// Long table, chairs, candles. Ground floor.
    Dining,
    /// Cooking: furnaces, smoker, cauldron. Ground floor wing.
    Kitchen,
    /// Food storage: barrels, chests, hay bales. Ground floor wing.
    Pantry,
    /// Bookshelves lining walls, lectern, enchanting table. Upper floor, Manor+.
    Library,
    /// Loom, glazed terracotta, flower pots, colored wool. Upper floor, Manor+.
    Studio,
    /// Armor stands, item frames, anvil. Upper floor, Manor+.
    Armory,
}

/// Optional custom floor style for a room, overriding the default palette floor.
/// The actual blocks placed depend on biome/palette context at placement time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloorType {
    /// Kitchen floor — resolved per biome (e.g. glazed terracotta in desert,
    /// stone bricks in temperate climates).
    Kitchen,
}

impl RoomType {
    /// Returns (display name, short label, furniture data key).
    fn metadata(&self) -> (&'static str, &'static str, &'static str) {
        match self {
            RoomType::Common        => ("Common",     "Com", "common"),
            RoomType::Hearth        => ("Hearth",     "Hrt", "hearth"),
            RoomType::GreatRoom     => ("Great Room", "Grt", "great_room"),
            RoomType::Bedroom       => ("Bedroom",    "Bed", "bedroom"),
            RoomType::MultiBedroom  => ("Bedrooms",   "MBd", "multi_bedroom"),
            RoomType::MasterBedroom => ("Master Bed", "Mst", "master_bedroom"),
            RoomType::Study         => ("Study",      "Std", "study"),
            RoomType::Storage       => ("Storage",    "Sto", "storage"),
            RoomType::Dining        => ("Dining",     "Din", "dining"),
            RoomType::Kitchen       => ("Kitchen",    "Kit", "kitchen"),
            RoomType::Pantry        => ("Pantry",     "Pnt", "pantry"),
            RoomType::Library       => ("Library",    "Lib", "library"),
            RoomType::Studio        => ("Studio",     "Art", "studio"),
            RoomType::Armory        => ("Armory",     "Arm", "armory"),
        }
    }

    pub fn name(&self) -> &'static str { self.metadata().0 }

    /// Short label for ASCII diagrams.
    pub fn label(&self) -> &'static str { self.metadata().1 }

    /// Key for looking up furniture data.
    pub fn furniture_key(&self) -> &'static str { self.metadata().2 }
}
