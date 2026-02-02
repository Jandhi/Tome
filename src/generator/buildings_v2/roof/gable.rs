use std::collections::HashMap;

use crate::editor::Editor;
use crate::generator::materials::{Material, MaterialId, Palette};
use crate::geometry::{Cardinal, Point3D};
use crate::minecraft::{Block, BlockID};
use crate::noise::RNG;

use super::super::footprint::Footprint;
use super::super::placement::{WallMaterials, place_gable_wall_block};
use super::{GableConfig, GableDecoration, Roof, RoofConfig, RoofMaterials, RoofPitch};
use super::x_decoration::place_x_decoration;
use super::overshoot::place_overshoot_decoration;

/// Place a gable roof on a rectangular footprint.
/// Ridge runs along the longest axis. Slopes perpendicular to the ridge.
pub async fn place_gable_roof(
    roof: &Roof,
    footprint: &Footprint,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    let config = match &roof.config {
        RoofConfig::Gable(c) => c,
        _ => return, // Not a gable roof
    };

    let (bounds_min, bounds_max) = footprint.bounds().unwrap();
    let width = bounds_max.x - bounds_min.x + 1;
    let depth = bounds_max.y - bounds_min.y + 1; // Point2D.y is Z coordinate

    // Ridge runs along the longest axis
    // If depth >= width, ridge runs along Z (north-south), slopes along X (east-west)
    let ridge_along_z = depth >= width;

    let overhang = config.overhang;

    // Calculate roof dimensions with overhang
    let roof_min_x = bounds_min.x - overhang;
    let roof_max_x = bounds_max.x + overhang;
    let roof_min_z = bounds_min.y - overhang; // Point2D.y is Z
    let roof_max_z = bounds_max.y + overhang;

    let roof_mats = RoofMaterials::from_palette(palette, materials, rng);

    // Determine slope dimension (perpendicular to ridge)
    let (slope_min, slope_max, ridge_min, ridge_max) = if ridge_along_z {
        (roof_min_x, roof_max_x, roof_min_z, roof_max_z)
    } else {
        (roof_min_z, roof_max_z, roof_min_x, roof_max_x)
    };

    let slope_span = slope_max - slope_min + 1;
    let half_span = slope_span / 2;
    // Only odd spans have a true center ridge that needs a slab cap
    let needs_ridge_cap = slope_span % 2 == 1;

    // Build roof row by row from each edge toward center
    let max_row = if needs_ridge_cap { half_span } else { half_span - 1 };
    for row in 0..=max_row {
        let (y_offset, blocks_for_row) = get_row_blocks(
            row,
            half_span,
            config.pitch,
            needs_ridge_cap,
            &roof_mats.slab,
            &roof_mats.stairs,
            &roof_mats.solid,
        );

        let y = roof.base_y + y_offset;

        // Place blocks along both sides of the roof
        for ridge_pos in ridge_min..=ridge_max {
            for entry in &blocks_for_row {
                let block_y = y + entry.y_adjust;

                // West/South side (slope_min + row)
                let slope_pos_low = slope_min + row;
                if slope_pos_low <= slope_min + half_span {
                    let pos = if ridge_along_z {
                        Point3D::new(slope_pos_low, block_y, ridge_pos)
                    } else {
                        Point3D::new(ridge_pos, block_y, slope_pos_low)
                    };

                    let placed_block = if entry.is_stair {
                        let facing = if entry.invert_facing {
                            if ridge_along_z { Cardinal::West } else { Cardinal::North }
                        } else {
                            if ridge_along_z { Cardinal::East } else { Cardinal::South }
                        };
                        let mut state = entry.base_state.clone().unwrap_or_default();
                        state.insert("facing".to_string(), facing.to_string());
                        Block::new(entry.block_id.clone(), Some(state), None)
                    } else {
                        Block::new(entry.block_id.clone(), entry.base_state.clone(), None)
                    };
                    editor.place_block(&placed_block, pos).await;
                }

                // East/North side (slope_max - row), skip if same as low side (center)
                let slope_pos_high = slope_max - row;
                if slope_pos_high > slope_pos_low {
                    let pos = if ridge_along_z {
                        Point3D::new(slope_pos_high, block_y, ridge_pos)
                    } else {
                        Point3D::new(ridge_pos, block_y, slope_pos_high)
                    };

                    let placed_block = if entry.is_stair {
                        let facing = if entry.invert_facing {
                            if ridge_along_z { Cardinal::East } else { Cardinal::South }
                        } else {
                            if ridge_along_z { Cardinal::West } else { Cardinal::North }
                        };
                        let mut state = entry.base_state.clone().unwrap_or_default();
                        state.insert("facing".to_string(), facing.to_string());
                        Block::new(entry.block_id.clone(), Some(state), None)
                    } else {
                        Block::new(entry.block_id.clone(), entry.base_state.clone(), None)
                    };
                    editor.place_block(&placed_block, pos).await;
                }
            }
        }
    }

    // Decorate roof underside
    place_roof_underside(
        config, roof.base_y, ridge_along_z, slope_min, slope_max, ridge_min, ridge_max,
        half_span, needs_ridge_cap, &roof_mats, editor,
    ).await;
}

