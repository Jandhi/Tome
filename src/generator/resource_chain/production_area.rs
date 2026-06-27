use std::collections::{HashMap, HashSet};

use log::warn;
use serde::Deserialize;

use crate::{
    editor::Editor,
    generator::{
        build_claim::BuildClaim,
        data::LoadedData,
        districts::{replace_ground, District, PaintPaletteId},
        materials::{MaterialRole, PaletteId},
        terrain::{feathered_flatten, group_trees, log_trees},
    },
    geometry::{cardinal_to_str, Point2D, Point3D, CARDINALS_2D, ALL_8},
    minecraft::{Block, BlockForm, BlockID},
    noise::RNG,
};

use super::production_painter::{parse_params, ProductionPainter};

/// Width (Chebyshev cells) of the buffer around a production district's edge.
/// Excluded from the field interior; used as the border strip and feather band.
/// The rural road network reuses this strip via [`border_ring_cells`].
const EDGE_BUFFER: i32 = 3;

/// How far (cells) production-area smoothing reaches into neighbouring land, to
/// feather the field's terrain into its surroundings rather than ending in a step.
const NEIGHBOUR_REACH: i32 = 2;

/// Cells within `EDGE_BUFFER` (Chebyshev) of any of `district`'s edge cells.
fn edge_buffer_cells(district: &District) -> HashSet<Point2D> {
    district.data.edges.iter()
        .flat_map(|p| {
            let p2 = p.drop_y();
            (-EDGE_BUFFER..=EDGE_BUFFER).flat_map(move |dx| {
                (-EDGE_BUFFER..=EDGE_BUFFER).map(move |dz| Point2D::new(p2.x + dx, p2.y + dz))
            })
        })
        .collect()
}

/// The production area's border ring: interior cells within `EDGE_BUFFER` of an
/// edge that are unclaimed and not water — the strip a border-painting painter
/// paves with `rural_road`. The single source of truth shared with
/// [`crate::generator::paths::rural`], so the road network's *predicted* ring and
/// the painter's *actual* ring stay in lockstep. The result depends on claim state
/// at call time: the road network calls this before the roads are claimed (to
/// route along the ring); the painter calls it after (so road-claimed cells are
/// already excluded).
pub fn border_ring_cells(district: &District, editor: &Editor) -> HashSet<Point2D> {
    let edge_buffer = edge_buffer_cells(district);
    district.data.points_2d.iter()
        .filter(|&&p| edge_buffer.contains(&p))
        .filter(|&&p| !editor.world().is_claimed(p))
        .filter(|&&p| !editor.world().is_water(p))
        .copied()
        .collect()
}

/// Paints a production area across all unclaimed cells of `district` after
/// a gathering building has been placed there. The area is claimed with
/// `BuildClaim::ProductionArea` tied to the most-recently-placed structure on the world.
///
/// Use [`paint_production_area_for`] when the painting pass is decoupled from
/// placement (e.g. all buildings placed first, then all areas painted): with the
/// passes split, `structures.last()` no longer identifies *this* district's
/// building, so the caller must pass the structure id explicitly.
pub async fn paint_production_area(
    district: &District,
    painter_name: &str,
    resource: &str,
    data: &LoadedData,
    editor: &mut Editor,
    rng: &mut RNG,
) {
    // Tie the production area to the most-recently placed structure.
    let Some(structure_id) = editor.world().structures.last().cloned() else {
        warn!("paint_production_area: no structure on world — was a building placed first?");
        return;
    };
    paint_production_area_for(district, painter_name, resource, &structure_id, data, editor, rng).await;
}

