use std::collections::{HashMap, HashSet};

use log::warn;

use crate::{
    editor::Editor,
    generator::{
        build_claim::BuildClaim,
        data::LoadedData,
        districts::{replace_ground, SuperDistrict},
        terrain::{log_trees, smooth_terrain},
    },
    geometry::{Point2D, Point3D},
    minecraft::Block,
    noise::RNG,
};

use super::production_painter::ProductionPainter;

/// Paints a production area across all unclaimed cells of `super_district` after
/// a gathering building has been placed there. The area is claimed with
/// `BuildClaim::ProductionArea` tied to the most-recently-placed structure on the world.
pub async fn paint_production_area(
    super_district: &SuperDistrict,
    painter_name: &str,
    data: &LoadedData,
    editor: &mut Editor,
    rng: &mut RNG,
) {
    let Some(painter) = data.resource_registry.production_painters.get(painter_name) else {
        warn!("paint_production_area: unknown painter '{}'", painter_name);
        return;
    };
    let painter = painter.clone();

    // Tie the production area to the most-recently placed structure.
    let Some(structure_id) = editor.world().structures.last().cloned() else {
        warn!("paint_production_area: no structure on world — was a building placed first?");
        return;
    };

    const EDGE_BUFFER: i32 = 3;

    // Build a set of cells within EDGE_BUFFER blocks (Chebyshev) of any edge cell.
    let edge_buffer: HashSet<Point2D> = super_district.data.edges.iter()
        .flat_map(|p| {
            let p2 = p.drop_y();
            (-EDGE_BUFFER..=EDGE_BUFFER).flat_map(move |dx| {
                (-EDGE_BUFFER..=EDGE_BUFFER).map(move |dz| Point2D::new(p2.x + dx, p2.y + dz))
            })
        })
        .collect();

    // Free cells: district interior excluding edge buffer, not yet claimed, not water.
    let free_cells: HashSet<Point2D> = super_district.data.points_2d.iter()
        .filter(|&&p| !edge_buffer.contains(&p))
        .filter(|&&p| !editor.world().is_claimed(p))
        .filter(|&&p| !editor.world().is_water(p))
        .copied()
        .collect();

    if free_cells.is_empty() {
        return;
    }

    match painter {
        ProductionPainter::Logging { percent } => {
            paint_logging(&free_cells, percent, &structure_id, editor, rng).await;
        }
        ProductionPainter::Palettes { palettes, border_palette, irrigation, flatten_strength } => {
            // Border cells: district interior points that fall within the edge buffer,
            // not yet claimed, not water.
            let border_cells: HashSet<Point2D> = super_district.data.points_2d.iter()
                .filter(|&&p| edge_buffer.contains(&p))
                .filter(|&&p| !editor.world().is_claimed(p))
                .filter(|&&p| !editor.world().is_water(p))
                .copied()
                .collect();

            paint_palettes(
                &free_cells,
                &border_cells,
                &palettes,
                border_palette.as_deref(),
                irrigation,
                flatten_strength,
                &structure_id,
                data,
                editor,
                rng,
            )
            .await;
        }
    }
}

