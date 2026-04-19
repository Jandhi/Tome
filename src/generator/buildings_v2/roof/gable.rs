use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Point2D, Point3D, Rect2D};
use crate::minecraft::{Block, BlockForm};
use crate::noise::RNG;
use super::heightmap::RoofHeightmap;

#[derive(Debug, Clone, Copy)]
pub enum GablePitch {
    Slab,   // 0.5 rise per horizontal block
    Stairs, // 1.0 rise per horizontal block
    Double, // 2.0 rise per horizontal block
}

impl GablePitch {
    pub fn value(&self) -> f32 {
        match self {
            GablePitch::Slab => 0.5,
            GablePitch::Stairs => 1.0,
            GablePitch::Double => 2.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RidgeAxis {
    X, // ridge runs along world X, slopes fall off in Z
    Z, // ridge runs along world Z, slopes fall off in X
}

/// Pick ridge axis: longer dimension. Random tiebreak if square.
pub fn pick_ridge_axis(rect: &Rect2D, rng: &mut RNG) -> RidgeAxis {
    if rect.length() > rect.width() {
        RidgeAxis::X
    } else if rect.width() > rect.length() {
        RidgeAxis::Z
    } else if rng.chance(1, 2) {
        RidgeAxis::X
    } else {
        RidgeAxis::Z
    }
}

/// Generate a gable roof heightmap for a single rect.
/// Heights are relative to roof_y. Negative heights appear in overhang zones.
///
/// `suppress_gable_overhang` controls which gable ends skip the overhang:
/// (suppress_low_end, suppress_high_end) along the ridge axis.
/// When a gable end faces an adjacent rect, its overhang should be suppressed
/// to avoid roof blocks overshooting past the junction.
pub fn gable_heightmap(
    rect: &Rect2D,
    pitch: GablePitch,
    ridge_axis: RidgeAxis,
    suppress_gable_overhang: (bool, bool),
) -> RoofHeightmap {
    let overhang = 1;
    let min = rect.min();
    let max = rect.max();
    let pitch_val = pitch.value();

    // Along the ridge axis, suppress overhang on gable ends adjacent to other rects
    let (ridge_lo_oh, ridge_hi_oh) = match ridge_axis {
        RidgeAxis::X => {
            let lo = if suppress_gable_overhang.0 { 0 } else { overhang };
            let hi = if suppress_gable_overhang.1 { 0 } else { overhang };
            (lo, hi)
        }
        RidgeAxis::Z => {
            let lo = if suppress_gable_overhang.0 { 0 } else { overhang };
            let hi = if suppress_gable_overhang.1 { 0 } else { overhang };
            (lo, hi)
        }
    };

    let (hm_min_x, hm_max_x, hm_min_z, hm_max_z) = match ridge_axis {
        RidgeAxis::X => (
            min.x - ridge_lo_oh, max.x + ridge_hi_oh,
            min.y - overhang, max.y + overhang,
        ),
        RidgeAxis::Z => (
            min.x - overhang, max.x + overhang,
            min.y - ridge_lo_oh, max.y + ridge_hi_oh,
        ),
    };

    let width = (hm_max_x - hm_min_x + 1) as usize;
    let depth = (hm_max_z - hm_min_z + 1) as usize;

    let mut hm = RoofHeightmap::new(hm_min_x, hm_min_z, width, depth);

    // Short axis bounds (perpendicular to ridge)
    let (short_min, short_max) = match ridge_axis {
        RidgeAxis::X => (min.y, max.y),
        RidgeAxis::Z => (min.x, max.x),
    };

    for x in hm_min_x..=hm_max_x {
        for z in hm_min_z..=hm_max_z {
            let short_pos = match ridge_axis {
                RidgeAxis::X => z,
                RidgeAxis::Z => x,
            };
            // Signed distance to nearest eave edge (negative outside rect)
            let dist = (short_pos - short_min).min(short_max - short_pos);
            let h = dist as f32 * pitch_val;
            hm.set(x, z, h);
        }
    }

    hm
}

/// Place gable wall triangles at the two short-axis edges of a rect.
/// Fills from roof_y up to the roof surface height with PrimaryWall material.
/// Returns (x, z) positions of any doorways placed in shared gable walls.
pub async fn place_gable_walls(
    editor: &Editor,
    rect: &Rect2D,
    ridge_axis: RidgeAxis,
    pitch: GablePitch,
    roof_y: i32,
    higher_rects: &[&Rect2D],
    all_rects: &[(&Rect2D, i32, RidgeAxis)],
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
) -> Vec<Point2D> {
    let mut doorways = Vec::new();
    let wall_material_id = palette
        .get_material(MaterialRole::PrimaryWall)
        .expect("No primary wall material")
        .clone();
    let mut placer_rng = rng.derive();
    let mut wall_placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng),
        wall_material_id,
    );

    let min = rect.min();
    let max = rect.max();
    let pitch_val = pitch.value();

    // Short axis bounds
    let (short_min, short_max) = match ridge_axis {
        RidgeAxis::X => (min.y, max.y),
        RidgeAxis::Z => (min.x, max.x),
    };

    // Gable end positions along the ridge axis
    let gable_positions: Vec<i32> = match ridge_axis {
        RidgeAxis::X => vec![min.x, max.x],
        RidgeAxis::Z => vec![min.y, max.y],
    };

    // For Double pitch, compute window position at the center of each gable
    let short_mid = (short_min + short_max) / 2;
    let mid_dist = (short_mid - short_min).min(short_max - short_mid);
    let mid_h = mid_dist as f32 * pitch_val;
    let mid_wall_height = mid_h.floor() as i32;
    // Window: 1 wide, 2 tall, vertically centered in the center column
    let win_height = 2;
    let win_y_offset = 1;
    let can_place_window = matches!(pitch, GablePitch::Double) && mid_wall_height >= win_height + 1;

    for &gable_pos in &gable_positions {
        // Check if this gable faces outward (no adjacent rect on the other side)
        let outward_check = match ridge_axis {
            RidgeAxis::X => {
                let outward_x = if gable_pos == min.x { gable_pos - 1 } else { gable_pos + 1 };
                let mid_z = (short_min + short_max) / 2;
                Point2D::new(outward_x, mid_z)
            }
            RidgeAxis::Z => {
                let outward_z = if gable_pos == min.y { gable_pos - 1 } else { gable_pos + 1 };
                let mid_x = (short_min + short_max) / 2;
                Point2D::new(mid_x, outward_z)
            }
        };
        let is_outward = !all_rects.iter().any(|&(r, r_roof_y, _)| {
            (r.min() != rect.min() || r.max() != rect.max())
                && r_roof_y >= roof_y
                && r.contains(outward_check)
        });

        // Check if this gable is on the shared edge of another same-height rect
        // with a perpendicular ridge axis
        let is_shared_edge = all_rects.iter().any(|&(r, r_roof_y, r_axis)| {
            (r.min() != rect.min() || r.max() != rect.max())
                && r_roof_y == roof_y
                && r_axis != ridge_axis
                && r.contains(outward_check)
        });

        for short_pos in short_min..=short_max {
            let (x, z) = match ridge_axis {
                RidgeAxis::X => (gable_pos, short_pos),
                RidgeAxis::Z => (short_pos, gable_pos),
            };

            // Wall precedence: skip if inside a higher-floor rect
            if higher_rects.iter().any(|r| r.contains(Point2D::new(x, z))) {
                continue;
            }

            let dist = (short_pos - short_min).min(short_max - short_pos);
            let h = dist as f32 * pitch_val;
            let frac = h - h.floor();
            let y_wall_top = roof_y + h.floor() as i32;
            // Extend wall to cover half-step and double-pitch extra blocks
            let extra = if matches!(pitch, GablePitch::Stairs) {
                1
            } else if matches!(pitch, GablePitch::Double) {
                0
            } else if frac >= 0.5 - f32::EPSILON {
                1
            } else {
                0
            };

            let is_window_col = can_place_window && is_outward && short_pos == short_mid;
            let win_y_start = roof_y - 1 + win_y_offset;
            let win_y_end = win_y_start + win_height;

            // Doorway: 1 wide, 2 tall at center of shared-edge gable walls
            let is_door_col = is_shared_edge && short_pos == short_mid;
            if is_door_col {
                doorways.push(Point2D::new(x, z));
            }
            let door_y_start = roof_y - 1;
            let door_y_end = door_y_start + 2;

            for y in (roof_y - 1)..(y_wall_top + extra - 1) {
                if is_door_col && y >= door_y_start && y < door_y_end {
                    continue; // leave air for doorway
                } else if is_window_col && y >= win_y_start && y < win_y_end {
                    editor.place_block_forced(
                        &Block::from_id("minecraft:glass_pane".into()),
                        Point3D::new(x, y, z),
                    ).await;
                } else {
                    wall_placer.place_block(
                        editor,
                        Point3D::new(x, y, z),
                        BlockForm::Block,
                        None,
                        None,
                    ).await;
                }
            }
        }
    }

    doorways
}