/// As [`paint_production_area`], but the production area is tied to an explicitly
/// supplied `structure_id` rather than `structures.last()`. Needed when buildings
/// are all placed in one pass and their areas painted in a later pass (so the
/// "last placed" structure is no longer the one for this district).
pub async fn paint_production_area_for(
    district: &District,
    painter_name: &str,
    resource: &str,
    structure_id: &crate::generator::nbts::StructureID,
    data: &LoadedData,
    editor: &mut Editor,
    rng: &mut RNG,
) {
    let Some(painter) = data.resource_registry.production_painters.get(painter_name) else {
        warn!("paint_production_area: unknown painter '{}'", painter_name);
        return;
    };
    let painter = painter.clone();
    let structure_id = structure_id.clone();

    // Build a set of cells within EDGE_BUFFER blocks (Chebyshev) of any edge cell.
    let edge_buffer = edge_buffer_cells(district);

    // Free cells: parcel interior excluding edge buffer, not yet claimed, not water.
    let raw_free_cells: HashSet<Point2D> = district.data.points_2d.iter()
        .filter(|&&p| !edge_buffer.contains(&p))
        .filter(|&&p| !editor.world().is_claimed(p))
        .filter(|&&p| !editor.world().is_water(p))
        .copied()
        .collect();

    // Smooth the field shape before painting: a morphological opening shaves off
    // the thin strips and frayed protrusions that make the painted area's edge
    // look ragged. Cells it removes simply stay natural terrain. Fall back to the
    // raw set if opening would erase a genuinely small-but-valid field.
    let smoothed = smooth_region(&raw_free_cells);
    let free_cells = if smoothed.is_empty() { raw_free_cells } else { smoothed };

    if free_cells.is_empty() {
        return;
    }

    // Border cells: parcel interior points that fall within the edge buffer, not
    // yet claimed, not water. Painted with the border palette (e.g. rural_road) by
    // both the palette and function painters. Shared with the rural road network's
    // ring prediction (see `border_ring_cells`).
    let border_cells = border_ring_cells(district, editor);

    // How many worker posts this building wants — the gather painters record that
    // many dynamic anchors at the features they create (a woodcutter by a stump, a
    // miner by an outcrop), so the rural crew stands at its work rather than
    // clustered at the hut door. Capped against any anchors already recorded
    // (interior anchors on ranch/farm), so a building never overstaffs.
    let worker_budget = data
        .npc_data
        .staffing_for(&structure_id.structure_type.0, &data.structures)
        .workers;

    // Centroid of this building's footprint (cells it claimed), used to keep mine
    // workers near the building rather than off at a distant outcrop.
    let building_centre = {
        let (mut sx, mut sz, mut n) = (0i64, 0i64, 0i64);
        for &p in &district.data.points_2d {
            if let Some(BuildClaim::Structure(sid)) = editor.world().get_claim(p) {
                if sid.id == structure_id.id {
                    sx += p.x as i64;
                    sz += p.y as i64;
                    n += 1;
                }
            }
        }
        (n > 0).then(|| Point2D::new((sx / n) as i32, (sz / n) as i32))
    };

    match painter {
        ProductionPainter::Palettes { palettes, border_palette, irrigation, flatten_strength } => {
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
        ProductionPainter::Function { function, params } => {
            // Dispatch to the named painter function, handing it the params map.
            match function.as_str() {
                "logging_production_painter" => {
                    logging_production_painter(&params, &free_cells, worker_budget, &structure_id, editor, rng).await;
                }
                "pasture_production_painter" => {
                    pasture_production_painter(&params, &free_cells, &border_cells, worker_budget, &structure_id, data, editor, rng).await;
                }
                "sugarcane_production_painter" => {
                    sugarcane_production_painter(&params, &free_cells, &border_cells, worker_budget, &structure_id, data, editor, rng).await;
                }
                "bee_area_production_painter" => {
                    bee_area_production_painter(&params, &free_cells, worker_budget, &structure_id, data, editor, rng).await;
                }
                "mine_production_painter" => {
                    mine_production_painter(&params, &free_cells, resource, worker_budget, building_centre, &structure_id, data, editor, rng).await;
                }
                other => {
                    warn!("paint_production_area: unknown painter function '{}'", other);
                }
            }
        }
    }
}

/// A worker post on the ground cardinally adjacent to `feature`: an open,
/// in-bounds, non-water cell beside it, at its surface height. `None` when the
/// feature is boxed in (no open neighbour). Used by the gather painters to seat a
/// worker beside the thing it tends.
fn stand_beside_feature(feature: Point2D, editor: &Editor) -> Option<Point3D> {
    for d in CARDINALS_2D {
        let n = feature + d;
        if !editor.world().is_in_bounds_2d(n) || editor.world().is_water(n) {
            continue;
        }
        let open = matches!(
            editor.world().get_claim(n),
            None | Some(BuildClaim::None)
                | Some(BuildClaim::Nature)
                | Some(BuildClaim::ProductionArea(_))
        );
        if !open {
            continue;
        }
        if let Some(y) = editor.world().get_ocean_floor_height_at(n) {
            return Some(Point3D::new(n.x, y, n.y));
        }
    }
    None
}

/// Record up to `budget` dynamic worker anchors standing beside `feature_cells`
/// (each facing the feature) for this production building, appending to any
/// anchors it already has and never exceeding `budget` total. The settlement
/// worker pass then stands the crew at these spots (see
/// `World::structure_anchors`). Features should already be passed in a spread,
/// deterministic order; the first `budget` that yield an open neighbour are used.
fn record_feature_anchors(
    feature_cells: &[Point2D],
    budget: usize,
    structure_id: &crate::generator::nbts::StructureID,
    editor: &mut Editor,
) {
    use crate::generator::population::yaw_toward;
    if budget == 0 {
        return;
    }
    let mut posts: Vec<(Point3D, f32)> = Vec::new();
    for &feat in feature_cells {
        if posts.len() >= budget {
            break;
        }
        if let Some(feet) = stand_beside_feature(feat, editor) {
            let look = Point3D::new(feat.x, feet.y, feat.y);
            posts.push((feet, yaw_toward(feet, look)));
        }
    }
    if posts.is_empty() {
        return;
    }
    let entry = editor
        .world_mut()
        .structure_anchors
        .entry(structure_id.id)
        .or_default();
    let remaining = budget.saturating_sub(entry.len());
    for p in posts.into_iter().take(remaining) {
        entry.push(p);
    }
}

/// Fells a fraction of the trees standing on the production area, leaving one
/// stump per trunk, to evoke a worked logging clearing. Cells are grouped into
/// whole trees first (see `group_trees`) so the fraction applies per tree.
///
/// Params: `percent` (f32, 0.0–1.0) — fraction of trees to fell.
async fn logging_production_painter(
    params: &serde_yaml::Value,
    free_cells: &HashSet<Point2D>,
    worker_budget: usize,
    structure_id: &crate::generator::nbts::StructureID,
    editor: &mut Editor,
    rng: &mut RNG,
) {
    #[derive(Deserialize)]
    struct Params {
        /// Fraction of tree-topped cells to fell, 0.0–1.0.
        percent: f32,
    }
    let Params { percent } = match parse_params(params) {
        Ok(p) => p,
        Err(e) => {
            warn!("logging_production_painter: invalid params: {}", e);
            return;
        }
    };

    // Group tree-topped cells into whole trees (trunk + canopy footprint) so
    // `percent` selects a fraction of *trees*, not of scattered leaf columns.
    let trees = group_trees(free_cells, editor);

    let count = ((trees.len() as f32) * percent.clamp(0.0, 1.0)).round() as usize;
    let selected = rng.choose_many(&trees, count);

    // Capture one stump (the trunk's base log) per selected tree before felling,
    // and gather every column to clear.
    let mut to_log: HashSet<Point2D> = HashSet::new();
    let mut stumps: Vec<(Point3D, Block)> = Vec::with_capacity(selected.len());
    for tree in &selected {
        let Some(stump_y) = editor.world().get_non_tree_height(tree.trunk) else {
            continue;
        };
        let stump_pos = tree.trunk.add_y(stump_y);
        stumps.push((stump_pos, editor.get_block(stump_pos)));
        to_log.extend(tree.cells.iter().copied());
    }

    log_trees(editor, to_log).await;

    // Stand the woodcutters at fresh stumps: collect the stump cells before the
    // placing loop consumes `stumps`, then seat a worker beside a spread of them.
    let stump_cells: Vec<Point2D> = stumps.iter().map(|(p, _)| p.drop_y()).collect();

    for (pos, block) in stumps {
        editor.place_block(&block, pos).await;
    }

    // Claim all free cells for this production area.
    for &cell in free_cells {
        editor.world_mut().claim(cell, BuildClaim::ProductionArea(structure_id.clone()));
    }

    // A woodcutter posted at a stump (a spread sample, so they don't bunch up).
    let chosen: Vec<Point2D> = rng
        .choose_many(&stump_cells, worker_budget.min(stump_cells.len()))
        .into_iter()
        .copied()
        .collect();
    record_feature_anchors(&chosen, worker_budget, structure_id, editor);
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
    feather_smooth(free_cells, border_cells, flatten_strength, editor).await;

    // Paint the border strip with its palette before handling the field interior.
    paint_border(border_cells, border_palette, data, editor, rng).await;

    // Split field cells into irrigation channels and crop rows.
    let (irrigation_cells, field_cells) = if irrigation {
        let axis_x = rng.rand_i32(2) == 0; // true = X axis, false = Z axis
        let offset = rng.rand_i32(5);
        let mut irr: HashSet<Point2D> = HashSet::new();
        let mut field: HashSet<Point2D> = HashSet::new();
        for &p in free_cells {
            let coord = if axis_x { p.x } else { p.y };
            if coord.rem_euclid(5) == offset {
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
            true, // replace the surface block with water
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
        let is_crop = palette.has_tag("crops");
        let height_offset = if is_crop { Some(1) } else { None };
        replace_ground(
            &field_cells,
            &block_dict,
            &block_list,
            rng,
            editor,
            height_offset,
            None,
            Some(false),
            // Ground layer replaces the (equally-dense) surface block, so force it.
            // Crop layer sits in the air cell above — leave unforced so it still
            // yields to any denser block already there.
            !is_crop,
        )
        .await;
    }

    // Claim all painted cells for this production area.
    for &cell in free_cells.iter().chain(border_cells.iter()) {
        editor.world_mut().claim(cell, BuildClaim::ProductionArea(structure_id.clone()));
    }
}

/// Feathered terrain smoothing over a production district: spans the field
/// interior, its border ring, and a couple of blocks into neighbouring land,
/// grading back to natural terrain at the outer edge so the area melts into its
/// surroundings instead of leaving a step under the border strip. No-op when
/// `flatten_strength` is 0.
async fn feather_smooth(
    free_cells: &HashSet<Point2D>,
    border_cells: &HashSet<Point2D>,
    flatten_strength: f32,
    editor: &mut Editor,
) {
    if flatten_strength <= 0.0 {
        return;
    }
    let smooth_iters = (flatten_strength.clamp(0.0, 1.0) * 5.0).round() as usize;
    if smooth_iters == 0 {
        return;
    }

    // The production district's own cells (interior + border ring).
    let mut region: HashSet<Point2D> =
        free_cells.iter().chain(border_cells.iter()).copied().collect();

    // Reach a couple blocks into neighbouring land, skipping water and anything
    // already claimed (buildings, walls, other production areas).
    let own_cells = region.clone();
    for &p in &own_cells {
        for dx in -NEIGHBOUR_REACH..=NEIGHBOUR_REACH {
            for dz in -NEIGHBOUR_REACH..=NEIGHBOUR_REACH {
                let q = Point2D::new(p.x + dx, p.y + dz);
                if own_cells.contains(&q) {
                    continue;
                }
                if editor.world().is_in_bounds_2d(q)
                    && !editor.world().is_water(q)
                    && !editor.world().is_claimed(q)
                {
                    region.insert(q);
                }
            }
        }
    }

    // Feather spans the border ring plus the neighbour reach, so the field
    // interior reaches full smoothing while the skirt grades to natural.
    feathered_flatten(editor, &region, EDGE_BUFFER + NEIGHBOUR_REACH, smooth_iters, true).await;
}

/// Paints the border ring with the named paint palette (e.g. `rural_road`),
/// forcing placement so it replaces the equally-dense surface block.
async fn paint_border(
    border_cells: &HashSet<Point2D>,
    border_palette: Option<&str>,
    data: &LoadedData,
    editor: &mut Editor,
    rng: &mut RNG,
) {
    let Some(name) = border_palette else { return };
    if border_cells.is_empty() {
        return;
    }
    let Some(palette) = data.paint_palettes.get(&PaintPaletteId(name.to_string())) else {
        warn!("paint_production_area: unknown border palette '{}'", name);
        return;
    };
    let (block_dict, block_list) = palette.to_weighted_blocks();
    replace_ground(border_cells, &block_dict, &block_list, rng, editor, None, None, Some(false), true).await;
}

/// Resolves the fence and fence-gate blocks from a building palette's primary
/// wood material, falling back to oak if the palette or material is missing.
fn resolve_fence_blocks(palette_name: &str, data: &LoadedData, rng: &mut RNG) -> (Block, BlockID) {
    let palette = data.palettes.get(&PaletteId::from(palette_name));
    let fence = palette
        .and_then(|p| p.get_block(MaterialRole::PrimaryWood, &BlockForm::Fence, &data.materials, rng).cloned())
        .unwrap_or_else(|| "minecraft:oak_fence".into());
    let gate = palette
        .and_then(|p| p.get_block(MaterialRole::PrimaryWood, &BlockForm::FenceGate, &data.materials, rng).cloned())
        .unwrap_or_else(|| "minecraft:oak_fence_gate".into());
    (Block::from_id(fence), gate)
}

/// Whether sugar cane can stand on `id` (grass/dirt/sand family). Sandstone is
/// explicitly excluded since `contains("sand")` would otherwise match it.
fn can_support_sugar_cane(id: &BlockID) -> bool {
    let s = id.as_str();
    if s.contains("sandstone") {
        return false;
    }
    const SOILS: [&str; 11] = [
        "grass_block", "dirt", "coarse_dirt", "rooted_dirt", "podzol", "mycelium",
        "moss_block", "mud", "muddy_mangrove_roots", "sand", "red_sand",
    ];
    SOILS.iter().any(|k| s.contains(k))
}

/// Soil to substitute when the current surface block can't support cane: sand on
/// sandy ground (to match the surroundings), grass elsewhere.
fn fallback_cane_soil(current: &Block) -> Block {
    let s = current.id.as_str();
    if s.contains("sand") || s.contains("sandstone") {
        Block::from_id("minecraft:sand".into())
    } else {
        Block::from_id("minecraft:grass_block".into())
    }
}

/// Whether `block` is a full solid block that can wall in / floor a water source
/// (so the source can't flow). Air, water, and plants are not.
fn is_solid_support(block: &Block) -> bool {
    !block.id.is_water() && BlockForm::infer_from_block(&block.id).density() >= 1.0
}

/// A field of sugar cane at varied growth stages, irrigated by contained water
/// channels. The block under each cane is left as-is where it already supports
/// cane (matching the existing dirt/sand), only swapped for soil where it can't.
///
/// Water safety: every placed water source is fully boxed by solid blocks (4
/// sides at its level + the floor), so it can never flow. Channel cells that
/// can't be sealed — at terrace steps or the field edge — are demoted to plain
/// soil (iteratively, so demotions wall in their neighbours), and cane is only
/// placed where a same-level water neighbour exists. The soil bed is committed
/// before any water is placed so flow ticks never fire over open ground.
///
/// Params:
/// - `border_palette` (string, default `rural_road`).
/// - `flatten_strength` (f32, default 0.7) — feathered smoothing; flatter ground
///   means more of the field can hold contained water.
/// - `water_spacing` (i32, default 3) — one water row per N rows.
/// - `min_height` / `max_height` (u32, default 1/3) — cane column height range.
async fn sugarcane_production_painter(
    params: &serde_yaml::Value,
    free_cells: &HashSet<Point2D>,
    border_cells: &HashSet<Point2D>,
    worker_budget: usize,
    structure_id: &crate::generator::nbts::StructureID,
    data: &LoadedData,
    editor: &mut Editor,
    rng: &mut RNG,
) {
    #[derive(Deserialize)]
    struct Params {
        #[serde(default = "default_border_palette")]
        border_palette: String,
        #[serde(default = "default_cane_flatten")]
        flatten_strength: f32,
        #[serde(default = "default_water_spacing")]
        water_spacing: i32,
        #[serde(default = "default_min_height")]
        min_height: u32,
        #[serde(default = "default_max_height")]
        max_height: u32,
    }
    fn default_border_palette() -> String { "rural_road".to_string() }
    fn default_cane_flatten() -> f32 { 0.7 }
    fn default_water_spacing() -> i32 { 3 }
    fn default_min_height() -> u32 { 1 }
    fn default_max_height() -> u32 { 3 }

    let p: Params = match parse_params(params) {
        Ok(p) => p,
        Err(e) => {
            warn!("sugarcane_production_painter: invalid params: {}", e);
            return;
        }
    };
    let spacing = p.water_spacing.max(2);

    feather_smooth(free_cells, border_cells, p.flatten_strength, editor).await;
    paint_border(border_cells, Some(p.border_palette.as_str()), data, editor, rng).await;

    // Deterministic cell order for any pass that draws from `rng`.
    let mut ordered: Vec<Point2D> = free_cells.iter().copied().collect();
    ordered.sort_by_key(|p| (p.x, p.y));

    // 1. Soil bed — every cell gets a cane-supportable solid surface, keeping the
    //    existing block where it already qualifies.
    for &c in &ordered {
        let Some(ch) = editor.world().get_non_tree_height(c) else {
            continue;
        };
        let y = ch - 1;
        let pos = Point3D::new(c.x, y, c.y);
        let current = editor.get_block(pos);
        if !can_support_sugar_cane(&current.id) {
            editor.place_block_forced(&fallback_cane_soil(&current), pos).await;
        }
    }
    // Commit the soil walls before any water exists (no transient flow on flush).
    editor.flush_buffer().await;

    // 2. Designate water rows by a striped pattern, then iteratively demote any
    //    cell that can't be sealed until the set is stable.
    let axis_x = rng.rand_i32(2) == 0;
    let offset = rng.rand_i32(spacing);
    let mut water_set: HashSet<Point2D> = free_cells
        .iter()
        .filter(|&&c| (if axis_x { c.x } else { c.y }).rem_euclid(spacing) == offset)
        .copied()
        .collect();

    loop {
        let mut demote: Vec<Point2D> = Vec::new();
        for &w in &water_set {
            let Some(wh) = editor.world().get_non_tree_height(w) else {
                demote.push(w);
                continue;
            };
            let wy = wh - 1;
            // Floor must be solid.
            if !is_solid_support(&editor.get_block(Point3D::new(w.x, wy - 1, w.y))) {
                demote.push(w);
                continue;
            }
            // Every side must be a solid wall, or a same-level water neighbour.
            let boxed = CARDINALS_2D.iter().all(|&d| {
                let n = w + d;
                if is_solid_support(&editor.get_block(Point3D::new(n.x, wy, n.y))) {
                    return true;
                }
                water_set.contains(&n)
                    && editor.world().get_non_tree_height(n).is_some_and(|nh| nh - 1 == wy)
            });
            if !boxed {
                demote.push(w);
            }
        }
        if demote.is_empty() {
            break;
        }
        for w in demote {
            water_set.remove(&w);
        }
    }

    // 3. Place the validated (boxed) water.
    let water_block: Block = "water".into();
    for &w in &water_set {
        let Some(wh) = editor.world().get_non_tree_height(w) else {
            continue;
        };
        let wy = wh - 1;
        editor.place_block_forced(&water_block, Point3D::new(w.x, wy, w.y)).await;
    }
    editor.flush_buffer().await;

    // 4. Cane columns on non-water cells that have a same-level water neighbour,
    //    at varied heights (mostly 2–3 tall) with a random age on the top block.
    let height_weights: HashMap<usize, f32> =
        HashMap::from([(1usize, 0.15f32), (2, 0.45), (3, 0.40)]);
    let min_h = p.min_height.max(1) as usize;
    let max_h = p.max_height.max(min_h as u32) as usize;

    let mut cane_cells: Vec<Point2D> = Vec::new();
    for &c in &ordered {
        if water_set.contains(&c) {
            continue;
        }
        let Some(cy) = editor.world().get_non_tree_height(c) else {
            continue;
        }; // base air cell of the column
        let support_y = cy - 1;
        let has_water = CARDINALS_2D.iter().any(|&d| {
            let n = c + d;
            water_set.contains(&n)
                && editor.world().get_non_tree_height(n).is_some_and(|nh| nh - 1 == support_y)
        });
        if !has_water {
            continue;
        }

        let height = (*rng.choose_weighted(&height_weights)).clamp(min_h, max_h);
        for i in 0..height {
            let age = if i == height - 1 { rng.rand_i32_range(0, 16) } else { 0 };
            let cane = Block::new(
                "minecraft:sugar_cane".into(),
                Some(HashMap::from([("age".to_string(), age.to_string())])),
                None,
            );
            editor.place_block(&cane, Point3D::new(c.x, cy + i as i32, c.y)).await;
        }
        cane_cells.push(c);
    }

    // 5. Claim every free cell for this production area.
    for &cell in free_cells {
        editor.world_mut().claim(cell, BuildClaim::ProductionArea(structure_id.clone()));
    }

    // A planter posted beside the cane (a spread sample), tending the crop.
    let chosen: Vec<Point2D> = rng
        .choose_many(&cane_cells, worker_budget.min(cane_cells.len()))
        .into_iter()
        .copied()
        .collect();
    record_feature_anchors(&chosen, worker_budget, structure_id, editor);
}

/// Builds beehive block-entity NBT filled with three bees, each given a random
/// (visible) funny name from `bee_names` — decorated with the same ~10% prefix /
/// ~10% suffix system as pasture animals (e.g. "Sir Buzz", "Beeyonce the Great").
///
/// Uses the snake_case `bees` / `entity_data` / `min_ticks_in_hive` /
/// `ticks_in_hive` tag names introduced in 1.21.4 — the old capitalized
/// `Bees`/`EntityData`/`MinOccupationTicks`/`TicksInHive` form is silently
/// dropped on current servers, leaving the nest empty. `min_ticks_in_hive` is a
/// positive value so the bees actually reside in the nest on placement (rather
/// than emerging on the first tick); they'll buzz the canopy on their own once
/// the chunk is loaded. Tweak here if a Minecraft version changes the format.
fn beehive_nbt(bee_names: &[String], prefixes: &[String], suffixes: &[String], rng: &mut RNG) -> String {
    const BEE_COUNT: usize = 3;
    // Minecraft's default minimum occupation; keeps bees in the nest until they
    // leave to pollinate, so a freshly-placed nest reads as populated.
    const MIN_TICKS_IN_HIVE: i32 = 600;
    // Distinct base names per hive where possible; fewer/none if the list is short/empty.
    let chosen: Vec<String> = rng.choose_many(bee_names, BEE_COUNT).into_iter().cloned().collect();

    let bees: Vec<String> = (0..BEE_COUNT)
        .map(|i| {
            let entity = match chosen.get(i) {
                Some(base) => {
                    let name = decorate_name(base, prefixes, suffixes, rng);
                    format!("{{id:\"minecraft:bee\",{}}}", custom_name_snbt(&name))
                }
                None => "{id:\"minecraft:bee\"}".to_string(),
            };
            format!(
                "{{entity_data:{},min_ticks_in_hive:{},ticks_in_hive:0}}",
                entity, MIN_TICKS_IN_HIVE
            )
        })
        .collect();

    format!("{{id:\"minecraft:beehive\",bees:[{}]}}", bees.join(","))
}

/// Finds a nest site on `trunk`'s log column: a cell cardinally adjacent to a log
/// (1 block away), itself air or leaves (so we don't carve the stem), tucked just
/// under the leaf canopy. Returns `(position, facing)` where `facing` points away
/// from the trunk. `None` if the tree has no such nook.
///
/// Searches from the bottom of the trunk *up*, taking the lowest qualifying spot,
/// so the hive ends up nestled against the trunk at the underside of the canopy
/// rather than perched on top of it. A spot qualifies if it is sheltered by a leaf
/// directly above (the canopy roof) — hanging beneath a leaf is fine even without
/// neighbouring leaves.
fn find_hive_spot(trunk: Point2D, editor: &Editor) -> Option<(Point3D, Point2D)> {
    let base_y = editor.world().get_non_tree_height(trunk)?;

    // Walk up the trunk's logs to find the top of the stem.
    let mut top_y = base_y;
    let mut y = base_y;
    while editor.get_block(trunk.add_y(y)).id.is_log() {
        top_y = y;
        y += 1;
    }

    for ly in base_y..=top_y {
        for d in CARDINALS_2D {
            let pos = Point3D::new(trunk.x + d.x, ly, trunk.y + d.y);
            let here = editor.get_block(pos).id;
            if !(here.is_air() || here.is_leaves()) {
                continue; // don't carve into a branch or neighbouring trunk
            }
            // Sheltered by the canopy roof directly overhead.
            let above = editor.get_block(Point3D::new(pos.x, pos.y + 1, pos.z)).id;
            if above.is_leaves() {
                return Some((pos, d));
            }
        }
    }
    None
}

/// Builds a populated beehive facing `facing`, with a random honey level and
/// randomly-named, prefix/suffix-decorated bees.
fn make_beehive(
    facing: Point2D,
    bee_names: &[String],
    prefixes: &[String],
    suffixes: &[String],
    rng: &mut RNG,
) -> Block {
    let state = HashMap::from([
        ("facing".to_string(), cardinal_to_str(&facing).unwrap_or_else(|| "north".to_string())),
        ("honey_level".to_string(), rng.rand_i32_range(0, 6).to_string()),
    ]);
    // `bee_nest` is the naturally-generated variant — looks at home hanging in a
    // tree canopy (the crafted `beehive` looks man-made). Both share the same
    // `minecraft:beehive` block-entity type, so the NBT `id` stays `beehive`.
    Block::new("minecraft:bee_nest".into(), Some(state), Some(beehive_nbt(bee_names, prefixes, suffixes, rng)))
}

/// Hangs a populated beehive in the canopy of a percentage of the area's trees —
/// beneath leaves, one block from a log. Uses the trunk-anchored tree recognition
/// (`group_trees`) so each tree is considered once.
///
/// Params: `percent` (f32, 0.0–1.0, default 0.3) — fraction of trees to nest.
async fn bee_area_production_painter(
    params: &serde_yaml::Value,
    free_cells: &HashSet<Point2D>,
    worker_budget: usize,
    structure_id: &crate::generator::nbts::StructureID,
    data: &LoadedData,
    editor: &mut Editor,
    rng: &mut RNG,
) {
    #[derive(Deserialize)]
    struct Params {
        #[serde(default = "default_hive_percent")]
        percent: f32,
    }
    fn default_hive_percent() -> f32 { 0.3 }

    let p: Params = match parse_params(params) {
        Ok(p) => p,
        Err(e) => {
            warn!("bee_area_production_painter: invalid params: {}", e);
            return;
        }
    };

    let trees = group_trees(free_cells, editor);
    let count = ((trees.len() as f32) * p.percent.clamp(0.0, 1.0)).round() as usize;
    let selected = rng.choose_many(&trees, count);
    let reg = &data.resource_registry;
    let bee_names = &reg.bee_names;
    let prefixes = &reg.animal_name_prefixes;
    let suffixes = &reg.animal_name_suffixes;

    // Trunks that actually got a hive — the beekeepers tend these.
    let mut hive_trunks: Vec<Point2D> = Vec::new();
    for tree in &selected {
        if let Some((pos, facing)) = find_hive_spot(tree.trunk, editor) {
            let hive = make_beehive(facing, bee_names, prefixes, suffixes, rng);
            // Forced so it can take a leaf cell as well as an air pocket.
            editor.place_block_forced(&hive, pos).await;
            hive_trunks.push(tree.trunk);
        }
    }

    // Claim all free cells for this production area.
    for &cell in free_cells {
        editor.world_mut().claim(cell, BuildClaim::ProductionArea(structure_id.clone()));
    }

    // A beekeeper posted at the foot of a hive tree (a spread sample).
    let chosen: Vec<Point2D> = rng
        .choose_many(&hive_trunks, worker_budget.min(hive_trunks.len()))
        .into_iter()
        .copied()
        .collect();
    record_feature_anchors(&chosen, worker_budget, structure_id, editor);
}

// --- Mine painter tunables (edit freely to change the look) ---
/// Per-cell chance, in parts-per-1000, of seeding a rock outcrop.
const MINE_BOULDER_CHANCE_PERMILLE: i32 = 15;
/// Miners are only posted at outcrops within this many blocks of the mine
/// building, so the crew stays by the mine instead of scattering across the dig.
const MINE_WORKER_RADIUS: i32 = 20;
/// Per-cell chance, in parts-per-1000, of an ore block poking through the surface.
const MINE_TERRAIN_ORE_PERMILLE: i32 = 15;
/// Percent of outcrops that carry ore.
const MINE_ORE_BOULDER_PERCENT: i32 = 30;
/// Within an ore-bearing outcrop, percent of blocks that are ore (vs rock).
const MINE_BOULDER_ORE_PERCENT: i32 = 16;
/// Outcrop horizontal radius and vertical height, in blocks.
const MINE_BOULDER_MAX_RADIUS: i32 = 2;
const MINE_BOULDER_MAX_HEIGHT: i32 = 3;
/// Local-rock sampling: how many cells to probe, and how far down each (blocks).
const MINE_GEOLOGY_SAMPLES: usize = 64;
const MINE_GEOLOGY_SCAN_DEPTH: i32 = 10;

/// Canonical natural rock id (no `minecraft:` prefix) if `id` is one, else `None`.
fn natural_rock_id(id: &BlockID) -> Option<&'static str> {
    let s = id.as_str().trim_start_matches("minecraft:");
    const ROCKS: [&str; 10] = [
        "stone", "deepslate", "granite", "diorite", "andesite", "tuff", "calcite", "basalt", "sandstone", "red_sandstone",
    ];
    ROCKS.iter().copied().find(|&r| r == s)
}

/// Samples the local geology: probes a scatter of cells, scanning down up to
/// `MINE_GEOLOGY_SCAN_DEPTH` for the first natural rock, and returns the most
/// common one plus whether it's deepslate. Defaults to stone if none is found.
///
/// Used for the *area-wide* default (ore deepslate-variant, surface seams). Because
/// vanilla terrain is overwhelmingly stone, this almost always returns stone — which
/// is why per-outcrop detection (`detect_outcrop_rock`) drives the boulder palettes,
/// so granite/andesite/diorite blobs still surface where they're actually exposed.
fn detect_local_rock(ordered: &[Point2D], editor: &Editor) -> (String, bool) {
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    let step = (ordered.len() / MINE_GEOLOGY_SAMPLES).max(1);
    for c in ordered.iter().step_by(step) {
        let Some(ch) = editor.world().get_non_tree_height(*c) else {
            continue;
        };
        let top = ch - 1;
        for dy in 0..MINE_GEOLOGY_SCAN_DEPTH {
            // `try_get_block` (not `get_block`) so scanning below the world floor —
            // common for a mine near bedrock — returns None instead of panicking.
            let Some(block) = editor.try_get_block(Point3D::new(c.x, top - dy, c.y)) else {
                break;
            };
            if let Some(rock) = natural_rock_id(&block.id) {
                *counts.entry(rock).or_insert(0) += 1;
                break;
            }
        }
    }
    match counts.into_iter().max_by_key(|&(_, n)| n) {
        Some((rock, _)) => (rock.to_string(), rock == "deepslate"),
        None => ("stone".to_string(), false),
    }
}

/// Detects the dominant natural rock in the columns a boulder would cover at
/// `center` (a `MINE_BOULDER_MAX_RADIUS` window), scanning each column down to the
/// first natural rock. Returns that rock plus whether it's deepslate; defaults to
/// stone. Sampling locally — rather than picking one rock for the whole mine — means
/// a boulder sitting on an exposed granite/andesite/diorite blob is built from that
/// variant, so small pockets of stone variants surface where they actually occur
/// instead of being drowned out by the area-wide stone majority.
fn detect_outcrop_rock(center: Point2D, editor: &Editor) -> (String, bool) {
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    let r = MINE_BOULDER_MAX_RADIUS;
    for dx in -r..=r {
        for dz in -r..=r {
            let cell = Point2D::new(center.x + dx, center.y + dz);
            if !editor.world().is_in_bounds_2d(cell) {
                continue;
            }
            let Some(ch) = editor.world().get_non_tree_height(cell) else {
                continue;
            };
            let top = ch - 1;
            for dy in 0..MINE_GEOLOGY_SCAN_DEPTH {
                let Some(block) = editor.try_get_block(Point3D::new(cell.x, top - dy, cell.y)) else {
                    break;
                };
                if let Some(rock) = natural_rock_id(&block.id) {
                    *counts.entry(rock).or_insert(0) += 1;
                    break;
                }
            }
        }
    }
    match counts.into_iter().max_by_key(|&(_, n)| n) {
        Some((rock, _)) => (rock.to_string(), rock == "deepslate"),
        None => ("stone".to_string(), false),
    }
}

/// A weighted block mix for an outcrop of the given local rock: the rock itself,
/// a cobbled accent, and a mossy speck for age — all matched to the local rock.
///
/// Only `stone` (cobblestone / mossy_cobblestone) and `deepslate` (cobbled_deepslate)
/// have dedicated cobbled/mossy forms in Minecraft. The other stone variants
/// (granite/diorite/andesite/tuff/calcite/basalt) have none, so they use the rock
/// itself for the accent and speck — keeping, say, a granite boulder granite-toned
/// instead of speckled with grey cobblestone.
fn rock_palette(rock: &str) -> Vec<(Block, f32)> {
    let primary = format!("minecraft:{}", rock);
    let (accent, mossy): (String, String) = match rock {
        "stone" => ("minecraft:cobblestone".into(), "minecraft:mossy_cobblestone".into()),
        "deepslate" => ("minecraft:cobbled_deepslate".into(), "minecraft:cobbled_deepslate".into()),
        _ => (primary.clone(), primary.clone()),
    };
    vec![
        (Block::from_id(primary.as_str().into()), 0.55),
        (Block::from_id(accent.as_str().into()), 0.35),
        (Block::from_id(mossy.as_str().into()), 0.10),
    ]
}

/// The ore block to place, switched to its deepslate variant on deepslate ground
/// (`minecraft:coal_ore` -> `minecraft:deepslate_coal_ore`).
fn ore_for_rock(ore_block: &str, is_deepslate: bool) -> Block {
    let id = if is_deepslate {
        match ore_block.strip_prefix("minecraft:") {
            Some(rest) => format!("minecraft:deepslate_{}", rest),
            None => format!("deepslate_{}", ore_block),
        }
    } else {
        ore_block.to_string()
    };
    Block::from_id(id.as_str().into())
}

/// Raises a small rock outcrop centred on `center`: a tapered mound of `rocks`,
/// seeded with `ore` when `ore_bearing`. Records and claims every column touched.
async fn place_outcrop(
    center: Point2D,
    rocks: &Vec<(Block, f32)>,
    ore: &Block,
    ore_bearing: bool,
    occupied: &mut HashSet<Point2D>,
    structure_id: &crate::generator::nbts::StructureID,
    editor: &mut Editor,
    rng: &mut RNG,
) {
    let radius = rng.rand_i32_range(1, MINE_BOULDER_MAX_RADIUS + 1);
    let r2 = radius * radius;
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            let dist2 = dx * dx + dz * dz;
            if dist2 > r2 {
                continue;
            }
            let cell = Point2D::new(center.x + dx, center.y + dz);
            // An outcrop can spill past its free-cell centre to the map edge;
            // `get_non_tree_height` indexes the heightmap unchecked, so guard it.
            if !editor.world().is_in_bounds_2d(cell) {
                continue;
            }
            let Some(ch) = editor.world().get_non_tree_height(cell) else {
                continue;
            };
            let top = ch - 1;
            // Height tapers from the centre outward, plus a 0–1 block of jitter.
            let falloff = 1.0 - (dist2 as f32 / (r2 as f32 + 1.0)).sqrt();
            let taper = (falloff * MINE_BOULDER_MAX_HEIGHT as f32).round() as i32;
            let h = (taper + rng.rand_i32_range(0, 2)).clamp(1, MINE_BOULDER_MAX_HEIGHT);
            for i in 0..h {
                let block = if ore_bearing && rng.rand_i32_range(0, 100) < MINE_BOULDER_ORE_PERCENT {
                    ore.clone()
                } else {
                    rng.choose_weighted_vec(rocks).clone()
                };
                editor.place_block_forced(&block, Point3D::new(cell.x, top + i, cell.y)).await;
            }
            occupied.insert(cell);
            editor.world_mut().claim(cell, BuildClaim::ProductionArea(structure_id.clone()));
        }
    }
}

