use std::collections::HashMap;
use crate::geometry::{Cardinal, Point3D};
use crate::editor::Editor;
use crate::generator::materials::{Material, MaterialId, MaterialRole, Palette};
use crate::minecraft::{Block, BlockForm, BlockID};
use crate::noise::RNG;
use super::footprint::Footprint;
use super::frame::Frame;

/// Type of roof geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoofType {
    /// Hip roof: slopes on all four sides, meeting at a peak or ridge.
    Hip,
    /// Gable roof: two slopes meeting at a ridge, with vertical gable walls on the ends.
    Gable,
}

/// Configuration for roof generation.
#[derive(Debug, Clone)]
pub struct RoofConfig {
    /// Roof pitch: rise per run (e.g., 0.5 = rise 1 block for every 2 horizontal blocks).
    pub pitch: f32,
    /// Horizontal overhang beyond walls in blocks.
    pub overhang: i32,
    /// Whether to use stair blocks for the roof surface.
    pub use_stairs: bool,
    /// Whether to use slab blocks for partial height sections.
    pub use_slabs: bool,
}

impl Default for RoofConfig {
    fn default() -> Self {
        Self {
            pitch: 0.5,
            overhang: 1,
            use_stairs: true,
            use_slabs: true,
        }
    }
}

/// A complete roof with type and configuration.
#[derive(Debug, Clone)]
pub struct Roof {
    pub roof_type: RoofType,
    pub base_y: i32,
    pub config: RoofConfig,
}

impl Roof {
    pub fn new(roof_type: RoofType, base_y: i32, config: RoofConfig) -> Self {
        Self {
            roof_type,
            base_y,
            config,
        }
    }

    pub fn hip(base_y: i32, config: RoofConfig) -> Self {
        Self::new(RoofType::Hip, base_y, config)
    }

    pub fn gable(base_y: i32, config: RoofConfig) -> Self {
        Self::new(RoofType::Gable, base_y, config)
    }
}

