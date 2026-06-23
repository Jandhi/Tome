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
    /// Japanese shoji-style: PrimaryWall (white) panels over a PrimaryWood
    /// (timber) baseboard, divided by evenly spaced vertical PrimaryWood beams
    /// so the white panels read as 2–4 blocks wide.
    TimberPanels,
}

/// Interior beam cell indices for a wall run of `len` cells, dividing it into
/// white panels 2–4 cells wide, as symmetric as possible. Returns an empty list
/// for short runs (≤ 4 cells), which stay a single panel.
fn beam_indices(len: usize) -> Vec<usize> {
    if len < 5 {
        return Vec::new();
    }

    // Pick the beam count whose panels are closest to 3 wide while every panel
    // stays within [2, 4].
    let mut best: Option<(i32, usize)> = None;
    for k in 1..len {
        let panels = k + 1;
        let cells = len - k;
        let min_w = cells / panels;
        let max_w = if cells % panels == 0 { min_w } else { min_w + 1 };
        if min_w < 2 {
            break; // panels too narrow; more beams only makes it worse
        }
        if max_w > 4 {
            continue; // panels too wide; need more beams
        }
        let score = (cells as i32 - 3 * panels as i32).abs();
        if best.map_or(true, |(s, _)| score < s) {
            best = Some((score, k));
        }
    }
    let k = match best {
        Some((_, k)) => k,
        None => return Vec::new(),
    };

    // Distribute panel widths evenly, biasing the wider panels toward the centre
    // so the run stays symmetric.
    let panels = k + 1;
    let base = (len - k) / panels;
    let extra = (len - k) % panels;
    let mut widths = vec![base; panels];
    let start = (panels - extra) / 2;
    for w in widths.iter_mut().skip(start).take(extra) {
        *w += 1;
    }

    let mut indices = Vec::with_capacity(k);
    let mut pos = 0usize;
    for w in widths.iter().take(panels - 1) {
        pos += w; // end of this panel
        indices.push(pos); // the beam cell
        pos += 1; // step past the beam
    }
    indices
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
            WallInfill::TimberPanels => {
                let white_id = palette
                    .get_material(MaterialRole::PrimaryWall)
                    .expect("No primary wall material")
                    .clone();
                let wood_id = palette
                    .get_material(MaterialRole::PrimaryWood)
                    .expect("No primary wood material")
                    .clone();
                let mut white_rng = rng.derive();
                let mut wood_rng = white_rng.derive();
                let mut white = MaterialPlacer::new(
                    Placer::new(&data.materials, &mut white_rng),
                    white_id,
                );
                let mut wood = MaterialPlacer::new(
                    Placer::new(&data.materials, &mut wood_rng),
                    wood_id,
                );

                let beams: std::collections::HashSet<usize> =
                    beam_indices(cells.len()).into_iter().collect();
                // Wood baseboard only on the ground floor; upper floors get beams
                // over white with no timber sill.
                let baseboard = seg.floor == 0;

                for (idx, cell) in cells.iter().enumerate() {
                    let is_beam = beams.contains(&idx);
                    for ry in 0..seg.height {
                        if is_inside_opening(&seg.openings, idx as u32, ry) {
                            continue;
                        }
                        let y = seg.base_y + ry as i32;
                        // Wood baseboard on the ground floor row + the vertical
                        // beams; white panels everywhere else.
                        let placer = if (ry == 0 && baseboard) || is_beam { &mut wood } else { &mut white };
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