/// Place decorative underside blocks for a finished interior look.
async fn place_roof_underside(
    config: &GableConfig,
    base_y: i32,
    ridge_along_z: bool,
    slope_min: i32,
    slope_max: i32,
    ridge_min: i32,
    ridge_max: i32,
    half_span: i32,
    needs_ridge_cap: bool,
    mats: &RoofMaterials,
    editor: &Editor,
) {
    let all_ridge_positions: Vec<i32> = (ridge_min..=ridge_max).collect();

    // Place decorative upside-down stairs/slabs along the entire roof underside
    // Skip row 0 (the lowest edge of the roof) - start from row 1
    for row in 1..half_span {
        let y_offset = match config.pitch {
            RoofPitch::Shallow => row / 2,
            RoofPitch::Medium => row,
            RoofPitch::Steep => row * 2,
        };

        let y = base_y + y_offset - 1;
        let slope_pos_low = slope_min + row;
        let slope_pos_high = slope_max - row;

        match config.pitch {
            RoofPitch::Shallow => {
                place_shallow_underside_row(
                    base_y, ridge_along_z, slope_pos_low, slope_pos_high, row,
                    &all_ridge_positions, mats, editor,
                ).await;
            }
            RoofPitch::Medium | RoofPitch::Steep => {
                place_stair_underside_row(
                    ridge_along_z, slope_pos_low, slope_pos_high, y,
                    &all_ridge_positions, mats, editor,
                ).await;
            }
        }
    }

    // Add decorative block below the ridge slab (if there's a ridge cap)
    if needs_ridge_cap {
        place_ridge_underside(
            config, base_y, ridge_along_z, slope_min, half_span,
            &all_ridge_positions, mats, editor,
        ).await;
    }
}

async fn place_shallow_underside_row(
    base_y: i32,
    ridge_along_z: bool,
    slope_pos_low: i32,
    slope_pos_high: i32,
    row: i32,
    ridge_positions: &[i32],
    mats: &RoofMaterials,
    editor: &Editor,
) {
    let is_top_slab_row = row % 2 == 1;

    if is_top_slab_row {
        // Replace the top slab with a full block at the roof level
        let block_y = base_y + row / 2;

        for &ridge_pos in ridge_positions {
            let pos_low = if ridge_along_z {
                Point3D::new(slope_pos_low, block_y, ridge_pos)
            } else {
                Point3D::new(ridge_pos, block_y, slope_pos_low)
            };
            let block = Block::from(mats.solid.clone());
            editor.place_block(&block, pos_low).await;

            if slope_pos_high > slope_pos_low {
                let pos_high = if ridge_along_z {
                    Point3D::new(slope_pos_high, block_y, ridge_pos)
                } else {
                    Point3D::new(ridge_pos, block_y, slope_pos_high)
                };
                editor.place_block(&block, pos_high).await;
            }
        }
    } else {
        // Place top slabs below the bottom slab
        let y = base_y + row / 2 - 1;
        let mut slab_state = HashMap::new();
        slab_state.insert("type".to_string(), "top".to_string());

        for &ridge_pos in ridge_positions {
            let pos_low = if ridge_along_z {
                Point3D::new(slope_pos_low, y, ridge_pos)
            } else {
                Point3D::new(ridge_pos, y, slope_pos_low)
            };
            let slab = Block::new(mats.slab.clone(), Some(slab_state.clone()), None);
            editor.place_block(&slab, pos_low).await;

            if slope_pos_high > slope_pos_low {
                let pos_high = if ridge_along_z {
                    Point3D::new(slope_pos_high, y, ridge_pos)
                } else {
                    Point3D::new(ridge_pos, y, slope_pos_high)
                };
                editor.place_block(&slab, pos_high).await;
            }
        }
    }
}

