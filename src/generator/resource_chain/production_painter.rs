use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProductionPainter {
    Palettes {
        palettes: Vec<String>,
        /// Palette painted on the 3-block border strip around the district edge, if any.
        #[serde(default)]
        border_palette: Option<String>,
        #[serde(default)]
        irrigation: bool,
        /// 0.0 = no smoothing, 1.0 = 5 smoothing passes via smooth_terrain.
        #[serde(default)]
        flatten_strength: f32,
    },
    Logging {
        /// Fraction of tree-topped cells to fell, 0.0–1.0.
        percent: f32,
    },
}

/// Top-level wrapper matching the YAML root key.
#[derive(Debug, Deserialize)]
pub struct ProductionPaintersFile {
    pub production_painters: HashMap<String, ProductionPainter>,
}