/// Mostly leaves the mine's terrain alone, dotting it with rock outcrops, some
/// bearing the mine's ore, plus occasional ore seams poking through the ground.
/// Each outcrop is built from the rock detected directly beneath it (see
/// `detect_outcrop_rock`), so a boulder on an exposed granite/andesite/diorite blob
/// surfaces that variant rather than every outcrop being plain stone. The ore is
/// resolved from the gathered `resource` (`ore_block` in resources.yaml), so one
/// painter serves every mine — an iron mine seeds iron ore, a coal mine coal.
async fn mine_production_painter(
    _params: &serde_yaml::Value,
    free_cells: &HashSet<Point2D>,
    resource: &str,
    worker_budget: usize,
    building_centre: Option<Point2D>,
    structure_id: &crate::generator::nbts::StructureID,
    data: &LoadedData,
    editor: &mut Editor,
    rng: &mut RNG,
) {
    let mut ordered: Vec<Point2D> = free_cells.iter().copied().collect();
    ordered.sort_by_key(|p| (p.x, p.y));

    // Raw ore id (e.g. "minecraft:iron_ore"); the deepslate variant is applied per
    // location from the rock detected there.
    let ore_id: Option<String> = data
        .resource_registry
        .resources()
        .get(resource)
        .and_then(|def| def.ore_block.as_ref())
        .cloned();
    if ore_id.is_none() {
        warn!("mine_production_painter: resource '{}' has no ore_block; placing plain rock", resource);
    }

    // Area-wide rock, used only as the deepslate signal for the surface seams below.
    let (_, area_deepslate) = detect_local_rock(&ordered, editor);

    let mut occupied: HashSet<Point2D> = HashSet::new();
    // Centres of ore-bearing outcrops — the rock faces the miners work.
    let mut ore_outcrops: Vec<Point2D> = Vec::new();

    // 1. Rock outcrops, a fraction of them ore-bearing. Each is built from the rock
    //    detected right under it, so boulders over granite/andesite/diorite blobs
    //    surface those variants instead of every outcrop reading as stone.
    for &c in &ordered {
        if occupied.contains(&c) {
            continue;
        }
        if rng.rand_i32_range(0, 1000) >= MINE_BOULDER_CHANCE_PERMILLE {
            continue;
        }
        let (rock_name, is_deepslate) = detect_outcrop_rock(c, editor);
        let rocks = rock_palette(&rock_name);
        let ore = ore_id
            .as_ref()
            .map(|id| ore_for_rock(id, is_deepslate))
            .unwrap_or_else(|| rocks[0].0.clone());
        let ore_bearing =
            ore_id.is_some() && rng.rand_i32_range(0, 100) < MINE_ORE_BOULDER_PERCENT;
        if ore_bearing {
            ore_outcrops.push(c);
        }
        place_outcrop(c, &rocks, &ore, ore_bearing, &mut occupied, structure_id, editor, rng).await;
    }

    // 2. Ore seams poking through the surface, away from the outcrops.
    if let Some(id) = &ore_id {
        let seam_ore = ore_for_rock(id, area_deepslate);
        for &c in &ordered {
            if occupied.contains(&c) {
                continue;
            }
            if rng.rand_i32_range(0, 1000) >= MINE_TERRAIN_ORE_PERMILLE {
                continue;
            }
            let Some(ch) = editor.world().get_non_tree_height(c) else {
                continue;
            };
            let top = ch - 1;
            editor.place_block_forced(&seam_ore, Point3D::new(c.x, top, c.y)).await;
        }
    }

    // 3. Claim the whole area.
    for &cell in free_cells {
        editor.world_mut().claim(cell, BuildClaim::ProductionArea(structure_id.clone()));
    }

    // A miner posted at an ore-bearing outcrop near the mine. Falls back to any
    // outcrop if none came up ore-bearing, so the crew still has a rock face.
    let mut faces = if ore_outcrops.is_empty() {
        occupied.into_iter().collect::<Vec<_>>()
    } else {
        ore_outcrops
    };
    // Keep the crew close to the mine: restrict to outcrops within MINE_WORKER_RADIUS
    // of the building and take the nearest ones (rather than scattering miners across
    // the whole dig). If none are that close, fall back to the outside-stand spread
    // around the building by recording nothing here.
    if let Some(centre) = building_centre {
        let dist2 = |p: &Point2D| (p.x - centre.x).pow(2) + (p.y - centre.y).pow(2);
        faces.retain(|p| dist2(p) <= MINE_WORKER_RADIUS * MINE_WORKER_RADIUS);
        faces.sort_by_key(dist2);
        faces.truncate(worker_budget);
    } else {
        let chosen: Vec<Point2D> = rng
            .choose_many(&faces, worker_budget.min(faces.len()))
            .into_iter()
            .copied()
            .collect();
        faces = chosen;
    }
    record_feature_anchors(&faces, worker_budget, structure_id, editor);
}

