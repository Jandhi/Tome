use std::collections::HashMap;

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Cardinal, Point2D, Point3D, Rect2D};
use crate::minecraft::BlockForm;
use crate::noise::RNG;
use super::gable::GablePitch;
use super::heightmap::RoofHeightmap;

/// Direction of steepest ascent (toward the ridge). Used for stair facing.
pub(super) fn stair_facing(hm: &RoofHeightmap, x: i32, z: i32) -> Cardinal {
    let h = hm.get(x, z);
    let mut best_dir = Cardinal::North;
    let mut best_rise = f32::NEG_INFINITY;

    for (dir, dx, dz) in [
        (Cardinal::North, 0, -1),
        (Cardinal::South, 0, 1),
        (Cardinal::East, 1, 0),
        (Cardinal::West, -1, 0),
    ] {
        let nh = hm.get(x + dx, z + dz);
        if nh == f32::NEG_INFINITY {
            continue;
        }
        let rise = nh - h;
        if rise > best_rise {
            best_rise = rise;
            best_dir = dir;
        }
    }

    best_dir
}

/// Check if a position is at the ridge (no neighbor is strictly higher).
pub fn is_ridge(hm: &RoofHeightmap, x: i32, z: i32) -> bool {
    let h = hm.get(x, z);
    if h == f32::NEG_INFINITY {
        return false;
    }
    for (dx, dz) in [(0, -1), (0, 1), (1, 0), (-1, 0)] {
        if hm.get(x + dx, z + dz) > h {
            return false;
        }
    }
    true
}

