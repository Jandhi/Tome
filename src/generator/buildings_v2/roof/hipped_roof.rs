//! Hipped roofs: all four sides slope to a single ridge/apex, with the four
//! diagonal corner overhang cells curled upward (the Japanese eave).
//!
//! Per rect we build a hipped heightmap (see [`super::hipped::hipped_heightmap`])
//! and dispatch on [`HippedPitch`]:
//! - Slab — dedicated placer below that diverges from the shared slab pipeline
//!   in two places: the eave gets a single thin top slab (no stacked bottom
//!   slab cap) and the interior surface is dropped half a block so the eave
//!   reads as a clear lip.
//! - Stairs — reuses the shared [`super::blocks::place_roof_blocks`] with
//!   `GablePitch::Stairs`, giving the steeper full-block stair-step variant.

use std::collections::{BTreeMap, HashMap};

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Point2D, Point3D, Rect2D};
use crate::minecraft::BlockForm;
use crate::noise::RNG;

use super::super::frame::Frame;
use super::super::pipeline::BuildCtx;
use super::blocks::place_roof_blocks;
use super::gable::GablePitch;
use super::heightmap::RoofHeightmap;
use super::hipped::{HIPPED_OVERHANG, HippedPitch, hipped_heightmap};
use super::top_floor_rects;

pub(super) async fn place_hipped_roof(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
    pitch: HippedPitch,
) -> (Vec<Point2D>, Vec<RoofHeightmap>) {
    let editor: &Editor = &*ctx.editor;
    let data = ctx.data;
    let palette = ctx.palette;
    let rng = &mut *ctx.rng;

    let rects = top_floor_rects(frame);
    let rects = &rects[..];
    let overhang = HIPPED_OVERHANG;

    let per_rect_heightmaps: Vec<RoofHeightmap> = (0..rects.len())
        .map(|i| {
            let others: Vec<&Rect2D> = (0..rects.len())
                .filter(|&j| j != i)
                .map(|j| &rects[j])
                .collect();
            hipped_heightmap(&rects[i], &others, pitch)
        })
        .collect();

    let mut groups: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
    for i in 0..rects.len() {
        groups.entry(frame.roof_y(i)).or_default().push(i);
    }

    for (&raw_roof_y, group_indices) in &groups {
        // Stairs pitch sits one block lower than slabs at the same heightmap
        // value — match the offset gable_roof uses so the wall top lines up.
        let roof_y = match pitch {
            HippedPitch::Slab => raw_roof_y,
            HippedPitch::Stairs => raw_roof_y - 1,
        };

        let group_rects: Vec<&Rect2D> =
            group_indices.iter().map(|&i| &rects[i]).collect();

        let higher_rects: Vec<&Rect2D> = groups
            .iter()
            .filter(|(&ry, _)| ry > raw_roof_y)
            .flat_map(|(_, indices)| indices.iter().map(|&i| &rects[i]))
            .collect();

        let combined_min_x = group_rects.iter().map(|r| r.min().x).min().unwrap() - overhang;
        let combined_min_z = group_rects.iter().map(|r| r.min().y).min().unwrap() - overhang;
        let combined_max_x = group_rects.iter().map(|r| r.max().x).max().unwrap() + overhang;
        let combined_max_z = group_rects.iter().map(|r| r.max().y).max().unwrap() + overhang;
        let width = (combined_max_x - combined_min_x + 1) as usize;
        let depth = (combined_max_z - combined_min_z + 1) as usize;

        let mut combined_hm = RoofHeightmap::new(combined_min_x, combined_min_z, width, depth);
        for &i in group_indices {
            combined_hm.merge_max(&per_rect_heightmaps[i]);
        }

        match pitch {
            HippedPitch::Slab => {
                place_hipped_blocks(
                    editor, &combined_hm, roof_y, &group_rects, &higher_rects,
                    data, palette, rng,
                ).await;
            }
            HippedPitch::Stairs => {
                place_roof_blocks(
                    editor, &combined_hm, roof_y, GablePitch::Stairs,
                    &group_rects, &higher_rects, data, palette, rng,
                ).await;
            }
        }
    }

    (Vec::new(), per_rect_heightmaps)
}

/// Hipped-roof slab placer. Same surface logic as the shared slab-pitch
/// placer for interior cells, but the eave overhang gets only a single top
/// slab (the lower of the two slabs the shared placer would stack) so the
/// rim reads as a thin tile edge rather than a 1-block-thick lip.
pub(super) async fn place_hipped_blocks(
    editor: &Editor,
    hm: &RoofHeightmap,
    roof_y: i32,
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

    let bottom_slab_state = HashMap::from([("type".to_string(), "bottom".to_string())]);
    let top_slab_state = HashMap::from([("type".to_string(), "top".to_string())]);

    for x in hm.min_x()..=hm.max_x() {
        for z in hm.min_z()..=hm.max_z() {
            let h = hm.get(x, z);
            if h == f32::NEG_INFINITY {
                continue;
            }

            let p = Point2D::new(x, z);
            if higher_rects.iter().any(|r| r.contains(p)) {
                continue;
            }

            // Slab-pitch surface: shift down by half a block so the half-step
            // landings (frac=0.5) become top slabs and whole-step landings
            // become bottom slabs — the canonical slab-staircase pattern.
            let h_adj = h - 0.5;
            let y_floor = roof_y + h_adj.floor() as i32;
            let frac = h_adj - h_adj.floor();
            let is_overhang = !group_rects.iter().any(|r| r.contains(p));

            if is_overhang {
                // Cardinal eave: top slab in the upper half of y_floor - 1
                // (the raised tile-edge position). Lifted corners (h > 0)
                // instead get a bottom slab there — half a block lower than
                // their natural top-slab spot — so the four corners settle
                // just 0.5 above the cardinal rim for a subtle curl.
                let slab_state = if h > 0.0 { &bottom_slab_state } else { &top_slab_state };
                placer.place_block(
                    editor, Point3D::new(x, y_floor - 1, z),
                    BlockForm::Slab, Some(slab_state), None,
                ).await;
            } else {
                // Interior surface: shift h_adj down by an extra half block
                // so the whole non-eave roof sits 0.5 below the standard
                // slab-pitch surface, exposing the eave as a clear lip.
                let h_adj_low = h - 1.0;
                let y_floor_low = roof_y + h_adj_low.floor() as i32;
                let frac_low = h_adj_low - h_adj_low.floor();

                // Ceiling block at roof_y - 1 — only place it where the
                // surface sits strictly above it. Otherwise it's a full
                // block that the surface slab can't overwrite (density
                // check in Editor.place_block), and you end up with a
                // full-block lip at the inner eave instead of a slab.
                if y_floor_low > roof_y - 1 {
                    placer.place_block(
                        editor, Point3D::new(x, roof_y - 1, z),
                        BlockForm::Block, None, None,
                    ).await;
                }

                let slab_state = if frac_low >= 0.5 - f32::EPSILON {
                    &top_slab_state
                } else {
                    &bottom_slab_state
                };
                placer.place_block(
                    editor, Point3D::new(x, y_floor_low, z),
                    BlockForm::Slab, Some(slab_state), None,
                ).await;
            }
        }
    }
}
