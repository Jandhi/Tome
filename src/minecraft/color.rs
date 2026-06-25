use serde_derive::{Serialize, Deserialize};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::minecraft::BlockID;

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

impl Color {
    /// An English surname root for this colour, or `None` for colours that don't
    /// read as a family name. Combined with a place-name suffix (`-well`,
    /// `-wood`, …) to mint a colour-derived house name — `Black` + `well` =
    /// `Blackwell`. Used to tie a manor's family name to its family colour.
    pub fn surname_root(self) -> Option<&'static str> {
        match self {
            Color::Black => Some("Black"),
            Color::White => Some("White"),
            Color::Gray | Color::LightGray => Some("Grey"),
            Color::Green => Some("Green"),
            Color::Brown => Some("Brown"),
            Color::Red => Some("Red"),
            Color::Yellow => Some("Gold"),
            Color::Blue => Some("Blue"),
            _ => None,
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
pub fn recolor_block(block_id: &BlockID, old_color: Color, new_color: Color) -> BlockID {
    let block_id_str = block_id.as_str();

    if !SWAPPABLE_STRINGS.iter().any(|s| block_id_str.contains(s)) {
        return block_id.clone();
    }

    let old_color_str: String = old_color.into();
    let new_color_str: String = new_color.into();

    // Don't replace light colors with dark colors (substring overlap)
    if old_color_str == "blue" && block_id_str.contains("light_blue") {
        return block_id.clone();
    }
    if old_color_str == "gray" && block_id_str.contains("light_gray") {
        return block_id.clone();
    }

    // Match `<color>_` so e.g. `red_bed` matches but `cured_block` doesn't.
    let pattern = format!("{}_", old_color_str);
    if block_id_str.contains(&pattern) {
        let replacement = format!("{}_", new_color_str);
        return BlockID::from(block_id_str.replace(&pattern, &replacement).as_str());
    }

    block_id.clone()
}

pub fn color_block(block_id: BlockID, new_color: Color) -> BlockID {
    let block_id_str = block_id.as_str();

    if !SWAPPABLE_STRINGS.iter().any(|s| block_id_str.contains(s)) {
        return block_id;
    }

    let new_color_str: String = new_color.into();
    // Iterate longest-first so `light_blue` / `light_gray` match before `blue` / `gray`.
    let mut colors: Vec<Color> = Color::iter().collect();
    colors.sort_by_key(|c| std::cmp::Reverse({
        let s: String = (*c).into();
        s.len()
    }));

    for color in colors {
        let color_str: String = color.into();
        let pattern = format!("{}_", color_str);
        if block_id_str.contains(&pattern) {
            let replacement = format!("{}_", new_color_str);
            return BlockID::from(block_id_str.replace(&pattern, &replacement).as_str());
        }
    }

    block_id
}
