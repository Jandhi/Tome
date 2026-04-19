pub mod blueprint;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildingType {
    House,
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
