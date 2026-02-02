use std::collections::HashMap;

use crate::editor::Editor;
use crate::geometry::{Cardinal, DOWN, Point3D, UP};
use crate::minecraft::Block;

use super::{RoofMaterials, RoofPitch};

/// Place X-shaped decoration at gable peaks (Viking longhouse style).
/// Dispatches to pitch/parity-specific functions.
pub async fn place_x_decoration(
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
    let half_span = slope_span / 2;

    for &gable_pos in &[gable_pos_1, gable_pos_2] {
        match (pitch, is_even) {
            (RoofPitch::Steep, true) => {
                place_x_steep_even(ridge_along_z, center_pos, gable_pos, peak_y, mats, editor).await;
            }
            (RoofPitch::Steep, false) => {
                place_x_steep_odd(ridge_along_z, center_pos, gable_pos, peak_y, mats, editor).await;
            }
            (RoofPitch::Medium, true) => {
                place_x_medium_even(ridge_along_z, center_pos, gable_pos, peak_y, mats, editor).await;
            }
            (RoofPitch::Medium, false) => {
                place_x_medium_odd(ridge_along_z, center_pos, gable_pos, peak_y, half_span, mats, editor).await;
            }
            (RoofPitch::Shallow, true) => {
                place_x_shallow_even(ridge_along_z, center_pos, gable_pos, peak_y, half_span, mats, editor).await;
            }
            (RoofPitch::Shallow, false) => {
                place_x_shallow_odd(ridge_along_z, center_pos, gable_pos, peak_y, half_span, mats, editor).await;
            }
        }
    }
}

/// Steep pitch, even width: place slabs at offset 1 from center.
async fn place_x_steep_even(
    ridge_along_z: bool,
    center_pos: i32,
    gable_pos: i32,
    peak_y: i32,
    mats: &RoofMaterials,
    editor: &Editor,
) {
    let (pos_left, pos_right) = get_x_positions(ridge_along_z, center_pos, gable_pos, peak_y, 1);

    let slab_state = HashMap::from([("type".to_string(), "bottom".to_string())]);
    let slab = Block::new(mats.slab.clone(), Some(slab_state), None);
    editor.place_block(&slab, pos_left).await;
    editor.place_block(&slab, pos_right).await;
}

/// Steep pitch, odd width: place slabs at center-1 and center+1.
async fn place_x_steep_odd(
    ridge_along_z: bool,
    center_pos: i32,
    gable_pos: i32,
    peak_y: i32,
    mats: &RoofMaterials,
    editor: &Editor,
) {
    let (pos_left, pos_right) = get_x_positions_odd(ridge_along_z, center_pos, gable_pos, peak_y);

    let slab_state = HashMap::from([("type".to_string(), "bottom".to_string())]);
    let slab = Block::new(mats.slab.clone(), Some(slab_state), None);
    editor.place_block(&slab, pos_left).await;
    editor.place_block(&slab, pos_right).await;
}

/// Medium pitch, even width: place slabs at offset 1 from center.
async fn place_x_medium_even(
    ridge_along_z: bool,
    center_pos: i32,
    gable_pos: i32,
    peak_y: i32,
    mats: &RoofMaterials,
    editor: &Editor,
) {
    let (pos_left, pos_right) = get_x_positions(ridge_along_z, center_pos, gable_pos, peak_y, 1);

    let slab_state = HashMap::from([("type".to_string(), "bottom".to_string())]);
    let slab = Block::new(mats.slab.clone(), Some(slab_state), None);
    editor.place_block(&slab, pos_left).await;
    editor.place_block(&slab, pos_right).await;
}

/// Medium pitch, odd width: place upside-down stairs facing inward.
async fn place_x_medium_odd(
    ridge_along_z: bool,
    center_pos: i32,
    gable_pos: i32,
    peak_y: i32,
    half_span: i32,
    mats: &RoofMaterials,
    editor: &Editor,
) {
    let (pos_left, pos_right) = get_x_positions_odd(ridge_along_z, center_pos, gable_pos, peak_y);
    let (facing_left, facing_right) = get_x_facings(ridge_along_z);

    let row = half_span - 1;
    let is_top_slab_below = row % 2 == 0;

    for (pos, facing) in [(pos_left, facing_left), (pos_right, facing_right)] {
        let state = HashMap::from([
                ("facing".to_string(), facing.to_string()),
                ("half".to_string(), "top".to_string()),
            ]);
            let stair = Block::new(mats.stairs.clone(), Some(state), None);
            editor.place_block(&stair, pos).await;
    }
}

