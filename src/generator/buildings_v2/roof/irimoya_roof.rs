//! Irimoya (hip-and-gable) roofs.
//!
//! The whole surface is rendered with the shared stepped placer
//! ([`super::blocks::place_roof_blocks`] at `GablePitch::Stairs`, exactly as the
//! hipped Stairs roof) over an [`super::irimoya`] heightmap whose central span
//! rises to a long ridge instead of a single apex. On top of that we fill the
//! two triangular gable-end pediments at the gable insets with wall material,
//! closing the ends of the ridge so it reads as a gable perched on a hip.

use std::collections::{BTreeMap, HashMap};

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Point2D, Point3D, Rect2D};
use crate::minecraft::{Block, BlockForm};
use crate::noise::RNG;

use super::super::frame::Frame;
use super::super::pipeline::BuildCtx;
use super::blocks::{is_ridge, place_roof_blocks, stair_facing};
use super::gable::GablePitch;
use super::heightmap::RoofHeightmap;
use super::hipped::HIPPED_OVERHANG;
use super::irimoya::{IRIMOYA_RISE, LongAxis, gable_inset, irimoya_heightmap, pick_long_axis, verge_depth};
use super::top_floor_rects;

pub(super) async fn place_irimoya_roof(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
) -> (Vec<Point2D>, Vec<RoofHeightmap>) {
    let editor: &Editor = &*ctx.editor;
    let data = ctx.data;
    let palette = ctx.palette;
    let rng = &mut *ctx.rng;

    let rects = top_floor_rects(frame);
    let rects = &rects[..];
    let overhang = HIPPED_OVERHANG;
    let rise = IRIMOYA_RISE;

    let axes: Vec<LongAxis> = rects.iter().map(pick_long_axis).collect();
    let insets: Vec<i32> = (0..rects.len()).map(|i| gable_inset(&rects[i], axes[i])).collect();

    let per_rect_heightmaps: Vec<RoofHeightmap> = (0..rects.len())
        .map(|i| {
            let others: Vec<&Rect2D> = (0..rects.len())
                .filter(|&j| j != i)
                .map(|j| &rects[j])
                .collect();
            irimoya_heightmap(&rects[i], &others, rise, axes[i], insets[i])
        })
        .collect();

    let mut groups: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
    for i in 0..rects.len() {
        groups.entry(frame.roof_y(i)).or_default().push(i);
    }

    for (&wall_top_y, group_indices) in &groups {
        // Stairs pitch sits one block lower than its heightmap value so the
        // wall top lines up — same offset the hipped Stairs roof applies.
        let roof_y = wall_top_y - 1;

        let group_rects: Vec<&Rect2D> = group_indices.iter().map(|&i| &rects[i]).collect();

        let higher_rects: Vec<&Rect2D> = groups
            .iter()
            .filter(|(&ry, _)| ry > wall_top_y)
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

        // Lower hipped skirt + the long central ridge, rendered as a stepped roof.
        place_roof_blocks(
            editor, &combined_hm, roof_y, GablePitch::Stairs, &group_rects, &higher_rects,
            data, palette, rng,
        )
        .await;

        // Triangular gable-end pediments closing the ridge + verge-overhang
        // brackets under the projecting gable lip on each rect.
        for &i in group_indices {
            place_gable_pediments(
                editor, &rects[i], axes[i], insets[i], rise, wall_top_y, roof_y, &higher_rects,
                data, palette, rng,
            )
            .await;
            place_verge_brackets(
                editor, &combined_hm, &rects[i], axes[i], insets[i], rise, roof_y, &higher_rects,
                data, palette, rng,
            )
            .await;
            let others: Vec<&Rect2D> = group_indices
                .iter()
                .filter(|&&j| j != i)
                .map(|&j| &rects[j])
                .chain(higher_rects.iter().copied())
                .collect();
            raise_corners(editor, &rects[i], &others, roof_y, data, palette, rng).await;
        }
    }

    (Vec::new(), per_rect_heightmaps)
}

