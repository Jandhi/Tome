use std::collections::HashMap;

use crate::editor::Editor;
use crate::geometry::{Cardinal, Point3D};
use crate::minecraft::Block;

use super::{RoofMaterials, RoofPitch};

/// Place overshooting elements at gable ends.
/// Dispatches to the appropriate function based on pitch and even/odd width.
pub async fn place_overshoot_decoration(
    ridge_along_z: bool,
    center_pos: i32,
    gable_pos_1: i32,
    gable_pos_2: i32,
    peak_y: i32,
    slope_span: i32,
    pitch: RoofPitch,
    mats: &RoofMaterials,
    editor: &Editor,
) {
    let is_even = slope_span % 2 == 0;

    match (pitch, is_even) {
        (RoofPitch::Shallow, true) => {
            place_overshoot_shallow_even(
                ridge_along_z, center_pos, gable_pos_1, gable_pos_2, peak_y, mats, editor,
            ).await;
        }
        (RoofPitch::Shallow, false) => {
            place_overshoot_shallow_odd(
                ridge_along_z, center_pos, gable_pos_1, gable_pos_2, peak_y, slope_span, mats, editor,
            ).await;
        }
        (RoofPitch::Medium, true) => {
            place_overshoot_medium_even(
                ridge_along_z, center_pos, gable_pos_1, gable_pos_2, peak_y, mats, editor,
            ).await;
        }
        (RoofPitch::Medium, false) => {
            place_overshoot_medium_odd(
                ridge_along_z, center_pos, gable_pos_1, gable_pos_2, peak_y, mats, editor,
            ).await;
        }
        (RoofPitch::Steep, true) => {
            place_overshoot_steep_even(
                ridge_along_z, center_pos, gable_pos_1, gable_pos_2, peak_y, mats, editor,
            ).await;
        }
        (RoofPitch::Steep, false) => {
            place_overshoot_steep_odd(
                ridge_along_z, center_pos, gable_pos_1, gable_pos_2, peak_y, mats, editor,
            ).await;
        }
    }
}

/// Shallow pitch, even width: two overshoots side by side, one block down from peak
async fn place_overshoot_shallow_even(
    ridge_along_z: bool,
    center_pos: i32,
    gable_pos_1: i32,
    gable_pos_2: i32,
    peak_y: i32,
    mats: &RoofMaterials,
    editor: &Editor,
) {
    let center_positions = vec![center_pos - 1, center_pos];
    let y = peak_y - 1;

    place_overshoot_at_positions(
        ridge_along_z, &center_positions, gable_pos_1, gable_pos_2, y, false, mats, editor,
    ).await;
}

/// Shallow pitch, odd width: special handling based on width % 4
/// - width % 4 == 1 (5, 9, 13...): at peak_y, two stairs
/// - width % 4 == 3 (7, 11, 15...): at peak_y, single upside-down stair only
async fn place_overshoot_shallow_odd(
    ridge_along_z: bool,
    center_pos: i32,
    gable_pos_1: i32,
    gable_pos_2: i32,
    peak_y: i32,
    slope_span: i32,
    mats: &RoofMaterials,
    editor: &Editor,
) {
    let center_positions = vec![center_pos];

    if slope_span % 4 == 1 {
        // widths 5, 9, 13...: at peak_y, two stairs
        place_overshoot_at_positions(
            ridge_along_z, &center_positions, gable_pos_1, gable_pos_2, peak_y, false, mats, editor,
        ).await;
    } else {
        // widths 7, 11, 15... (slope_span % 4 == 3): at peak_y, single upside-down stair
        place_overshoot_at_positions(
            ridge_along_z, &center_positions, gable_pos_1, gable_pos_2, peak_y, true, mats, editor,
        ).await;
    }
}

