use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Point2D, Point3D, Rect2D};
use crate::minecraft::{Block, BlockForm};
use crate::noise::RNG;
use super::heightmap::RoofHeightmap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
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

/// Generate a gable roof heightmap for a single rect, with a one-block
/// overhang on every side. Heights are relative to roof_y; the eave overhang
/// is negative. Overhang cells that run into another rect's roof are cleared
/// afterwards by `gable_roof::clear_covered_gable_overhang`.
pub fn gable_heightmap(
    rect: &Rect2D,
    pitch: GablePitch,
    ridge_axis: RidgeAxis,
) -> RoofHeightmap {
    let overhang = 1;
    let min = rect.min();
    let max = rect.max();
    let pitch_val = pitch.value();

    let (hm_min_x, hm_max_x, hm_min_z, hm_max_z) =
        (min.x - overhang, max.x + overhang, min.y - overhang, max.y + overhang);

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
/// Returns (x, z) positions of any doorways cut through shared gable walls
/// (both cells of the opening where two gables meet end to end).
/// `knee` is the rect's knee-wall height (`Frame::knee_for_rect`): outward
/// gable columns sit on the exterior knee wall, but shared (interior) ones
/// reach down through it to the attic floor so the seam stays closed and the
/// doorway stands on the floor.
pub async fn place_gable_walls(
    editor: &Editor,
    rect: &Rect2D,
    ridge_axis: RidgeAxis,
    pitch: GablePitch,
    roof_y: i32,
    knee: i32,
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

    // Our extent along the short axis, for any rect.
    let span = |r: &Rect2D| match ridge_axis {
        RidgeAxis::X => (r.min().y, r.max().y),
        RidgeAxis::Z => (r.min().x, r.max().x),
    };
    let others: Vec<(&Rect2D, i32, RidgeAxis)> = all_rects
        .iter()
        .copied()
        .filter(|(r, _, _)| r.min() != rect.min() || r.max() != rect.max())
        .collect();

    for &gable_pos in &gable_positions {
        // The cell just past this gable end, at a position along the short axis.
        let outward_along = if gable_pos == gable_positions[0] { gable_pos - 1 } else { gable_pos + 1 };
        let outward_cell = |short_pos: i32| match ridge_axis {
            RidgeAxis::X => Point2D::new(outward_along, short_pos),
            RidgeAxis::Z => Point2D::new(short_pos, outward_along),
        };
        // A column faces outward when no rect at least as tall lies beyond it.
        let col_outward = |short_pos: i32| {
            !others.iter().any(|&(r, r_roof_y, _)| r_roof_y >= roof_y && r.contains(outward_cell(short_pos)))
        };

        // Doorway columns (1 wide, 2 tall), with whether the cell beyond is
        // part of the opening too:
        // - at our middle when the gable stands on the side of a same-height
        //   rect with a perpendicular ridge (T/L junction);
        // - at the middle of each same-height, same-axis rect whose own gable
        //   meets this one end to end. The wider of the two keeps its gable
        //   wall, standing above the narrower roof, and carries the doorway;
        //   the narrower one's wall would only double it, so that end is
        //   skipped and its column counts as part of the opening.
        let mut door_cols: Vec<(i32, bool)> = Vec::new();
        if others.iter().any(|&(r, r_roof_y, r_axis)| {
            r_roof_y == roof_y && r_axis != ridge_axis && r.contains(outward_cell(short_mid))
        }) {
            door_cols.push((short_mid, false));
        }
        let mut covered_by_wider = false;
        for &(r, r_roof_y, r_axis) in &others {
            if r_roof_y != roof_y || r_axis != ridge_axis {
                continue;
            }
            let (r_lo, r_hi) = span(r);
            let (lo, hi) = (r_lo.max(short_min), r_hi.min(short_max));
            if lo > hi || !r.contains(outward_cell(lo)) {
                continue;
            }
            let same_span = (r_lo, r_hi) == (short_min, short_max);
            let r_first = (r.min().x, r.min().y) < (rect.min().x, rect.min().y);
            if r_lo <= short_min && r_hi >= short_max && (!same_span || r_first) {
                covered_by_wider = true;
            } else {
                door_cols.push(((lo + hi) / 2, true));
            }
        }
        if covered_by_wider {
            continue;
        }

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

            let is_outward = col_outward(short_pos);
            let is_window_col = can_place_window && is_outward && short_pos == short_mid;
            let win_y_start = roof_y - 1 + win_y_offset;
            let win_y_end = win_y_start + win_height;

            let door = door_cols.iter().find(|(col, _)| *col == short_pos);
            let is_door_col = door.is_some();
            if let Some(&(_, through)) = door {
                doorways.push(Point2D::new(x, z));
                if through {
                    doorways.push(outward_cell(short_pos));
                }
            }
            let wall_base_y = if is_outward { roof_y - 1 } else { roof_y - 1 - knee };
            let door_y_start = wall_base_y;
            let door_y_end = door_y_start + 2;

            for y in wall_base_y..(y_wall_top + extra - 1) {
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