/// Raise the four upturned (sorihafu) eave corners and sweep the eave up into
/// them. Each corner is rendered by [`place_roof_blocks`] as a ridge tile (bottom
/// slab at the eave level) over a full-block overhang bracket; the cardinal eave
/// cells beside it are stairs.
///
/// - Corner: shift the stack up half a block contiguously — the bracket full
///   block becomes a top slab and the tile becomes a full block (top surface +1.0).
/// - Eave cell adjacent to the corner: top slab + a bottom slab on top (+0.5).
/// - Eave cell next out: the stair flips to a top slab (+0.0).
///
/// This steps the eave up by half a block per cell into the curled corner.
/// Corners inside another rect aren't curled, so they're skipped (mirrors
/// [`super::irimoya::irimoya_heightmap`]).
async fn raise_corners(
    editor: &Editor,
    rect: &Rect2D,
    others: &[&Rect2D],
    roof_y: i32,
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

    let top_slab = HashMap::from([("type".to_string(), "top".to_string())]);
    let bottom_slab = HashMap::from([("type".to_string(), "bottom".to_string())]);
    let min = rect.min();
    let max = rect.max();
    let oh = HIPPED_OVERHANG;

    for (cx, cz) in [
        (min.x - oh, min.y - oh),
        (max.x + oh, min.y - oh),
        (min.x - oh, max.y + oh),
        (max.x + oh, max.y + oh),
    ] {
        let corner = Point2D::new(cx, cz);
        if others.iter().any(|r| r.contains(corner)) {
            continue;
        }
        // Corner: tile bottom slab (roof_y) -> full block, and clear the bracket
        // below it (roof_y - 1) to air so the curl tip floats at +1.0.
        placer
            .place_block_forced(editor, Point3D::new(cx, roof_y, cz), BlockForm::Block, None, None)
            .await;
        editor
            .place_block_forced(&Block::from_id("minecraft:air".into()), Point3D::new(cx, roof_y - 1, cz))
            .await;

        // Step into the corner along each eave (the cardinal overhang row/column).
        let sx = if cx < min.x { 1 } else { -1 };
        let sz = if cz < min.y { 1 } else { -1 };
        // Up-slope facing for each eave (toward the ridge / building interior).
        let x_facing = if sz == 1 { "south" } else { "north" };
        let z_facing = if sx == 1 { "east" } else { "west" };
        // (adjacent, next) cells along the X-eave (row z = cz) and Z-eave (col x = cx),
        // each with the in-edge coordinate to bounds-check and the eave facing.
        let edges = [
            (
                Point2D::new(cx + sx, cz),
                Point2D::new(cx + 2 * sx, cz),
                (cx + sx, cx + 2 * sx, min.x, max.x),
                x_facing,
            ),
            (
                Point2D::new(cx, cz + sz),
                Point2D::new(cx, cz + 2 * sz),
                (cz + sz, cz + 2 * sz, min.y, max.y),
                z_facing,
            ),
        ];
        for (adj, nxt, (adj_c, nxt_c, lo, hi), facing) in edges {
            // Adjacent eave cell: top slab + a bottom slab on top.
            if adj_c >= lo && adj_c <= hi && !others.iter().any(|r| r.contains(adj)) {
                placer
                    .place_block_forced(editor, Point3D::new(adj.x, roof_y - 1, adj.y), BlockForm::Slab, Some(&top_slab), None)
                    .await;
                placer
                    .place_block_forced(editor, Point3D::new(adj.x, roof_y, adj.y), BlockForm::Slab, Some(&bottom_slab), None)
                    .await;
            }
            // Next eave cell: a top-heavy (upside-down) stair, facing up-slope.
            if nxt_c >= lo && nxt_c <= hi && !others.iter().any(|r| r.contains(nxt)) {
                let inv_stair = HashMap::from([
                    ("facing".to_string(), facing.to_string()),
                    ("half".to_string(), "top".to_string()),
                ]);
                placer
                    .place_block_forced(editor, Point3D::new(nxt.x, roof_y - 1, nxt.y), BlockForm::Stairs, Some(&inv_stair), None)
                    .await;
            }
        }
    }
}

/// Fill the two triangular gable-end walls of a rect with wall material.
///
/// Each pediment sits at a gable inset plane (`along_min + inset` and
/// `along_max - inset`) and fills solid from the wall top (`wall_top_y`) up to
/// just below the roof surface, giving the classic gable triangle. The hipped
/// end cap leans against its lower outer face. Surface height matches the
/// stepped surface placed by [`place_roof_blocks`]: `roof_y + floor(h)`, where
/// `roof_y = wall_top_y - 1` for the Stairs pitch.
async fn place_gable_pediments(
    editor: &Editor,
    rect: &Rect2D,
    axis: LongAxis,
    inset: i32,
    rise: f32,
    wall_top_y: i32,
    roof_y: i32,
    higher_rects: &[&Rect2D],
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
) {
    // The gable pediment reads as a timber tympanum — wood, not the wall stone.
    let wall_material_id = palette
        .get_material(MaterialRole::PrimaryWood)
        .expect("No primary wood material")
        .clone();
    let mut placer_rng = rng.derive();
    let mut wall_placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng),
        wall_material_id,
    );

    let min = rect.min();
    let max = rect.max();

    let (along_min, along_max, across_min, across_max) = match axis {
        LongAxis::X => (min.x, max.x, min.y, max.y),
        LongAxis::Z => (min.y, max.y, min.x, max.x),
    };

    let along_ends = [along_min + inset, along_max - inset];
    if along_ends[0] > along_ends[1] {
        return; // gable span collapsed — nothing to close
    }

    let across_extent = across_max - across_min;
    let cap_h = ((across_extent / 2) as f32 * rise).floor();

    for &along_pos in &along_ends {
        for across_pos in across_min..=across_max {
            let (x, z) = match axis {
                LongAxis::X => (along_pos, across_pos),
                LongAxis::Z => (across_pos, along_pos),
            };
            let p = Point2D::new(x, z);
            if higher_rects.iter().any(|r| r.contains(p)) {
                continue;
            }

            let d_across = (across_pos - across_min).min(across_max - across_pos);
            let gable_h = (d_across as f32 * rise).min(cap_h);
            // Stepped surface sits here (matches place_roof_blocks at Stairs).
            let surface_y = roof_y + gable_h.floor() as i32;

            for y in wall_top_y..surface_y {
                wall_placer
                    .place_block(editor, Point3D::new(x, y, z), BlockForm::Block, None, None)
                    .await;
            }
        }
    }
}

