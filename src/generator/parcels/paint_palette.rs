use std::collections::HashMap;

use serde::Deserialize;

use crate::minecraft::{Block, string_to_block};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub struct PaintPaletteId(pub String);

#[derive(Debug, Clone, Deserialize)]
pub struct PaintPalette {
    pub palette: HashMap<String, f32>,
    #[serde(default)]
    pub smooth: bool,
    pub tags: Option<Vec<String>>,
}

impl PaintPalette {
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.as_ref().map(|t| t.iter().any(|s| s == tag)).unwrap_or(false)
    }

    /// Converts to the (block_dict, block_list) form expected by `replace_ground`.
    /// block_dict maps index into block_list to relative weight.
    pub fn to_weighted_blocks(&self) -> (HashMap<usize, f32>, Vec<Block>) {
        let mut block_list: Vec<Block> = Vec::new();
        let mut block_dict: HashMap<usize, f32> = HashMap::new();
        for (block_str, &weight) in &self.palette {
            let block = string_to_block(block_str)
                .unwrap_or_else(|| block_str.as_str().into());
            let idx = block_list.len();
            block_dict.insert(idx, weight);
            block_list.push(block);
        }
        (block_dict, block_list)
    }
}

/// Top-level wrapper matching the YAML root key.
#[derive(Debug, Deserialize)]
pub struct PaintPalettesFile {
    pub paint_palettes: HashMap<String, PaintPalette>,
}
