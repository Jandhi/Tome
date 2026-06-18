use std::collections::HashMap;

use serde::{de::DeserializeOwned, Deserialize};

/// How a production area is painted. The YAML `type` field selects the kind:
/// - `palettes` — the built-in field/ground painter (crops, irrigation, border).
/// - `function` — dispatch to a named painter function plus optional params.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProductionPainter {
    Palettes {
        palettes: Vec<String>,
        /// Palette painted on the 3-block border strip around the parcel edge, if any.
        #[serde(default)]
        border_palette: Option<String>,
        #[serde(default)]
        irrigation: bool,
        /// 0.0 = no smoothing, 1.0 = 5 smoothing passes via smooth_terrain.
        #[serde(default)]
        flatten_strength: f32,
    },
    /// Dispatch to a named painter function (resolved in `production_area.rs`),
    /// passing it the free-form `params` map. Add a new function painter by
    /// writing the function, registering it in the dispatch, and referencing it
    /// here by name — no change to this type.
    Function {
        /// Name of the painter function to run (e.g. `logging_production_painter`).
        function: String,
        /// Optional parameters, deserialized by the function via [`parse_params`].
        #[serde(default)]
        params: serde_yaml::Value,
    },
}

impl ProductionPainter {
    /// Whether this painter lays a border ring — the `rural_road` strip painted
    /// around the production area's edge buffer. The rural road network predicts
    /// and reuses that ring, so it needs to know which areas will have one.
    ///
    /// `palettes` painters have a ring iff they declare a `border_palette`; the
    /// `pasture` and `sugarcane` function painters always paint one (their
    /// `border_palette` param defaults to `rural_road`). Other function painters
    /// (logging, bees, mine) lay no border.
    pub fn paints_border(&self) -> bool {
        match self {
            ProductionPainter::Palettes { border_palette, .. } => border_palette.is_some(),
            ProductionPainter::Function { function, .. } => matches!(
                function.as_str(),
                "pasture_production_painter" | "sugarcane_production_painter"
            ),
        }
    }
}

/// Deserialize a painter's free-form `params` value into a function-specific
/// struct. Optional fields rely on the target's `#[serde(default)]`. A null
/// (absent) params value deserializes to the target's `Default` where derived.
pub fn parse_params<T: DeserializeOwned>(params: &serde_yaml::Value) -> anyhow::Result<T> {
    Ok(serde_yaml::from_value(params.clone())?)
}

/// Top-level wrapper matching the YAML root key.
#[derive(Debug, Deserialize)]
pub struct ProductionPaintersFile {
    pub production_painters: HashMap<String, ProductionPainter>,
}

/// Top-level wrapper for `animal_names.yaml` — names randomly assigned to animals
/// spawned by the pasture/ranch painter, plus optional decorative prefixes/suffixes.
#[derive(Debug, Deserialize)]
pub struct AnimalNamesFile {
    pub animal_names: Vec<String>,
    /// e.g. "Ol'", "Sir" — prepended to a name ~10% of the time.
    #[serde(default)]
    pub name_prefixes: Vec<String>,
    /// e.g. "the Great", "Jr." — appended to a name ~10% of the time.
    #[serde(default)]
    pub name_suffixes: Vec<String>,
    /// Funny names for bees placed inside beehives by the bee_area painter.
    #[serde(default)]
    pub bee_names: Vec<String>,
}
