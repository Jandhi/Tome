use serde_derive::{Serialize, Deserialize};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::minecraft::BlockID;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumIter)]
pub enum Color {
    #[serde(rename = "black")]
    Black,
    #[serde(rename = "dark_blue")]
    DarkBlue,
    #[serde(rename = "dark_green")]
    DarkGreen,
    #[serde(rename = "dark_aqua")]
    DarkAqua,
    #[serde(rename = "dark_red")]
    DarkRed,
    #[serde(rename = "dark_purple")]
    DarkPurple,
    #[serde(rename = "gold")]
    Gold,
    #[serde(rename = "gray")]
    Gray,
    #[serde(rename = "dark_gray")]
    DarkGray,
    #[serde(rename = "blue")]
    Blue,
    #[serde(rename = "green")]
    Green,
    #[serde(rename = "aqua")]
    Aqua,
    #[serde(rename = "red")]
    Red,
    #[serde(rename = "light_purple")]
    LightPurple,
    #[serde(rename = "yellow")]
    Yellow,
    #[serde(rename = "white")]
    White,
}

impl Into<String> for Color {
    fn into(self) -> String {
        match self {
            Color::Black => "black".to_string(),
            Color::DarkBlue => "dark_blue".to_string(),
            Color::DarkGreen => "dark_green".to_string(),
            Color::DarkAqua => "dark_aqua".to_string(),
            Color::DarkRed => "dark_red".to_string(),
            Color::DarkPurple => "dark_purple".to_string(),
            Color::Gold => "gold".to_string(),
            Color::Gray => "gray".to_string(),
            Color::DarkGray => "dark_gray".to_string(),
            Color::Blue => "blue".to_string(),
            Color::Green => "green".to_string(),
            Color::Aqua => "aqua".to_string(),
            Color::Red => "red".to_string(),
            Color::LightPurple => "light_purple".to_string(),
            Color::Yellow => "yellow".to_string(),
            Color::White => "white".to_string(),
        }
    }
}

fn color_block(block_id : BlockID, color : Color) -> BlockID {
    let block_id_str: String = serde_json::to_string(&block_id).unwrap();

    let swappable_strings = vec![
        "wool",
        "carpet",
        "stained_glass",
        "terracotta",
        "concrete",
        "shulker_box",
        "bed",
        "candle",
        "banner",
    ];

    if !swappable_strings.iter().any(|s| block_id_str.contains(s)) {
        return block_id; // No swappable strings found, return original block ID
    }

    for color in Color::iter() {
        let color_in: String = serde_json::to_string(&color).unwrap();
        let color_out : String = color.into();
        if block_id_str.contains(&color_in) {
            return serde_json::from_str(&block_id_str.replace(&color_in, &color_out)).unwrap();
        }
    }

    block_id // If no color match found, return original block ID
}