/// Closes diagonal gaps in a fence ring. Minecraft fences only connect along
/// cardinals, so where the perimeter turns at a corner — two fence cells meeting
/// only diagonally — the ring leaks. For each such diagonal pair with no fence
/// cell on either orthogonal in-between cell, this adds one bridging cell,
/// preferring an in-pasture (free) cell so the fence hugs the boundary, returning
/// the full orthogonally-connected fence set.
fn close_diagonal_gaps(
    perimeter: &HashSet<Point2D>,
    free_cells: &HashSet<Point2D>,
) -> HashSet<Point2D> {
    const DIAGONALS: [(i32, i32); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
    let mut fence = perimeter.clone();
    // Snapshot the original cells; we check against the growing `fence` so a
    // bridge added for one diagonal also satisfies its mirror pass.
    let originals: Vec<Point2D> = perimeter.iter().copied().collect();
    for c in originals {
        for (dx, dz) in DIAGONALS {
            let diag = Point2D::new(c.x + dx, c.y + dz);
            if !fence.contains(&diag) {
                continue;
            }
            let c1 = Point2D::new(c.x + dx, c.y);
            let c2 = Point2D::new(c.x, c.y + dz);
            // Already orthogonally connected through an in-between fence cell.
            if fence.contains(&c1) || fence.contains(&c2) {
                continue;
            }
            // Bridge the corner — prefer the in-pasture cell (the convex-corner
            // case); fall back to the other if neither is free.
            let bridge = if free_cells.contains(&c1) { c1 } else { c2 };
            fence.insert(bridge);
        }
    }
    fence
}

/// Cleans up a cell region's outline so painted production areas don't end in
/// ragged, thin strips. A morphological *opening* with a 3x3 (8-connectivity)
/// structuring element: a one-cell erosion followed by a one-cell dilation, both
/// 8-connected and clipped to the original region.
///
/// Erosion keeps only cells whose full 8-neighbourhood is present, so anything
/// narrower than three cells — 1-/2-wide tendrils, frayed single-cell fringes,
/// convex spikes — erodes to nothing. Dilation then regrows the 8-ring around the
/// surviving core, which restores solid blobs *including their corners* (a 3x3
/// square is preserved) without re-extending the thin features, since the core
/// never reaches into them. The result is always a subset of the input, so it
/// never paints onto claimed/water/out-of-parcel cells. A region with no solid
/// ≥3-wide core reduces to empty and returns empty (caller decides the fallback).
fn smooth_region(region: &HashSet<Point2D>) -> HashSet<Point2D> {
    let eroded: HashSet<Point2D> = region
        .iter()
        .copied()
        .filter(|&c| ALL_8.iter().all(|&d| region.contains(&(c + d))))
        .collect();

    let mut opened = eroded.clone();
    for &c in &eroded {
        for d in ALL_8 {
            let n = c + d;
            if region.contains(&n) {
                opened.insert(n);
            }
        }
    }
    opened
}

/// Prunes dangling spurs from a fence ring. A thin protrusion of the pasture
/// turns into a fence line that juts out and dead-ends — visually a fence
/// "randomly connecting" to nothing. Fences only join along cardinals, so a cell
/// that belongs to the enclosing loop has at least two orthogonal fence
/// neighbours; a spur tip has one (and an isolated cell, none). Iteratively
/// removing every cell with fewer than two orthogonal fence neighbours peels each
/// appendage back to the loop it hangs off, leaving only closed rings.
fn prune_fence_spurs(mut fence: HashSet<Point2D>) -> HashSet<Point2D> {
    loop {
        let spurs: Vec<Point2D> = fence
            .iter()
            .filter(|&&c| CARDINALS_2D.iter().filter(|&&d| fence.contains(&(c + d))).count() < 2)
            .copied()
            .collect();
        if spurs.is_empty() {
            break;
        }
        for s in spurs {
            fence.remove(&s);
        }
    }
    fence
}

/// Applies the shared name decoration to a chosen `name`: a decorative prefix
/// ~10% of the time and a suffix ~10% of the time (independent rolls, so ~1% get
/// both), joined with spaces, e.g. "Ol' Bessie", "Daisy the Great". Used for both
/// pasture animals and beehive bees.
fn decorate_name(name: &str, prefixes: &[String], suffixes: &[String], rng: &mut RNG) -> String {
    let mut out = name.to_string();
    if !prefixes.is_empty() && rng.rand_i32_range(0, 100) < 10 {
        out = format!("{} {}", rng.choose(prefixes), out);
    }
    if !suffixes.is_empty() && rng.rand_i32_range(0, 100) < 10 {
        out = format!("{} {}", out, rng.choose(suffixes));
    }
    out
}

/// Builds the `CustomName` SNBT for a visible nametag, as a plain text component.
///
/// On 1.21.5+ the `CustomName` value is parsed directly as an SNBT text
/// component, so the old `'{"text":"..."}'` JSON-string form is rendered
/// *literally* (braces and all). A bare double-quoted string is a valid text
/// component (literal text), so we emit `CustomName:"Name"` — escaping only the
/// backslashes and double quotes that would close the string. Apostrophes (e.g.
/// "Ol'") are fine unescaped inside double quotes.
fn custom_name_snbt(name: &str) -> String {
    let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
    format!("CustomName:\"{}\",CustomNameVisible:1b", escaped)
}

/// Builds the `CustomName` NBT for a spawned animal (visible nametag), or `None`
/// when no names are loaded. Tweak the NBT string here if a Minecraft version
/// changes the entity name format.
fn animal_name_nbt(
    names: &[String],
    prefixes: &[String],
    suffixes: &[String],
    rng: &mut RNG,
) -> Option<String> {
    if names.is_empty() {
        return None;
    }
    let base = rng.choose(names).clone();
    let name = decorate_name(&base, prefixes, suffixes, rng);
    Some(format!("{{{}}}", custom_name_snbt(&name)))
}

/// Enclosed grazing pasture: fences the perimeter of the free cells (with a few
/// gates), paints the border ring, and spawns a small herd of named animals
/// inside. Shared by `sheep_pasture` and `cattle_ranch` via the `animal` param.
///
/// Params:
/// - `animal` (string, required) — entity id, e.g. `minecraft:sheep`.
/// - `min_count` / `max_count` (u32, default 10/20) — herd size range (inclusive).
/// - `border_palette` (string, default `rural_road`) — border ring palette.
/// - `palette` (string, default `oak`) — building palette the fence wood follows.
/// - `flatten_strength` (f32, default 0) — optional feathered terrain smoothing.
async fn pasture_production_painter(
    params: &serde_yaml::Value,
    free_cells: &HashSet<Point2D>,
    border_cells: &HashSet<Point2D>,
    worker_budget: usize,
    structure_id: &crate::generator::nbts::StructureID,
    data: &LoadedData,
    editor: &mut Editor,
    rng: &mut RNG,
) {
    #[derive(Deserialize)]
    struct Params {
        animal: String,
        #[serde(default = "default_min_count")]
        min_count: u32,
        #[serde(default = "default_max_count")]
        max_count: u32,
        #[serde(default = "default_border_palette")]
        border_palette: String,
        #[serde(default = "default_fence_palette")]
        palette: String,
        #[serde(default)]
        flatten_strength: f32,
    }
    fn default_min_count() -> u32 { 10 }
    fn default_max_count() -> u32 { 20 }
    fn default_border_palette() -> String { "rural_road".to_string() }
    fn default_fence_palette() -> String { "oak".to_string() }

    let p: Params = match parse_params(params) {
        Ok(p) => p,
        Err(e) => {
            warn!("pasture_production_painter: invalid params: {}", e);
            return;
        }
    };

    // Smooth (optional) and lay the border ring, mirroring the palette painter.
    feather_smooth(free_cells, border_cells, p.flatten_strength, editor).await;
    paint_border(border_cells, Some(p.border_palette.as_str()), data, editor, rng).await;

    // Perimeter = free cells with at least one cardinal neighbour outside the
    // pasture. These form the fence ring; gates are chosen only from these (not
    // from the diagonal bridge cells added below).
    let perimeter_set: HashSet<Point2D> = free_cells
        .iter()
        .filter(|&&c| CARDINALS_2D.iter().any(|&d| !free_cells.contains(&(c + d))))
        .copied()
        .collect();

    // Fences only join along cardinals, so close any diagonal gaps where the ring
    // turns at a corner (otherwise the enclosure leaks).
    let fence_cells = close_diagonal_gaps(&perimeter_set, free_cells);
    // Drop dead-end spurs left by thin pasture protrusions so the fence reads as a
    // clean enclosure instead of stray lines jutting out.
    let fence_cells = prune_fence_spurs(fence_cells);

    // A few gates spaced around the ring (~1 per 15 perimeter cells, at least 2).
    // Only real perimeter cells that survived pruning are gate-eligible (never the
    // diagonal bridge cells, and never a pruned spur).
    let perimeter: Vec<Point2D> = perimeter_set
        .iter()
        .copied()
        .filter(|c| fence_cells.contains(c))
        .collect();
    const GATE_SPACING: usize = 15;
    let gate_count = (perimeter.len() / GATE_SPACING).max(2).min(perimeter.len());
    let gate_cells: HashSet<Point2D> =
        rng.choose_many(&perimeter, gate_count).into_iter().copied().collect();

    let (fence_block, gate_id) = resolve_fence_blocks(&p.palette, data, rng);

    for &cell in &fence_cells {
        let Some(y) = editor.world().get_non_tree_height(cell) else {
            continue;
        };
        let pos = Point3D::new(cell.x, y, cell.y);
        if gate_cells.contains(&cell) {
            // Face the gate toward an outward (non-pasture) neighbour.
            let state = CARDINALS_2D
                .iter()
                .find(|&&d| !free_cells.contains(&(cell + d)))
                .and_then(cardinal_to_str)
                .map(|f| HashMap::from([("facing".to_string(), f)]));
            editor.place_block(&Block::new(gate_id.clone(), state, None), pos).await;
        } else {
            editor.place_block(&fence_block, pos).await;
        }
        editor.world_mut().claim(cell, BuildClaim::ProductionArea(structure_id.clone()));
    }

    // Spawn the herd on interior cells (not on the fence ring or its bridges).
    let interior: Vec<Point2D> = free_cells
        .iter()
        .filter(|c| !fence_cells.contains(c))
        .copied()
        .collect();

    let mut herd_cells: Vec<Point2D> = Vec::new();
    if !interior.is_empty() {
        let lo = p.min_count.min(p.max_count) as i32;
        let hi = p.min_count.max(p.max_count) as i32;
        // rand_i32_range is exclusive of the upper bound, so +1 makes it inclusive
        // (and guarantees a non-zero range even when min == max).
        let count = (rng.rand_i32_range(lo, hi + 1) as usize).min(interior.len());

        let reg = &data.resource_registry;
        let spots = rng.choose_many(&interior, count);
        let mut entities: Vec<(Point3D, String, Option<String>)> = Vec::with_capacity(spots.len());
        for &spot in spots {
            let Some(y) = editor.world().get_non_tree_height(spot) else {
                continue;
            };
            let pos = Point3D::new(spot.x, y, spot.y);
            let nbt = animal_name_nbt(&reg.animal_names, &reg.animal_name_prefixes, &reg.animal_name_suffixes, rng);
            entities.push((pos, p.animal.clone(), nbt));
            herd_cells.push(spot);
        }
        editor.spawn_entities(&entities).await;
    }

    // Claim every free cell for this production area.
    for &cell in free_cells {
        editor.world_mut().claim(cell, BuildClaim::ProductionArea(structure_id.clone()));
    }

    // A herder posted among the flock — beside an animal, out in the pasture (a
    // spread sample). On the ranch this is a no-op, since its indoor anchors
    // already fill the crew; the shepherd's hut has no interior anchors, so its
    // crew lands here.
    let chosen: Vec<Point2D> = rng
        .choose_many(&herd_cells, worker_budget.min(herd_cells.len()))
        .into_iter()
        .copied()
        .collect();
    record_feature_anchors(&chosen, worker_budget, structure_id, editor);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::World;
    use crate::generator::nbts::{StructureID, StructureType};
    use crate::geometry::Rect3D;

    fn set(cells: &[(i32, i32)]) -> HashSet<Point2D> {
        cells.iter().map(|&(x, z)| Point2D::new(x, z)).collect()
    }

    /// A flat synthetic world (no server) so the dynamic-anchor helpers can be
    /// exercised offline: open ground everywhere at `ground_y`.
    fn flat_editor(ground_y: i32) -> Editor {
        let build_area = Rect3D {
            origin: Point3D::new(0, 0, 0),
            size: Point3D::new(32, 256, 32),
        };
        World::synthetic(build_area, ground_y).get_offline_editor()
    }

    #[test]
    fn feature_anchor_stands_beside_and_faces_feature() {
        let ground_y = 64;
        let mut editor = flat_editor(ground_y);
        let id = StructureID { id: 7, structure_type: StructureType("woodcutter_hut".into()) };

        // One feature (a stump) at (10,10). Budget 1 → one anchor beside it, facing it.
        record_feature_anchors(&[Point2D::new(10, 10)], 1, &id, &mut editor);

        let posts = editor.world().structure_anchors.get(&7).expect("anchor recorded");
        assert_eq!(posts.len(), 1);
        let (feet, _yaw) = posts[0];
        // Feet are on the ground, in a cell cardinally adjacent to the feature.
        assert_eq!(feet.y, ground_y);
        let manhattan = (feet.x - 10).abs() + (feet.z - 10).abs();
        assert_eq!(manhattan, 1, "worker should stand one cell from the stump");
    }

    #[test]
    fn feature_anchors_respect_budget() {
        let mut editor = flat_editor(64);
        let id = StructureID { id: 1, structure_type: StructureType("iron_mine".into()) };
        let feats: Vec<Point2D> = (0..10).map(|i| Point2D::new(i * 2, 5)).collect();

        // Budget 2 caps the recorded posts at 2 even with ten features.
        record_feature_anchors(&feats, 2, &id, &mut editor);
        assert_eq!(editor.world().structure_anchors.get(&1).unwrap().len(), 2);

        // A second call appends but never exceeds the budget total.
        record_feature_anchors(&feats, 2, &id, &mut editor);
        assert_eq!(editor.world().structure_anchors.get(&1).unwrap().len(), 2);
    }

    #[test]
    fn bridges_convex_corner_with_interior_cell() {
        // Two perimeter cells meeting only diagonally; the cell that joins them
        // orthogonally is in the pasture, the other is outside.
        let perimeter = set(&[(1, 0), (2, 1)]);
        let free = set(&[(1, 0), (2, 1), (1, 1)]); // (2,0) is outside
        let fence = close_diagonal_gaps(&perimeter, &free);
        assert!(fence.contains(&Point2D::new(1, 1)), "diagonal gap should be bridged via the free cell");
    }

    #[test]
    fn no_bridge_when_already_orthogonally_connected() {
        // (1,0) already sits between the diagonal pair, so nothing is added.
        let perimeter = set(&[(0, 0), (1, 0), (1, 1)]);
        let free = set(&[(0, 0), (1, 0), (1, 1)]);
        let fence = close_diagonal_gaps(&perimeter, &free);
        assert_eq!(fence.len(), perimeter.len(), "no bridge needed for a connected ring");
    }

    #[test]
    fn straight_edge_is_left_untouched() {
        let perimeter = set(&[(0, 0), (0, 1), (0, 2)]);
        let free = perimeter.clone();
        let fence = close_diagonal_gaps(&perimeter, &free);
        assert_eq!(fence, perimeter, "a straight run has no diagonal gaps");
    }

    #[test]
    fn prune_keeps_closed_loop() {
        // A 3x3 ring (8 cells, hollow centre): every cell has two orthogonal
        // neighbours, so nothing is pruned.
        let ring = set(&[
            (0, 0), (1, 0), (2, 0),
            (0, 1),         (2, 1),
            (0, 2), (1, 2), (2, 2),
        ]);
        assert_eq!(prune_fence_spurs(ring.clone()), ring, "a closed loop must survive intact");
    }

    #[test]
    fn prune_removes_spur_off_a_loop() {
        // Same ring plus a two-cell spur hanging off (3,1)-(4,1).
        let mut withspur = set(&[
            (0, 0), (1, 0), (2, 0),
            (0, 1),         (2, 1),
            (0, 2), (1, 2), (2, 2),
        ]);
        withspur.insert(Point2D::new(3, 1));
        withspur.insert(Point2D::new(4, 1));
        let loop_only = set(&[
            (0, 0), (1, 0), (2, 0),
            (0, 1),         (2, 1),
            (0, 2), (1, 2), (2, 2),
        ]);
        assert_eq!(prune_fence_spurs(withspur), loop_only, "the dangling spur must be pruned back to the loop");
    }

    #[test]
    fn prune_clears_an_open_line() {
        // A bare line (no cycle) is all dead-ends and prunes away entirely.
        let line = set(&[(0, 0), (1, 0), (2, 0), (3, 0)]);
        assert!(prune_fence_spurs(line).is_empty(), "an open line with no loop should fully prune");
    }

    /// A solid 3x3 blob keeps a 1-wide tendril hanging off it; smoothing removes
    /// the whole tendril (opening shortens it, the prune peels the stub) while the
    /// blob survives intact.
    #[test]
    fn smooth_drops_thin_tendril_keeps_blob() {
        let mut region: HashSet<Point2D> = HashSet::new();
        for x in 0..3 {
            for z in 0..3 {
                region.insert(Point2D::new(x, z));
            }
        }
        // 1-wide tendril extending right from the blob's middle row.
        region.insert(Point2D::new(3, 1));
        region.insert(Point2D::new(4, 1));

        let smoothed = smooth_region(&region);
        assert!(!smoothed.contains(&Point2D::new(4, 1)), "tendril tip must be removed");
        assert!(!smoothed.contains(&Point2D::new(3, 1)), "tendril stub must be removed");
        // The solid 3x3 blob is preserved.
        for x in 0..3 {
            for z in 0..3 {
                assert!(smoothed.contains(&Point2D::new(x, z)), "blob cell ({x},{z}) must survive");
            }
        }
    }

    #[test]
    fn smooth_erases_a_one_wide_region() {
        // Nothing is ≥3 wide, so the whole strip erodes/prunes away (caller falls back).
        let strip = set(&[(0, 0), (1, 0), (2, 0), (3, 0)]);
        assert!(smooth_region(&strip).is_empty(), "a 1-wide region has no solid core to keep");
    }
}