/// Place a gable roof on a rectangular footprint.
/// Slopes on the longer sides (assumed to be North/South), gable walls on shorter sides (East/West).
pub async fn place_gable_roof(
    roof: &Roof,
    footprint: &Footprint,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    let (bounds_min, bounds_max) = footprint.bounds().unwrap();
    let width = bounds_max.x - bounds_min.x + 1;
    let depth = bounds_max.y - bounds_min.y + 1; // Point2D.y is Z coordinate
    
    // Determine which direction to slope
    let slope_ns = depth >= width; // Slope North-South if depth >= width
    
    let overhang = roof.config.overhang;
    let pitch = roof.config.pitch;
    
    // Calculate roof dimensions with overhang
    let roof_min_x = bounds_min.x - overhang;
    let roof_max_x = bounds_max.x + overhang;
    let roof_min_z = bounds_min.y - overhang;  // Point2D.y is Z
    let roof_max_z = bounds_max.y + overhang;
    
    let roof_width = roof_max_x - roof_min_x + 1;
    let roof_depth = roof_max_z - roof_min_z + 1;
    
    // Get roof material blocks
    let roof_block_stairs = palette
        .get_block(MaterialRole::PrimaryRoof, &BlockForm::Stairs, materials, rng)
        .cloned()
        .unwrap_or_else(|| BlockID::from("oak_stairs"));
    let roof_block_solid = palette
        .get_block(MaterialRole::PrimaryRoof, &BlockForm::Block, materials, rng)
        .cloned()
        .unwrap_or_else(|| BlockID::from("oak_planks"));
    let wall_block_id = palette
        .get_block(MaterialRole::PrimaryWall, &BlockForm::Block, materials, rng)
        .cloned()
        .unwrap_or_else(|| BlockID::from("stone_bricks"));
    
    if slope_ns {
        // Slope North-South, gables on East-West
        let ridge_height = ((roof_width as f32) / 2.0 * pitch).ceil() as i32;
        let center_x = (roof_min_x + roof_max_x) / 2;
        
        for z in roof_min_z..=roof_max_z {
            for x in roof_min_x..=roof_max_x {
                let dist_from_center = (x - center_x).abs();
                let y_offset = ridge_height - (dist_from_center as f32 * pitch).ceil() as i32;
                
                if y_offset >= 0 {
                    let y = roof.base_y + y_offset;
                    let pos = Point3D::new(x, y, z);
                    
                    if roof.config.use_stairs && y_offset > 0 {
                        // Use stairs facing toward center
                        let facing = if x < center_x {
                            Cardinal::East
                        } else {
                            Cardinal::West
                        };
                        
                        let mut state = HashMap::new();
                        state.insert("facing".to_string(), facing.to_string());
                        let stair_block = Block::new(roof_block_stairs.clone(), Some(state), None);
                        editor.place_block(&stair_block, pos).await;
                    } else {
                        // Use solid block at ridge
                        let solid_block = Block::from(roof_block_solid.clone());
                        editor.place_block(&solid_block, pos).await;
                    }
                }
            }
        }
        
        // Fill gable walls (vertical triangular sections)
        let wall_block = Block::from(wall_block_id.clone());
        for x in roof_min_x..=roof_max_x {
            let dist_from_center = (x - center_x).abs();
            let height = ridge_height - (dist_from_center as f32 * pitch).ceil() as i32;
            
            if height > 0 {
                // Front gable (South - min Z)
                for y_offset in 0..height {
                    let y = roof.base_y + y_offset;
                    let pos = Point3D::new(x, y, roof_min_z);
                    editor.place_block(&wall_block, pos).await;
                }
                
                // Back gable (North - max Z)
                for y_offset in 0..height {
                    let y = roof.base_y + y_offset;
                    let pos = Point3D::new(x, y, roof_max_z);
                    editor.place_block(&wall_block, pos).await;
                }
            }
        }
    } else {
        // Slope East-West, gables on North-South
        let ridge_height = ((roof_depth as f32) / 2.0 * pitch).ceil() as i32;
        let center_z = (roof_min_z + roof_max_z) / 2;
        
        for x in roof_min_x..=roof_max_x {
            for z in roof_min_z..=roof_max_z {
                let dist_from_center = (z - center_z).abs();
                let y_offset = ridge_height - (dist_from_center as f32 * pitch).ceil() as i32;
                
                if y_offset >= 0 {
                    let y = roof.base_y + y_offset;
                    let pos = Point3D::new(x, y, z);
                    
                    if roof.config.use_stairs && y_offset > 0 {
                        // Use stairs facing toward center
                        let facing = if z < center_z {
                            Cardinal::South
                        } else {
                            Cardinal::North
                        };
                        
                        let mut state = HashMap::new();
                        state.insert("facing".to_string(), facing.to_string());
                        let stair_block = Block::new(roof_block_stairs.clone(), Some(state), None);
                        editor.place_block(&stair_block, pos).await;
                    } else {
                        // Use solid block at ridge
                        let solid_block = Block::from(roof_block_solid.clone());
                        editor.place_block(&solid_block, pos).await;
                    }
                }
            }
        }
        
        // Fill gable walls (vertical triangular sections)
        let wall_block = Block::from(wall_block_id.clone());
        for z in roof_min_z..=roof_max_z {
            let dist_from_center = (z - center_z).abs();
            let height = ridge_height - (dist_from_center as f32 * pitch).ceil() as i32;
            
            if height > 0 {
                // West gable (min X)
                for y_offset in 0..height {
                    let y = roof.base_y + y_offset;
                    let pos = Point3D::new(roof_min_x, y, z);
                    editor.place_block(&wall_block, pos).await;
                }
                
                // East gable (max X)
                for y_offset in 0..height {
                    let y = roof.base_y + y_offset;
                    let pos = Point3D::new(roof_max_x, y, z);
                    editor.place_block(&wall_block, pos).await;
                }
            }
        }
    }
}