async fn place_stair_underside_row(
    ridge_along_z: bool,
    slope_pos_low: i32,
    slope_pos_high: i32,
    y: i32,
    ridge_positions: &[i32],
    mats: &RoofMaterials,
    editor: &Editor,
) {
    // Upside-down stairs facing outward
    let facing_low = if ridge_along_z { Cardinal::West } else { Cardinal::North };
    let facing_high = if ridge_along_z { Cardinal::East } else { Cardinal::South };

    for &ridge_pos in ridge_positions {
        // Low side stair
        let pos_low = if ridge_along_z {
            Point3D::new(slope_pos_low, y, ridge_pos)
        } else {
            Point3D::new(ridge_pos, y, slope_pos_low)
        };
        let mut state_low = HashMap::new();
        state_low.insert("facing".to_string(), facing_low.to_string());
        state_low.insert("half".to_string(), "top".to_string());
        let stair_low = Block::new(mats.stairs.clone(), Some(state_low), None);
        editor.place_block(&stair_low, pos_low).await;

        // High side stair (skip if same as low - center position)
        if slope_pos_high > slope_pos_low {
            let pos_high = if ridge_along_z {
                Point3D::new(slope_pos_high, y, ridge_pos)
            } else {
                Point3D::new(ridge_pos, y, slope_pos_high)
            };
            let mut state_high = HashMap::new();
            state_high.insert("facing".to_string(), facing_high.to_string());
            state_high.insert("half".to_string(), "top".to_string());
            let stair_high = Block::new(mats.stairs.clone(), Some(state_high), None);
            editor.place_block(&stair_high, pos_high).await;
        }
    }
}

async fn place_ridge_underside(
    config: &GableConfig,
    base_y: i32,
    ridge_along_z: bool,
    slope_min: i32,
    half_span: i32,
    ridge_positions: &[i32],
    mats: &RoofMaterials,
    editor: &Editor,
) {
    let ridge_slope_pos = slope_min + half_span;

    if config.pitch == RoofPitch::Shallow && half_span % 2 == 1 {
        // Replace top slab with full block at ridge level
        let block_y = base_y + half_span / 2;

        for &ridge_pos in ridge_positions {
            let pos = if ridge_along_z {
                Point3D::new(ridge_slope_pos, block_y, ridge_pos)
            } else {
                Point3D::new(ridge_pos, block_y, ridge_slope_pos)
            };
            let block = Block::from(mats.solid.clone());
            editor.place_block(&block, pos).await;
        }
    } else {
        // Place top slab below the ridge
        let ridge_y_offset = match config.pitch {
            RoofPitch::Shallow => half_span / 2,
            RoofPitch::Medium => half_span,
            RoofPitch::Steep => half_span * 2,
        };
        let ridge_y = base_y + ridge_y_offset - 1;

        let mut slab_state = HashMap::new();
        slab_state.insert("type".to_string(), "top".to_string());

        for &ridge_pos in ridge_positions {
            let pos = if ridge_along_z {
                Point3D::new(ridge_slope_pos, ridge_y, ridge_pos)
            } else {
                Point3D::new(ridge_pos, ridge_y, ridge_slope_pos)
            };
            let slab = Block::new(mats.slab.clone(), Some(slab_state.clone()), None);
            editor.place_block(&slab, pos).await;
        }
    }
}