/// Place roof blocks from a merged heightmap.
/// - Stairs on slopes, top slabs at ridges and half-heights.
/// - Fill blocks below surface inside the footprint.
/// - Overhang gets only the surface block, no fill.
/// - Overhang brackets vary by pitch.
pub async fn place_roof_blocks(
    editor: &Editor,
    hm: &RoofHeightmap,
    roof_y: i32,
    pitch: GablePitch,
    group_rects: &[&Rect2D],
    higher_rects: &[&Rect2D],
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
) {
    let material_id = palette
        .get_material(MaterialRole::PrimaryRoof)
        .expect("No primary roof material")
        .clone();

    let mut placer_rng = rng.derive();
    let mut placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng),
        material_id,
    );

    let slab_state = HashMap::from([("type".to_string(), "bottom".to_string())]);
    let top_slab_state = HashMap::from([("type".to_string(), "top".to_string())]);

    for x in hm.min_x()..=hm.max_x() {
        for z in hm.min_z()..=hm.max_z() {
            let h = hm.get(x, z);
            if h == f32::NEG_INFINITY {
                continue;
            }

            let p = Point2D::new(x, z);

            // Wall precedence: skip if inside a higher-floor rect
            if higher_rects.iter().any(|r| r.contains(p)) {
                continue;
            }

            let h_adj = if matches!(pitch, GablePitch::Slab) { h - 0.5 } else { h };
            let y_floor = roof_y + h_adj.floor() as i32;
            let frac = h_adj - h_adj.floor();
            let is_overhang = !group_rects.iter().any(|r| r.contains(p));

            // Base row block (only inside footprint, not overhang, not double pitch)
            if !is_overhang && !matches!(pitch, GablePitch::Double) {
                placer.place_block(
                    editor,
                    Point3D::new(x, roof_y - 1, z),
                    BlockForm::Block,
                    None,
                    None,
                ).await;
            }

            // Surface block
            if !matches!(pitch, GablePitch::Slab) && is_ridge(hm, x, z) {
                placer.place_block(
                    editor,
                    Point3D::new(x, y_floor, z),
                    BlockForm::Slab,
                    Some(&slab_state),
                    None,
                ).await;

                // Double pitch: block below ridge slab (inside footprint)
                if matches!(pitch, GablePitch::Double) && !is_overhang && y_floor - 1 != roof_y {
                    placer.place_block(
                        editor,
                        Point3D::new(x, y_floor - 1, z),
                        BlockForm::Block,
                        None,
                        None,
                    ).await;
                }

                // Overhang ridge bracket: full block below the ridge
                if is_overhang {
                    placer.place_block(
                        editor,
                        Point3D::new(x, y_floor - 1, z),
                        BlockForm::Block,
                        None,
                        None,
                    ).await;

                    // Double pitch: upside-down stair below the block
                    if matches!(pitch, GablePitch::Double) {
                        let facing = stair_facing(hm, x, z);
                        let inv_state = HashMap::from([
                            ("facing".to_string(), facing.to_string()),
                            ("half".to_string(), "top".to_string()),
                        ]);
                        placer.place_block(
                            editor,
                            Point3D::new(x, y_floor - 2, z),
                            BlockForm::Stairs,
                            Some(&inv_state),
                            None,
                        ).await;
                    }
                }
            } else if frac >= 0.5 - f32::EPSILON {
                if is_overhang {
                    // Overhang: full block instead of top slab
                    placer.place_block(
                        editor,
                        Point3D::new(x, y_floor, z),
                        BlockForm::Block,
                        None,
                        None,
                    ).await;
                } else {
                    // Half-step: top slab
                    placer.place_block(
                        editor,
                        Point3D::new(x, y_floor, z),
                        BlockForm::Slab,
                        Some(&top_slab_state),
                        None,
                    ).await;
                }
            } else if matches!(pitch, GablePitch::Slab) {
                // Slab pitch: bottom slab
                placer.place_block(
                    editor,
                    Point3D::new(x, y_floor, z),
                    BlockForm::Slab,
                    Some(&slab_state),
                    None,
                ).await;
                if is_overhang {
                    // Top slab below to fill overhang
                    placer.place_block(
                        editor,
                        Point3D::new(x, y_floor - 1, z),
                        BlockForm::Slab,
                        Some(&top_slab_state),
                        None,
                    ).await;
                }
            } else {
                let facing = stair_facing(hm, x, z);
                let stair_state = HashMap::from([
                    ("facing".to_string(), facing.to_string()),
                ]);
                placer.place_block(
                    editor,
                    Point3D::new(x, y_floor, z),
                    BlockForm::Stairs,
                    Some(&stair_state),
                    None,
                ).await;

                // Double pitch: block below stair (inside footprint, eave and upper stairs)
                if matches!(pitch, GablePitch::Double) && !is_overhang && h >= 0.0 {
                    if y_floor - 1 != roof_y {
                        placer.place_block(
                            editor,
                            Point3D::new(x, y_floor - 1, z),
                            BlockForm::Block,
                            None,
                            None,
                        ).await;
                    }

                    // Extra interior block below, but don't cut through the ceiling
                    if y_floor - 2 >= roof_y - 1 && y_floor - 2 != roof_y {
                        placer.place_block(
                            editor,
                            Point3D::new(x, y_floor - 2, z),
                            BlockForm::Block,
                            None,
                            None,
                        ).await;
                    }
                }

                // Overhang brackets — pitch-dependent
                if is_overhang && h >= 0.0 {
                    let opposite = -facing;
                    let inv_state = HashMap::from([
                        ("facing".to_string(), opposite.to_string()),
                        ("half".to_string(), "top".to_string()),
                    ]);

                    match pitch {
                        GablePitch::Slab => {
                            // No brackets for slab pitch
                        }
                        GablePitch::Stairs => {
                            // One upside-down stair below
                            placer.place_block(
                                editor,
                                Point3D::new(x, y_floor - 1, z),
                                BlockForm::Stairs,
                                Some(&inv_state),
                                None,
                            ).await;
                        }
                        GablePitch::Double => {
                            // Block + upside-down stair below (stairs, block, inv stair top to bottom)
                            placer.place_block(
                                editor,
                                Point3D::new(x, y_floor - 1, z),
                                BlockForm::Block,
                                None,
                                None,
                            ).await;
                            placer.place_block(
                                editor,
                                Point3D::new(x, y_floor - 2, z),
                                BlockForm::Stairs,
                                Some(&inv_state),
                                None,
                            ).await;
                        }
                    }
                }
            }
        }
    }
}