/// Place a hip roof on a rectangular footprint.
/// Slopes on all four sides, meeting at a peak or ridge.
pub async fn place_hip_roof(
    roof: &Roof,
    footprint: &Footprint,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    let (bounds_min, bounds_max) = footprint.bounds().unwrap();
    let overhang = roof.config.overhang;
    let pitch = roof.config.pitch;
    
    // Calculate roof dimensions with overhang
    let roof_min_x = bounds_min.x - overhang;
    let roof_max_x = bounds_max.x + overhang;
    let roof_min_z = bounds_min.y - overhang;  // Point2D.y is Z
    let roof_max_z = bounds_max.y + overhang;
    
    // Get roof material blocks
    let roof_block_stairs = palette
        .get_block(MaterialRole::PrimaryRoof, &BlockForm::Stairs, materials, rng)
        .cloned()
        .unwrap_or_else(|| BlockID::from("oak_stairs"));
    let roof_block_solid = palette
        .get_block(MaterialRole::PrimaryRoof, &BlockForm::Block, materials, rng)
        .cloned()
        .unwrap_or_else(|| BlockID::from("oak_planks"));
    
    // For each position, calculate distance to nearest edge
    for z in roof_min_z..=roof_max_z {
        for x in roof_min_x..=roof_max_x {
            // Distance to each edge
            let dist_west = x - roof_min_x;
            let dist_east = roof_max_x - x;
            let dist_north = z - roof_min_z;
            let dist_south = roof_max_z - z;
            
            // Minimum distance to any edge determines the height
            let min_dist = dist_west.min(dist_east).min(dist_north).min(dist_south);
            let y_offset = (min_dist as f32 * pitch).ceil() as i32;
            let y = roof.base_y + y_offset;
            
            let pos = Point3D::new(x, y, z);
            
            if roof.config.use_stairs {
                // Determine which edge is closest and face opposite direction
                let facing = if min_dist == dist_west {
                    Cardinal::East
                } else if min_dist == dist_east {
                    Cardinal::West
                } else if min_dist == dist_north {
                    Cardinal::South
                } else {
                    Cardinal::North
                };
                
                let mut state = HashMap::new();
                state.insert("facing".to_string(), facing.to_string());
                let stair_block = Block::new(roof_block_stairs.clone(), Some(state), None);
                editor.place_block(&stair_block, pos).await;
            } else {
                // Use solid blocks
                let solid_block = Block::from(roof_block_solid.clone());
                editor.place_block(&solid_block, pos).await;
            }
        }
    }
}

/// Place a roof based on its type.
pub async fn place_roof(
    roof: &Roof,
    footprint: &Footprint,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    match roof.roof_type {
        RoofType::Hip => place_hip_roof(roof, footprint, editor, palette, materials, rng).await,
        RoofType::Gable => place_gable_roof(roof, footprint, editor, palette, materials, rng).await,
    }
}

/// Rules for automatic roof generation.
#[derive(Debug, Clone)]
pub struct RoofRules {
    /// Prefer hip roofs (true) or gable roofs (false).
    pub prefer_hip: bool,
    /// Aspect ratio threshold: if width/depth ratio > this, use gable roof.
    /// Default: 1.5 (buildings that are 1.5x longer than wide get gable roofs).
    pub gable_threshold: f32,
    /// Roof configuration.
    pub config: RoofConfig,
}

impl Default for RoofRules {
    fn default() -> Self {
        Self {
            prefer_hip: true,
            gable_threshold: 1.5,
            config: RoofConfig::default(),
        }
    }
}

/// Generate a roof for a frame based on rules.
pub fn generate_roof(frame: &Frame, rules: &RoofRules) -> Roof {
    let (bounds_min, bounds_max) = frame.footprint.bounds().unwrap();
    let width = (bounds_max.x - bounds_min.x + 1) as f32;
    let depth = (bounds_max.y - bounds_min.y + 1) as f32;  // Point2D.y is Z
    
    let aspect_ratio = if width > depth {
        width / depth
    } else {
        depth / width
    };
    
    let roof_type = if aspect_ratio > rules.gable_threshold {
        // Long rectangular building -> gable roof
        RoofType::Gable
    } else if rules.prefer_hip {
        // Square-ish building with hip preference -> hip roof
        RoofType::Hip
    } else {
        // Square-ish building without hip preference -> gable roof
        RoofType::Gable
    };
    
    let base_y = frame.base_y + (frame.wall_height * frame.floors as i32);
    
    Roof::new(roof_type, base_y, rules.config.clone())
}
