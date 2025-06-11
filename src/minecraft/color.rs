use log::info;
use serde_derive::{Serialize, Deserialize};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::minecraft::BlockID;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumIter)]
pub enum Color {
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
    #[serde(rename = "orange")]
    Orange,
    #[serde(rename = "magenta")]
    Magenta,
    #[serde(rename = "light_blue")]
    LightBlue,
    #[serde(rename = "lime")]
    Lime,
    #[serde(rename = "pink")]
    Pink,
    #[serde(rename = "brown")]
    Brown,
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
            Color::Orange => "orange".to_string(),
            Color::Magenta => "magenta".to_string(),
            Color::LightBlue => "light_blue".to_string(),
            Color::Lime => "lime".to_string(),
            Color::Pink => "pink".to_string(),
            Color::Brown => "brown".to_string(),
        }
    }
}

const SWAPPABLE_STRINGS: &[&str] = &[
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

// Only colors a block if it was the old color
pub fn recolor_block(block_id: BlockID, old_color: Color, new_color: Color) -> BlockID {
    let block_id_str: String = serde_json::to_string(&block_id).expect("Failed to serialize block ID");
    
    if !SWAPPABLE_STRINGS.iter().any(|s| block_id_str.contains(s)) {
        return block_id; // No swappable strings found, return original block ID
    }

    
    let old_color_str: String = old_color.into();
    let new_color_str: String = new_color.into();

    if block_id_str.contains(&old_color_str) {
        return serde_json::from_str(&block_id_str.replace(&old_color_str, &new_color_str))
            .expect("Failed to replace color in block ID");
    }

    block_id // If no color match found, return original block ID
}

pub fn color_block(block_id: BlockID, color: Color) -> BlockID {
    let block_id_str: String = serde_json::to_string(&block_id).expect("Failed to serialize block ID");

    if !SWAPPABLE_STRINGS.iter().any(|s| block_id_str.contains(s)) {
        return block_id; // No swappable strings found, return original block ID
    }

    for color in Color::iter() {
        let color_in: String = serde_json::to_string(&color).expect("Failed to serialize color");
        let color_out: String = color.into();
        if block_id_str.contains(&color_in) {
            return serde_json::from_str(&block_id_str.replace(&color_in, &color_out))
                .expect("Failed to replace color in block ID");
        }
    }

    block_id // If no color match found, return original block ID
}
