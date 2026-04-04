use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Point2D, Point3D, Rect2D};
use crate::minecraft::BlockForm;
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
pub fn gable_heightmap(rect: &Rect2D, pitch: GablePitch, ridge_axis: RidgeAxis) -> RoofHeightmap {
    let overhang = 1;
    let min = rect.min();
    let max = rect.max();
    let pitch_val = pitch.value();

    let hm_min_x = min.x - overhang;
    let hm_min_z = min.y - overhang;
    let hm_max_x = max.x + overhang;
    let hm_max_z = max.y + overhang;
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
pub async fn place_gable_walls(
    editor: &Editor,
    rect: &Rect2D,
    ridge_axis: RidgeAxis,
    pitch: GablePitch,
    roof_y: i32,
    higher_rects: &[&Rect2D],
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
) {
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

    for &gable_pos in &gable_positions {
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

            for y in (roof_y - 1)..(y_wall_top + extra - 1) {
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
