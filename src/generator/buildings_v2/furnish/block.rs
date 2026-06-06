//! Block-level transforms: wall-relative offset/facing resolution, block-string
//! parsing, rotation, and palette substitution applied to furniture blocks.

use std::collections::HashMap;

use crate::generator::materials::{Material, MaterialId, MaterialRole, Palette};
use crate::geometry::{Cardinal, Point2D};
use crate::minecraft::{Block, BlockForm, color_block, string_to_block};
use crate::noise::RNG;

use super::data::PaletteSwap;
use super::types::FacingMode;

/// Convert a wall-relative offset [along, y, away] to world (dx, dz, dy).
pub(super) fn resolve_offset(offset: [i32; 3], wall_dir: Cardinal) -> (i32, i32, i32) {
    let along: Point2D = wall_dir.rotate_right().into();
    let away: Point2D = (-wall_dir).into();
    let dx = along.x * offset[0] + away.x * offset[2];
    let dz = along.y * offset[0] + away.y * offset[2];
    (dx, dz, offset[1])
}

/// Convert a 2D wall-relative offset [along, away] to world (dx, dz).
pub(super) fn resolve_offset_2d(offset: [i32; 2], wall_dir: Cardinal) -> (i32, i32) {
    let (dx, dz, _) = resolve_offset([offset[0], 0, offset[1]], wall_dir);
    (dx, dz)
}

/// Resolve facing for a constraint given the wall direction.
pub(super) fn resolve_facing(mode: FacingMode, wall_dir: Cardinal) -> Option<String> {
    match mode {
        FacingMode::None => Option::None,
        FacingMode::AwayFromWall => Some((-wall_dir).to_string()),
        FacingMode::TowardWall => Some(wall_dir.to_string()),
        FacingMode::Perpendicular => Some(wall_dir.rotate_right().to_string()),
    }
}

/// Clone a block and merge a facing state into it. Only updates `facing`
/// when the block already declares a `facing` property in its literal —
/// otherwise blocks that have no facing state (slabs, wool, planks, …)
/// would receive an invalid state like `oak_slab[type=bottom,facing=north]`
/// that the server rejects silently.
pub(super) fn apply_facing(block: &Block, facing: Option<String>) -> Block {
    let mut result = block.clone();
    if let Some(f) = facing {
        if let Some(state) = result.state.as_mut() {
            if state.contains_key("facing") {
                state.insert("facing".into(), f);
            }
        }
    }
    result
}

/// Parse a block string into a Block.
pub(super) fn parse_block(block_str: &str) -> Block {
    string_to_block(block_str)
        .unwrap_or_else(|| Block::from_id(block_str.into()))
}

/// Apply palette substitution to a block.
pub(super) fn swap_block_for_palette(
    block: Block,
    swap: PaletteSwap,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) -> Block {
    match swap {
        PaletteSwap::None => block,
        PaletteSwap::Wood => {
            // Furniture wants the SECONDARY wood so it contrasts with the
            // building's primary wood (used for floors/frame). Palette
            // auto-falls-back to PrimaryWood when SecondaryWood isn't defined
            // (see MaterialRole::backup_role).
            let form = BlockForm::infer_from_block(&block.id);
            if let Some(new_id) = palette.get_block(MaterialRole::SecondaryWood, &form, materials, rng) {
                Block::new(new_id.clone(), block.state, block.data)
            } else {
                block
            }
        }
        PaletteSwap::Color => {
            if let Some(color) = palette.primary_color {
                Block::new(color_block(block.id, color), block.state, block.data)
            } else {
                block
            }
        }
        PaletteSwap::SecondaryColor => {
            // Falls back to primary so patterned items degrade to solid
            // primary-color when no secondary is defined.
            let color = palette.secondary_color.or(palette.primary_color);
            if let Some(color) = color {
                Block::new(color_block(block.id, color), block.state, block.data)
            } else {
                block
            }
        }
    }
}

/// Rotate any existing `facing` state in a block.
/// North is identity (no rotation). East = 1 clockwise, South = 2, West = 3.
pub(super) fn rotate_block(block: &Block, dir: Cardinal) -> Block {
    let mut result = block.clone();
    if let Some(state) = &mut result.state {
        if let Some(facing) = state.get("facing") {
            let parsed: Option<Cardinal> = match facing.as_str() {
                "north" => Some(Cardinal::North),
                "south" => Some(Cardinal::South),
                "east" => Some(Cardinal::East),
                "west" => Some(Cardinal::West),
                _ => None,
            };
            if let Some(orig) = parsed {
                let rotated = match dir {
                    Cardinal::North => orig,
                    Cardinal::East => orig.rotate_right(),
                    Cardinal::South => orig.rotate_right().rotate_right(),
                    Cardinal::West => orig.rotate_right().rotate_right().rotate_right(),
                };
                state.insert("facing".into(), rotated.to_string());
            }
        }
    }
    result
}