/// Close the gable verge end and bracket its lip — the roof cells that project
/// one block past each pediment (at [`verge_depth`]). The verge cells sit inside
/// the footprint, so [`place_roof_blocks`] never flags them as overhang. For each
/// gable column we:
/// - fill full blocks from the wall top up to the lip, closing the open wedge
///   that otherwise shows under the projecting gable where it meets the hip; and
/// - reproduce the shared placer's Stairs-pitch overhang bracket along the lip:
///   an upside-down (`half=top`) stair facing down-slope, or a full block at the
///   ridge.
async fn place_verge_brackets(
    editor: &Editor,
    hm: &RoofHeightmap,
    rect: &Rect2D,
    axis: LongAxis,
    inset: i32,
    rise: f32,
    roof_y: i32,
    higher_rects: &[&Rect2D],
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
) {
    let depth = verge_depth(inset);
    if depth >= inset {
        return; // no overhang projects past the pediment
    }

    let material_id = palette
        .get_material(MaterialRole::PrimaryRoof)
        .expect("No primary roof material")
        .clone();
    let mut placer_rng = rng.derive();
    let mut placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng),
        material_id,
    );

    let min = rect.min();
    let max = rect.max();

    let (along_min, along_max, across_min, across_max) = match axis {
        LongAxis::X => (min.x, max.x, min.y, max.y),
        LongAxis::Z => (min.y, max.y, min.x, max.x),
    };

    let across_extent = across_max - across_min;
    let cap_h = ((across_extent / 2) as f32 * rise).floor();

    for &along_pos in &[along_min + depth, along_max - depth] {
        for across_pos in across_min..=across_max {
            let (x, z) = match axis {
                LongAxis::X => (along_pos, across_pos),
                LongAxis::Z => (across_pos, along_pos),
            };
            let p = Point2D::new(x, z);
            if higher_rects.iter().any(|r| r.contains(p)) {
                continue;
            }

            let d_across = (across_pos - across_min).min(across_max - across_pos);
            let gable_h = (d_across as f32 * rise).min(cap_h);
            // Only bracket cells whose lip is actually raised above the eave.
            if gable_h < 1.0 {
                continue;
            }
            let y_floor = roof_y + gable_h.floor() as i32;
            let bracket_y = y_floor - 1;

            // Close the gap between the hip roof and the gable verge by filling up
            // to where the hip stairs end at the verge boundary. The hip cap one
            // cell outside the verge tops out at min(depth - 1, d_across) * rise,
            // so the fill rises with the hip rather than sitting at a fixed row.
            let hip_end_h = ((depth - 1).min(d_across) as f32 * rise).min(cap_h);
            let hip_end_y = roof_y + hip_end_h.floor() as i32;
            for y in (roof_y + 1)..=hip_end_y {
                if y >= bracket_y {
                    break; // stop below the lip bracket / surface
                }
                placer
                    .place_block_forced(editor, Point3D::new(x, y, z), BlockForm::Block, None, None)
                    .await;
            }

            if is_ridge(hm, x, z) {
                // Full block beneath the ridge lip — same as the shared placer.
                placer
                    .place_block(editor, Point3D::new(x, bracket_y, z), BlockForm::Block, None, None)
                    .await;
            } else {
                // Upside-down stair facing down-slope, half=top.
                let facing = -stair_facing(hm, x, z);
                let inv_state = HashMap::from([
                    ("facing".to_string(), facing.to_string()),
                    ("half".to_string(), "top".to_string()),
                ]);
                placer
                    .place_block(editor, Point3D::new(x, bracket_y, z), BlockForm::Stairs, Some(&inv_state), None)
                    .await;
            }
        }
    }
}