async fn paint_logging(
    free_cells: &HashSet<Point2D>,
    percent: f32,
    structure_id: &crate::generator::nbts::StructureID,
    editor: &mut Editor,
    rng: &mut RNG,
) {
    // Find cells whose motion-blocking top block is a tree.
    let tree_cells: Vec<Point2D> = free_cells
        .iter()
        .filter(|&&p| {
            let height = editor.world().get_motion_blocking_height_at(p) - 1;
            editor.get_block(Point3D::new(p.x, height, p.y)).id.is_tree()
        })
        .copied()
        .collect();

    let count = ((tree_cells.len() as f32) * percent.clamp(0.0, 1.0)).round() as usize;
    let selected = rng.choose_many(&tree_cells, count);
    let selected_set: HashSet<Point2D> = selected.iter().map(|&&p| p).collect();

    // Capture stump positions and block types before logging removes the trees.
    // A cell is the canonical stump position for its trunk group if neither its
    // west nor north neighbour is also selected — this picks the top-left cell of
    // any multi-column trunk (2×2 dark oak, large jungle tree) without a full
    // flood-fill, so each trunk gets one stump instead of one per column.
    let stumps: Vec<(Point3D, Block)> = selected_set
        .iter()
        .filter(|&&p| {
            !selected_set.contains(&Point2D::new(p.x - 1, p.y)) &&
            !selected_set.contains(&Point2D::new(p.x, p.y - 1))
        })
        .map(|&p| {
            let stump_y = editor.world().get_non_tree_height(p);
            let block = editor.get_block(Point3D::new(p.x, stump_y, p.y));
            (Point3D::new(p.x, stump_y, p.y), block)
        })
        .collect();

    log_trees(editor, selected_set).await;

    for (pos, block) in stumps {
        editor.place_block(&block, pos).await;
    }

    // Claim all free cells for this production area.
    for &cell in free_cells {
        editor.world_mut().claim(cell, BuildClaim::ProductionArea(structure_id.clone()));
    }
}

async fn paint_palettes(
    free_cells: &HashSet<Point2D>,
    border_cells: &HashSet<Point2D>,
    palettes: &[String],
    border_palette: Option<&str>,
    irrigation: bool,
    flatten_strength: f32,
    structure_id: &crate::generator::nbts::StructureID,
    data: &LoadedData,
    editor: &mut Editor,
    rng: &mut RNG,
) {
    // Smooth terrain first so crop height offsets are consistent.
    if flatten_strength > 0.0 {
        smooth_terrain(free_cells, flatten_strength, editor).await;
    }

    // Paint the border strip with its palette before handling the field interior.
    if let Some(name) = border_palette {
        if !border_cells.is_empty() {
            if let Some(palette) = data.paint_palettes.get(&crate::generator::districts::PaintPaletteId(name.to_string())) {
                let (block_dict, block_list) = palette.to_weighted_blocks();
                replace_ground(border_cells, &block_dict, &block_list, rng, editor, None, None, Some(false)).await;
            } else {
                warn!("paint_production_area: unknown border palette '{}'", name);
            }
        }
    }

    // Split field cells into irrigation channels and crop rows.
    let (irrigation_cells, field_cells) = if irrigation {
        let axis_x = rng.rand_i32(2) == 0; // true = X axis, false = Z axis
        let offset = rng.rand_i32(5);
        let mut irr: HashSet<Point2D> = HashSet::new();
        let mut field: HashSet<Point2D> = HashSet::new();
        for &p in free_cells {
            let coord = if axis_x { p.x } else { p.y };
            if coord % 5 == offset {
                irr.insert(p);
            } else {
                field.insert(p);
            }
        }
        (irr, field)
    } else {
        (HashSet::new(), free_cells.clone())
    };

    // Place water channels.
    if !irrigation_cells.is_empty() {
        let water_dict: HashMap<usize, f32> = HashMap::from([(0, 1.0)]);
        let water_list: Vec<Block> = vec!["water".into()];
        replace_ground(
            &irrigation_cells,
            &water_dict,
            &water_list,
            rng,
            editor,
            None,
            None,
            Some(false),
        )
        .await;
    }

    // Apply each palette in order (ground layer before crop layer).
    for palette_name in palettes {
        let Some(palette) = data.paint_palettes.get(&crate::generator::districts::PaintPaletteId(palette_name.clone())) else {
            warn!("paint_production_area: unknown palette '{}'", palette_name);
            continue;
        };
        let (block_dict, block_list) = palette.to_weighted_blocks();
        let height_offset = if palette.has_tag("crops") { Some(1) } else { None };
        replace_ground(
            &field_cells,
            &block_dict,
            &block_list,
            rng,
            editor,
            height_offset,
            None,
            Some(false),
        )
        .await;
    }

    // Claim all painted cells for this production area.
    for &cell in free_cells.iter().chain(border_cells.iter()) {
        editor.world_mut().claim(cell, BuildClaim::ProductionArea(structure_id.clone()));
    }
}