/// Place the triangular gable wall sections for a gable roof.
/// Call this BEFORE place_roof to ensure walls are placed first.
/// Uses the same wall placement system as regular walls for consistency.
pub async fn place_gable_walls(
    roof: &Roof,
    footprint: &Footprint,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    let config = match &roof.config {
        RoofConfig::Gable(c) => c,
        _ => return, // Not a gable roof
    };

    // Use shared wall materials from placement system
    let wall_mats = WallMaterials::from_palette(palette, materials, rng);

    let (bounds_min, bounds_max) = footprint.bounds().unwrap();
    let width = bounds_max.x - bounds_min.x + 1;
    let depth = bounds_max.y - bounds_min.y + 1;

    // Ridge runs along the longest axis
    let ridge_along_z = depth >= width;

    let overhang = config.overhang;

    // Building bounds (without overhang) for the gable wall positions
    let (gable_wall_z1, gable_wall_z2) = if ridge_along_z {
        (bounds_min.y, bounds_max.y) // Gables at north and south ends
    } else {
        (bounds_min.x, bounds_max.x) // Gables at west and east ends
    };

    // Building width in the slope direction (without overhang)
    let (building_slope_min, building_slope_max) = if ridge_along_z {
        (bounds_min.x, bounds_max.x)
    } else {
        (bounds_min.y, bounds_max.y)
    };
    let building_slope_span = building_slope_max - building_slope_min + 1;
    let building_half_span = building_slope_span / 2;
    let building_needs_center = building_slope_span % 2 == 1;

    // Crossbar level to skip (to avoid overriding timber frame)
    let crossbar_y = Some(roof.base_y);

    // Fill the triangular gable wall area
    let max_gable_row = if building_needs_center { building_half_span } else { building_half_span - 1 };
    for row in 0..=max_gable_row {
        // Height at this row (accounting for overhang offset)
        let roof_row = row + overhang;
        let row_height = get_row_height(roof_row, config.pitch);

        // Fill from base_y up to (but not including) the roof at this row
        for y_offset in 0..row_height {
            let y = roof.base_y + y_offset;

            // Both sides of the slope at this row (within building bounds)
            let slope_pos_low = building_slope_min + row;
            let slope_pos_high = building_slope_max - row;

            // Collect unique positions to avoid double-placing at center
            let positions: Vec<i32> = if slope_pos_low == slope_pos_high {
                vec![slope_pos_low]
            } else {
                vec![slope_pos_low, slope_pos_high]
            };

            for slope_pos in positions {
                if slope_pos < building_slope_min || slope_pos > building_slope_max {
                    continue;
                }

                // Use pillar blocks at the edges (continuation of corner posts)
                let is_edge = slope_pos == building_slope_min || slope_pos == building_slope_max;

                // Gable at first end
                let pos1 = if ridge_along_z {
                    Point3D::new(slope_pos, y, gable_wall_z1)
                } else {
                    Point3D::new(gable_wall_z1, y, slope_pos)
                };
                place_gable_wall_block(editor, pos1, is_edge, crossbar_y, &wall_mats).await;

                // Gable at second end
                let pos2 = if ridge_along_z {
                    Point3D::new(slope_pos, y, gable_wall_z2)
                } else {
                    Point3D::new(gable_wall_z2, y, slope_pos)
                };
                place_gable_wall_block(editor, pos2, is_edge, crossbar_y, &wall_mats).await;
            }
        }
    }
}

/// Get the y offset at a given row (used for gable wall height calculation).
fn get_row_height(row: i32, pitch: RoofPitch) -> i32 {
    match pitch {
        RoofPitch::Shallow => row / 2,
        RoofPitch::Medium => row,
        RoofPitch::Steep => row * 2,
    }
}

/// Describes a block to place in a roof row.
struct RoofBlockEntry {
    block_id: BlockID,
    base_state: Option<HashMap<String, String>>,
    y_adjust: i32,
    is_stair: bool,
    /// If true, the stair should face the opposite direction (away from center).
    invert_facing: bool,
}

