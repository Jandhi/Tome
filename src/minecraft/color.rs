use log::info;
use serde_derive::{Serialize, Deserialize};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::minecraft::{block, BlockID};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumIter)]
pub enum Color {
    #[serde(rename = "black")]
    Black,
    #[serde(rename = "gray")]
    Gray,
    #[serde(rename = "light_gray")]
    LightGray,
    #[serde(rename = "blue")]
    Blue,
    #[serde(rename = "cyan")]
    Cyan,
    #[serde(rename = "green")]
    Green,
    #[serde(rename = "red")]
    Red,
    #[serde(rename = "purple")]
    Purple,
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
            Color::Gray => "gray".to_string(),
            Color::LightGray => "light_gray".to_string(),
            Color::Blue => "blue".to_string(),
            Color::Cyan => "cyan".to_string(),
            Color::Green => "green".to_string(),
            Color::Red => "red".to_string(),
            Color::Purple => "purple".to_string(),
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

    // Don't replace light colors with dark colors
    if old_color_str == "blue" && block_id_str.contains("light_blue") {
        return block_id;
    }
    if old_color_str == "gray" && block_id_str.contains("light_gray") {
        return block_id;
    }

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
