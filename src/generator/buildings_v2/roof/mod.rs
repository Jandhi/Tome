mod hip;
mod gable;
mod x_decoration;
mod overshoot;

use std::collections::HashMap;

use crate::editor::Editor;
use crate::generator::materials::{Material, MaterialId, MaterialRole, Palette};
use crate::minecraft::{BlockForm, BlockID};
use crate::noise::RNG;

use super::footprint::Footprint;
use super::frame::Frame;

pub use hip::place_hip_roof;
pub use gable::{place_gable_roof, place_gable_walls, place_gable_decorations};

/// Type of roof geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoofType {
    /// Hip roof: slopes on all four sides, meeting at a peak or ridge.
    Hip,
    /// Gable roof: two slopes meeting at a ridge, with vertical gable walls on the ends.
    Gable,
}

/// Roof pitch determines the slope steepness and block types used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoofPitch {
    /// Shallow (1/2 block rise per run): uses slabs, alternating bottom/top placement.
    Shallow,
    /// Medium (1 block rise per run): uses stairs, +1 y per row.
    Medium,
    /// Steep (2 block rise per run): uses block + stair per row, +2 y per row.
    Steep,
}

/// Decorative elements at gable ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GableDecoration {
    /// No decoration at gable ends.
    #[default]
    None,
    /// X-shaped decoration at the peak (Viking longhouse style).
    /// Uses slabs for even-width roofs, stairs for odd-width roofs.
    X,
    /// Overshooting element extending past the gable wall (inverted stair + stair).
    Overshoot,
}

/// Configuration for gable roofs.
#[derive(Debug, Clone)]
pub struct GableConfig {
    /// Roof pitch determines slope steepness and block types.
    pub pitch: RoofPitch,
    /// Horizontal overhang beyond walls in blocks.
    pub overhang: i32,
    /// Optional decoration at gable ends.
    pub decoration: GableDecoration,
}

impl Default for GableConfig {
    fn default() -> Self {
        Self {
            pitch: RoofPitch::Medium,
            overhang: 1,
            decoration: GableDecoration::None,
        }
    }
}

/// Configuration for hip roofs.
#[derive(Debug, Clone)]
pub struct HipConfig {
    /// Roof pitch determines slope steepness and block types.
    pub pitch: RoofPitch,
    /// Horizontal overhang beyond walls in blocks.
    pub overhang: i32,
}

impl Default for HipConfig {
    fn default() -> Self {
        Self {
            pitch: RoofPitch::Medium,
            overhang: 1,
        }
    }
}

/// Type-specific roof configuration.
#[derive(Debug, Clone)]
pub enum RoofConfig {
    Gable(GableConfig),
    Hip(HipConfig),
}

impl RoofConfig {
    pub fn pitch(&self) -> RoofPitch {
        match self {
            RoofConfig::Gable(c) => c.pitch,
            RoofConfig::Hip(c) => c.pitch,
        }
    }

    pub fn overhang(&self) -> i32 {
        match self {
            RoofConfig::Gable(c) => c.overhang,
            RoofConfig::Hip(c) => c.overhang,
        }
    }
}

/// A complete roof with type and configuration.
#[derive(Debug, Clone)]
pub struct Roof {
    pub base_y: i32,
    pub config: RoofConfig,
}

impl Roof {
    pub fn roof_type(&self) -> RoofType {
        match &self.config {
            RoofConfig::Gable(_) => RoofType::Gable,
            RoofConfig::Hip(_) => RoofType::Hip,
        }
    }

    pub fn hip(base_y: i32, config: HipConfig) -> Self {
        Self {
            base_y,
            config: RoofConfig::Hip(config),
        }
    }

    pub fn gable(base_y: i32, config: GableConfig) -> Self {
        Self {
            base_y,
            config: RoofConfig::Gable(config),
        }
    }
}

/// Rules for automatic roof generation.
#[derive(Debug, Clone)]
pub struct RoofRules {
    /// Preferred roof type for square-ish buildings.
    pub preferred_type: RoofType,
    /// Aspect ratio threshold: if width/depth ratio > this, use gable roof.
    /// Default: 1.5 (buildings that are 1.5x longer than wide get gable roofs).
    pub gable_threshold: f32,
    /// Configuration for gable roofs.
    pub gable: GableConfig,
    /// Configuration for hip roofs.
    pub hip: HipConfig,
}

impl Default for RoofRules {
    fn default() -> Self {
        Self {
            preferred_type: RoofType::Hip,
            gable_threshold: 1.5,
            gable: GableConfig::default(),
            hip: HipConfig::default(),
        }
    }
}

/// Generate a roof for a frame based on rules.
pub fn generate_roof(frame: &Frame, rules: &RoofRules) -> Roof {
    let (bounds_min, bounds_max) = frame.footprint.bounds().unwrap();
    let width = (bounds_max.x - bounds_min.x + 1) as f32;
    let depth = (bounds_max.y - bounds_min.y + 1) as f32; // Point2D.y is Z

    let aspect_ratio = if width > depth {
        width / depth
    } else {
        depth / width
    };

    // Long rectangular buildings always get gable roofs (ridge along longest axis)
    // Square-ish buildings use the preferred type
    let roof_type = if aspect_ratio > rules.gable_threshold {
        RoofType::Gable
    } else {
        rules.preferred_type
    };

    let base_y = frame.roof_base_y() - 1;

    match roof_type {
        RoofType::Gable => Roof::gable(base_y, rules.gable.clone()),
        RoofType::Hip => Roof::hip(base_y, rules.hip.clone()),
    }
}

/// Place a roof based on its type.
/// For gable roofs, call place_gable_walls BEFORE this function to ensure
/// walls are placed first and roof tiles can cut into them.
pub async fn place_roof(
    roof: &Roof,
    footprint: &Footprint,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    match roof.roof_type() {
        RoofType::Hip => place_hip_roof(roof, footprint, editor, palette, materials, rng).await,
        RoofType::Gable => place_gable_roof(roof, footprint, editor, palette, materials, rng).await,
    }
}

/// Helper to get roof material blocks from palette.
pub(crate) struct RoofMaterials {
    pub stairs: BlockID,
    pub solid: BlockID,
    pub slab: BlockID,
}

impl RoofMaterials {
    pub fn from_palette(
        palette: &Palette,
        materials: &HashMap<MaterialId, Material>,
        rng: &mut RNG,
    ) -> Self {
        let stairs = palette
            .get_block(MaterialRole::PrimaryRoof, &BlockForm::Stairs, materials, rng)
            .cloned()
            .unwrap_or_else(|| BlockID::from("oak_stairs"));
        let solid = palette
            .get_block(MaterialRole::PrimaryRoof, &BlockForm::Block, materials, rng)
            .cloned()
            .unwrap_or_else(|| BlockID::from("oak_planks"));
        let slab = palette
            .get_block(MaterialRole::PrimaryRoof, &BlockForm::Slab, materials, rng)
            .cloned()
            .unwrap_or_else(|| BlockID::from("oak_slab"));

        Self { stairs, solid, slab }
    }
}
