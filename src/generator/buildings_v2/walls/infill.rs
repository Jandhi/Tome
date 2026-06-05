//! Wall infill: the block fill for the non-opening panels of each segment.
//! Runs before the timber frame and openings so both can overwrite it.

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Point2D, Point3D};
use crate::minecraft::BlockForm;
use crate::noise::RNG;

use super::super::pipeline::BuildCtx;
use super::segments::{WallSegment, WallSegments, is_inside_opening, segment_cells};

/// A wall infill pattern that controls what blocks are placed in wall panels.
pub enum WallInfill {
    /// Single material fills every cell.
    Solid,
    /// PrimaryStone for most of the wall, SecondaryStone on the bottom row.
    StoneBase,
}

impl WallInfill {
    async fn fill_segment(
        &self,
        editor: &Editor,
        seg: &WallSegment,
        cells: &[Point2D],
        data: &LoadedData,
        palette: &Palette,
        rng: &mut RNG,
    ) {
        match self {
            WallInfill::Solid => {
                let material_id = palette
                    .get_material(MaterialRole::PrimaryWall)
                    .expect("No primary wall material")
                    .clone();
                let mut placer_rng = rng.derive();
                let mut placer = MaterialPlacer::new(
                    Placer::new(&data.materials, &mut placer_rng),
                    material_id,
                );

                for (idx, cell) in cells.iter().enumerate() {
                    for ry in 0..seg.height {
                        if is_inside_opening(&seg.openings, idx as u32, ry) {
                            continue;
                        }
                        let y = seg.base_y + ry as i32;
                        placer.place_block(
                            editor,
                            Point3D::new(cell.x, y, cell.y),
                            BlockForm::Block,
                            None,
                            None,
                        ).await;
                    }
                }
            }
            WallInfill::StoneBase => {
                let primary_id = palette
                    .get_material(MaterialRole::PrimaryStone)
                    .expect("No primary stone material")
                    .clone();
                let secondary_id = palette
                    .get_material(MaterialRole::SecondaryStone)
                    .unwrap_or_else(|| palette.get_material(MaterialRole::PrimaryStone).expect("No stone material"))
                    .clone();
                let mut placer_rng = rng.derive();
                let mut secondary_rng = placer_rng.derive();
                let mut primary = MaterialPlacer::new(
                    Placer::new(&data.materials, &mut placer_rng),
                    primary_id,
                );
                let mut secondary = MaterialPlacer::new(
                    Placer::new(&data.materials, &mut secondary_rng),
                    secondary_id,
                );

                for (idx, cell) in cells.iter().enumerate() {
                    for ry in 0..seg.height {
                        if is_inside_opening(&seg.openings, idx as u32, ry) {
                            continue;
                        }
                        let y = seg.base_y + ry as i32;
                        let placer = if ry == 0 { &mut secondary } else { &mut primary };
                        placer.place_block(
                            editor,
                            Point3D::new(cell.x, y, cell.y),
                            BlockForm::Block,
                            None,
                            None,
                        ).await;
                    }
                }
            }
        }
    }
}

/// Place wall infill blocks for all segments. Should be called BEFORE
/// place_frame and openings so the frame and openings can overwrite.
/// Accepts separate infill patterns for the ground floor and upper floors.
pub async fn place_wall_infill(
    ctx: &mut BuildCtx<'_>,
    wall_segs: &WallSegments,
    ground_infill: &WallInfill,
    upper_infill: &WallInfill,
) {
    let editor: &Editor = &*ctx.editor;
    let data = ctx.data;
    let palette = ctx.palette;
    let rng = &mut *ctx.rng;

    for seg in &wall_segs.segments {
        let cells = segment_cells(seg);
        let infill = if seg.floor == 0 { ground_infill } else { upper_infill };
        infill.fill_segment(editor, seg, &cells, data, palette, rng).await;
    }
}