/// Shallow pitch, even width: place stairs at offset 2 from center.
async fn place_x_shallow_even(
    ridge_along_z: bool,
    center_pos: i32,
    gable_pos: i32,
    peak_y: i32,
    half_span: i32,
    mats: &RoofMaterials,
    editor: &Editor,
) {
    let (pos_left, pos_right) = get_x_positions(ridge_along_z, center_pos, gable_pos, peak_y, 2);
    let (facing_left, facing_right) = get_x_facings(ridge_along_z);

    let row = half_span - 2;
    let is_bottom_slab_below = row % 2 == 1;

    for (pos, facing) in [(pos_left, facing_left), (pos_right, facing_right)] {
        if is_bottom_slab_below {
            let state = HashMap::from([
                ("facing".to_string(), facing.to_string()),
                ("half".to_string(), "bottom".to_string()),
            ]);
            let stair = Block::new(mats.stairs.clone(), Some(state), None);
            editor.place_block(&stair, pos + DOWN).await;

            let slab_state = HashMap::from([("type".to_string(), "bottom".to_string())]);
            let slab = Block::new(mats.slab.clone(), Some(slab_state), None);
            editor.place_block(&slab, pos).await;
        } else {
            let state = HashMap::from([
                ("facing".to_string(), facing.to_string()),
                ("half".to_string(), "top".to_string()),
            ]);
            let stair = Block::new(mats.stairs.clone(), Some(state), None);
            editor.place_block(&stair, pos).await;
        }
    }
}

/// Shallow pitch, odd width: place stairs at center-1 and center+1.
async fn place_x_shallow_odd(
    ridge_along_z: bool,
    center_pos: i32,
    gable_pos: i32,
    peak_y: i32,
    half_span: i32,
    mats: &RoofMaterials,
    editor: &Editor,
) {
    let (pos_left, pos_right) = get_x_positions_odd(ridge_along_z, center_pos, gable_pos, peak_y);
    let (facing_left, facing_right) = get_x_facings(ridge_along_z);

    let row = half_span - 1;
    let is_top_slab_below = row % 2 == 0;

    for (pos, facing) in [(pos_left, facing_left), (pos_right, facing_right)] {
        if is_top_slab_below {
            let state = HashMap::from([
                ("facing".to_string(), facing.to_string()),
                ("half".to_string(), "top".to_string()),
            ]);
            let stair = Block::new(mats.stairs.clone(), Some(state), None);
            editor.place_block(&stair, pos).await;
        } else {
            let state = HashMap::from([
                ("facing".to_string(), facing.to_string()),
                ("half".to_string(), "bottom".to_string()),
            ]);
            let stair = Block::new(mats.stairs.clone(), Some(state), None);
            editor.place_block(&stair, pos).await;

            let pos_above = pos + UP;
            let slab_state = HashMap::from([("type".to_string(), "bottom".to_string())]);
            let slab = Block::new(mats.slab.clone(), Some(slab_state), None);
            editor.place_block(&slab, pos_above).await;
        }
    }
}

/// Get left/right positions for even-width X decoration with given offset.
fn get_x_positions(ridge_along_z: bool, center_pos: i32, gable_pos: i32, peak_y: i32, offset: i32) -> (Point3D, Point3D) {
    if ridge_along_z {
        (
            Point3D::new(center_pos - offset, peak_y, gable_pos),
            Point3D::new(center_pos + offset - 1, peak_y, gable_pos),
        )
    } else {
        (
            Point3D::new(gable_pos, peak_y, center_pos - offset),
            Point3D::new(gable_pos, peak_y, center_pos + offset - 1),
        )
    }
}

/// Get left/right positions for odd-width X decoration (center-1 and center+1).
fn get_x_positions_odd(ridge_along_z: bool, center_pos: i32, gable_pos: i32, peak_y: i32) -> (Point3D, Point3D) {
    if ridge_along_z {
        (
            Point3D::new(center_pos - 1, peak_y, gable_pos),
            Point3D::new(center_pos + 1, peak_y, gable_pos),
        )
    } else {
        (
            Point3D::new(gable_pos, peak_y, center_pos - 1),
            Point3D::new(gable_pos, peak_y, center_pos + 1),
        )
    }
}

/// Get facing directions for X decoration stairs (facing inward toward center).
fn get_x_facings(ridge_along_z: bool) -> (Cardinal, Cardinal) {
    if ridge_along_z {
        (Cardinal::East, Cardinal::West)
    } else {
        (Cardinal::South, Cardinal::North)
    }
}
