use strum_macros::EnumIter;
use serde_derive::{Serialize, Deserialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum MaterialRole {
    #[serde(rename = "accent")]
    Accent,

    #[serde(rename = "primary_wall")]
    PrimaryWall,
    #[serde(rename = "secondary_wall")]
    SecondaryWall,

    #[serde(rename = "primary_roof")]
    PrimaryRoof,
    #[serde(rename = "secondary_roof")]
    SecondaryRoof,

    #[serde(rename = "wood_pillar")]
    WoodPillar,
    #[serde(rename = "stone_pillar")]
    StonePillar,

    #[serde(rename = "primary_stone")]
    PrimaryStone,
    #[serde(rename = "secondary_stone")]
    SecondaryStone,
    #[serde(rename = "primary_wood")]
    PrimaryWood,
    #[serde(rename = "secondary_wood")]
    SecondaryWood,
<<<<<<< HEAD
=======

    #[serde(rename = "flower")]
    Flower,
>>>>>>> master
}

impl MaterialRole {
    pub fn backup_role(&self) -> MaterialRole {
        match self {
            MaterialRole::SecondaryStone => MaterialRole::PrimaryStone,
            MaterialRole::SecondaryWood => MaterialRole::PrimaryWood,
            MaterialRole::Accent => MaterialRole::PrimaryStone,
            
            MaterialRole::PrimaryWall => MaterialRole::PrimaryStone,
            MaterialRole::SecondaryWall => MaterialRole::SecondaryStone,
            
            MaterialRole::WoodPillar => MaterialRole::PrimaryWood,
            MaterialRole::StonePillar => MaterialRole::PrimaryStone,
            
            MaterialRole::PrimaryRoof => MaterialRole::PrimaryStone,
            MaterialRole::SecondaryRoof => MaterialRole::SecondaryStone,
            
            _ => *self, // PrimaryStone remains unchanged
        }
    }
}