/// Medium pitch, even width: two overshoots side by side, one block down from peak
async fn place_overshoot_medium_even(
    ridge_along_z: bool,
    center_pos: i32,
    gable_pos_1: i32,
    gable_pos_2: i32,
    peak_y: i32,
    mats: &RoofMaterials,
    editor: &Editor,
) {
    let center_positions = vec![center_pos - 1, center_pos];
    let y = peak_y - 1;

    place_overshoot_at_positions(
        ridge_along_z, &center_positions, gable_pos_1, gable_pos_2, y, false, mats, editor,
    ).await;
}

/// Medium pitch, odd width: single overshoot at center, one block down from peak
async fn place_overshoot_medium_odd(
    ridge_along_z: bool,
    center_pos: i32,
    gable_pos_1: i32,
    gable_pos_2: i32,
    peak_y: i32,
    mats: &RoofMaterials,
    editor: &Editor,
) {
    let center_positions = vec![center_pos];
    let y = peak_y - 1;

    place_overshoot_at_positions(
        ridge_along_z, &center_positions, gable_pos_1, gable_pos_2, y, false, mats, editor,
    ).await;
}

/// Steep pitch, even width: two overshoots side by side, one block down from peak
async fn place_overshoot_steep_even(
    ridge_along_z: bool,
    center_pos: i32,
    gable_pos_1: i32,
    gable_pos_2: i32,
    peak_y: i32,
    mats: &RoofMaterials,
    editor: &Editor,
) {
    let center_positions = vec![center_pos - 1, center_pos];
    let y = peak_y - 1;

    place_overshoot_at_positions(
        ridge_along_z, &center_positions, gable_pos_1, gable_pos_2, y, false, mats, editor,
    ).await;
}

/// Steep pitch, odd width: single overshoot at center, one block down from peak
async fn place_overshoot_steep_odd(
    ridge_along_z: bool,
    center_pos: i32,
    gable_pos_1: i32,
    gable_pos_2: i32,
    peak_y: i32,
    mats: &RoofMaterials,
    editor: &Editor,
) {
    let center_positions = vec![center_pos];
    let y = peak_y - 1;

    place_overshoot_at_positions(
        ridge_along_z, &center_positions, gable_pos_1, gable_pos_2, y, false, mats, editor,
    ).await;
}

/// Helper: place overshoot elements at the given center positions and y level
async fn place_overshoot_at_positions(
    ridge_along_z: bool,
    center_positions: &[i32],
    gable_pos_1: i32,
    gable_pos_2: i32,
    y: i32,
    single_stair_only: bool,
    mats: &RoofMaterials,
    editor: &Editor,
) {
    for &gable_pos in &[gable_pos_1, gable_pos_2] {
        let is_first_end = gable_pos == gable_pos_1;
        let outward_facing = if ridge_along_z {
            if is_first_end { Cardinal::North } else { Cardinal::South }
        } else {
            if is_first_end { Cardinal::West } else { Cardinal::East }
        };

        // Move one block further out from the gable
        let adjusted_gable_pos = if is_first_end { gable_pos - 1 } else { gable_pos + 1 };

        for &cp in center_positions {
            let pos = if ridge_along_z {
                Point3D::new(cp, y, adjusted_gable_pos)
            } else {
                Point3D::new(adjusted_gable_pos, y, cp)
            };

            // Lower block: inverted stair facing inward
            let inward_facing = outward_facing.opposite();
            let mut state_lower = HashMap::new();
            state_lower.insert("facing".to_string(), inward_facing.to_string());
            state_lower.insert("half".to_string(), "top".to_string());
            let stair_lower = Block::new(mats.stairs.clone(), Some(state_lower), None);
            editor.place_block(&stair_lower, pos).await;

            // Upper block: normal stair facing outward (skip for single_stair_only)
            if !single_stair_only {
                let pos_upper = Point3D::new(pos.x, pos.y + 1, pos.z);
                let mut state_upper = HashMap::new();
                state_upper.insert("facing".to_string(), outward_facing.to_string());
                let stair_upper = Block::new(mats.stairs.clone(), Some(state_upper), None);
                editor.place_block(&stair_upper, pos_upper).await;
            }
        }
    }
}
