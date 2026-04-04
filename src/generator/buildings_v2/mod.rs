pub mod floors;
pub mod footprint;
pub mod foundation;
pub mod frame;
pub mod furnish;
pub mod roof;
pub mod rooms;
pub mod walls;

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
    /// Short label for ASCII diagrams.
    pub fn label(&self) -> &'static str {
        match self {
            RoomType::Common => "Com",
            RoomType::Hearth => "Hrt",
            RoomType::GreatRoom => "Grt",
            RoomType::Bedroom => "Bed",
            RoomType::MultiBedroom => "MBd",
            RoomType::MasterBedroom => "Mst",
            RoomType::Study => "Std",
            RoomType::Storage => "Sto",
            RoomType::Dining => "Din",
            RoomType::Kitchen => "Kit",
            RoomType::Pantry => "Pnt",
            RoomType::Library => "Lib",
            RoomType::Studio => "Art",
            RoomType::Armory => "Arm",
        }
    }
}