/// Get the blocks to place for a given row based on pitch type.
/// Returns (base_y_offset, vec of block entries)
fn get_row_blocks(
    row: i32,
    half_span: i32,
    pitch: RoofPitch,
    needs_ridge_cap: bool,
    slab_id: &BlockID,
    stairs_id: &BlockID,
    block_id: &BlockID,
) -> (i32, Vec<RoofBlockEntry>) {
    let is_ridge = row == half_span;
    let use_slab_cap = is_ridge && needs_ridge_cap;

    match pitch {
        RoofPitch::Shallow => {
            let y_offset = row / 2;
            // Ridge cap should always be bottom slab; other rows alternate
            let is_top_slab = row % 2 == 1 && !use_slab_cap;

            let mut state = HashMap::new();
            state.insert("type".to_string(), if is_top_slab { "top" } else { "bottom" }.to_string());
            (y_offset, vec![RoofBlockEntry {
                block_id: slab_id.clone(),
                base_state: Some(state),
                y_adjust: 0,
                is_stair: false,
                invert_facing: false,
            }])
        }
        RoofPitch::Medium => {
            let y_offset = row;

            if use_slab_cap {
                let mut state = HashMap::new();
                state.insert("type".to_string(), "bottom".to_string());
                (y_offset, vec![RoofBlockEntry {
                    block_id: slab_id.clone(),
                    base_state: Some(state),
                    y_adjust: 0,
                    is_stair: false,
                    invert_facing: false,
                }])
            } else {
                (y_offset, vec![RoofBlockEntry {
                    block_id: stairs_id.clone(),
                    base_state: None,
                    y_adjust: 0,
                    is_stair: true,
                    invert_facing: false,
                }])
            }
        }
        RoofPitch::Steep => {
            let y_offset = row * 2;

            if use_slab_cap {
                let mut state = HashMap::new();
                state.insert("type".to_string(), "bottom".to_string());
                (y_offset, vec![RoofBlockEntry {
                    block_id: slab_id.clone(),
                    base_state: Some(state),
                    y_adjust: 0,
                    is_stair: false,
                    invert_facing: false,
                }])
            } else if row == 0 {
                // Row 0: upside-down stair facing outward + normal stair above
                let mut bottom_state = HashMap::new();
                bottom_state.insert("half".to_string(), "top".to_string());
                (y_offset, vec![
                    RoofBlockEntry {
                        block_id: stairs_id.clone(),
                        base_state: Some(bottom_state),
                        y_adjust: 0,
                        is_stair: true,
                        invert_facing: true,
                    },
                    RoofBlockEntry {
                        block_id: stairs_id.clone(),
                        base_state: None,
                        y_adjust: 1,
                        is_stair: true,
                        invert_facing: false,
                    },
                ])
            } else {
                (y_offset, vec![
                    RoofBlockEntry {
                        block_id: block_id.clone(),
                        base_state: None,
                        y_adjust: 0,
                        is_stair: false,
                        invert_facing: false,
                    },
                    RoofBlockEntry {
                        block_id: stairs_id.clone(),
                        base_state: None,
                        y_adjust: 1,
                        is_stair: true,
                        invert_facing: false,
                    },
                ])
            }
        }
    }
}

/// Place decorative elements at gable ends.
/// Call this AFTER place_gable_roof to add decorations on top of the roof.
pub async fn place_gable_decorations(
    roof: &Roof,
    footprint: &Footprint,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    let config = match &roof.config {
        RoofConfig::Gable(c) => c,
        _ => return, // Not a gable roof
    };

    // Skip if no decoration is configured
    if config.decoration == GableDecoration::None {
        return;
    }

    let roof_mats = RoofMaterials::from_palette(palette, materials, rng);

    let (bounds_min, bounds_max) = footprint.bounds().unwrap();
    let width = bounds_max.x - bounds_min.x + 1;
    let depth = bounds_max.y - bounds_min.y + 1;

    // Ridge runs along the longest axis
    let ridge_along_z = depth >= width;

    let overhang = config.overhang;

    // Gable end positions (one block past the building on each end)
    let (gable_pos_1, gable_pos_2) = if ridge_along_z {
        (bounds_min.y - 1, bounds_max.y + 1) // North and south ends
    } else {
        (bounds_min.x - 1, bounds_max.x + 1) // West and east ends
    };

    // Center of the slope (ridge position)
    let (slope_min, slope_max) = if ridge_along_z {
        (bounds_min.x, bounds_max.x)
    } else {
        (bounds_min.y, bounds_max.y)
    };
    let slope_span = slope_max - slope_min + 1;
    let half_span = slope_span / 2;
    let center_pos = slope_min + half_span;

    // Calculate the peak height
    let roof_row = half_span + overhang;
    let peak_y_offset = get_row_height(roof_row, config.pitch);
    let peak_y = roof.base_y + peak_y_offset;

    match config.decoration {
        GableDecoration::None => {}
        GableDecoration::X => {
            place_x_decoration(
                ridge_along_z, center_pos, gable_pos_1, gable_pos_2, peak_y,
                slope_span, config.pitch, &roof_mats, editor,
            ).await;
        }
        GableDecoration::Overshoot => {
            place_overshoot_decoration(
                ridge_along_z, center_pos, gable_pos_1, gable_pos_2, peak_y,
                slope_span, config.pitch, &roof_mats, editor,
            ).await;
        }
    }
}
