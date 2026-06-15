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
    geometry::{cardinal_to_str, Point2D, Point3D, CARDINALS_2D},
    minecraft::{Block, BlockForm, BlockID},
    noise::RNG,
};

use super::production_painter::{parse_params, ProductionPainter};

/// Width (Chebyshev cells) of the buffer around a production district's edge.
/// Excluded from the field interior; used as the border strip and feather band.
const EDGE_BUFFER: i32 = 3;

/// How far (cells) production-area smoothing reaches into neighbouring land, to
/// feather the field's terrain into its surroundings rather than ending in a step.
const NEIGHBOUR_REACH: i32 = 2;

/// Paints a production area across all unclaimed cells of `district` after
/// a gathering building has been placed there. The area is claimed with
/// `BuildClaim::ProductionArea` tied to the most-recently-placed structure on the world.
pub async fn paint_production_area(
    district: &District,
    painter_name: &str,
    resource: &str,
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

    // Build a set of cells within EDGE_BUFFER blocks (Chebyshev) of any edge cell.
    let edge_buffer: HashSet<Point2D> = district.data.edges.iter()
        .flat_map(|p| {
            let p2 = p.drop_y();
            (-EDGE_BUFFER..=EDGE_BUFFER).flat_map(move |dx| {
                (-EDGE_BUFFER..=EDGE_BUFFER).map(move |dz| Point2D::new(p2.x + dx, p2.y + dz))
            })
        })
        .collect();

    // Free cells: parcel interior excluding edge buffer, not yet claimed, not water.
    let free_cells: HashSet<Point2D> = district.data.points_2d.iter()
        .filter(|&&p| !edge_buffer.contains(&p))
        .filter(|&&p| !editor.world().is_claimed(p))
        .filter(|&&p| !editor.world().is_water(p))
        .copied()
        .collect();

    if free_cells.is_empty() {
        return;
    }

    // Border cells: parcel interior points that fall within the edge buffer, not
    // yet claimed, not water. Painted with the border palette (e.g. rural_road) by
    // both the palette and function painters.
    let border_cells: HashSet<Point2D> = district.data.points_2d.iter()
        .filter(|&&p| edge_buffer.contains(&p))
        .filter(|&&p| !editor.world().is_claimed(p))
        .filter(|&&p| !editor.world().is_water(p))
        .copied()
        .collect();

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
                    logging_production_painter(&params, &free_cells, &structure_id, editor, rng).await;
                }
                "pasture_production_painter" => {
                    pasture_production_painter(&params, &free_cells, &border_cells, &structure_id, data, editor, rng).await;
                }
                "sugarcane_production_painter" => {
                    sugarcane_production_painter(&params, &free_cells, &border_cells, &structure_id, data, editor, rng).await;
                }
                "bee_area_production_painter" => {
                    bee_area_production_painter(&params, &free_cells, &structure_id, data, editor, rng).await;
                }
                "mine_production_painter" => {
                    mine_production_painter(&params, &free_cells, resource, &structure_id, data, editor, rng).await;
                }
                other => {
                    warn!("paint_production_area: unknown painter function '{}'", other);
                }
            }
        }
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
        let stump_y = editor.world().get_non_tree_height(tree.trunk);
        let stump_pos = tree.trunk.add_y(stump_y);
        stumps.push((stump_pos, editor.get_block(stump_pos)));
        to_log.extend(tree.cells.iter().copied());
    }

    log_trees(editor, to_log).await;

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
        let y = editor.world().get_non_tree_height(c) - 1;
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
            let wy = editor.world().get_non_tree_height(w) - 1;
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
                water_set.contains(&n) && editor.world().get_non_tree_height(n) - 1 == wy
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
        let wy = editor.world().get_non_tree_height(w) - 1;
        editor.place_block_forced(&water_block, Point3D::new(w.x, wy, w.y)).await;
    }
    editor.flush_buffer().await;

    // 4. Cane columns on non-water cells that have a same-level water neighbour,
    //    at varied heights (mostly 2–3 tall) with a random age on the top block.
    let height_weights: HashMap<usize, f32> =
        HashMap::from([(1usize, 0.15f32), (2, 0.45), (3, 0.40)]);
    let min_h = p.min_height.max(1) as usize;
    let max_h = p.max_height.max(min_h as u32) as usize;

    for &c in &ordered {
        if water_set.contains(&c) {
            continue;
        }
        let cy = editor.world().get_non_tree_height(c); // base air cell of the column
        let support_y = cy - 1;
        let has_water = CARDINALS_2D.iter().any(|&d| {
            let n = c + d;
            water_set.contains(&n) && editor.world().get_non_tree_height(n) - 1 == support_y
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
    }

    // 5. Claim every free cell for this production area.
    for &cell in free_cells {
        editor.world_mut().claim(cell, BuildClaim::ProductionArea(structure_id.clone()));
    }
}

/// Builds beehive block-entity NBT filled with three bees, each given a random
/// (visible) funny name from `bee_names` — decorated with the same ~10% prefix /
/// ~10% suffix system as pasture animals (e.g. "Sir Buzz", "Beeyonce the Great").
/// Bees emerge to buzz the canopy (low occupation/in-hive ticks). Tweak here if a
/// Minecraft version changes the beehive `Bees` / bee-name format.
fn beehive_nbt(bee_names: &[String], prefixes: &[String], suffixes: &[String], rng: &mut RNG) -> String {
    const BEE_COUNT: usize = 3;
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
            format!("{{EntityData:{},MinOccupationTicks:0,TicksInHive:0}}", entity)
        })
        .collect();

    format!("{{id:\"minecraft:beehive\",Bees:[{}]}}", bees.join(","))
}

/// Finds a nest site on `trunk`'s log column: a cell cardinally adjacent to a log
/// (1 block away), itself air or leaves (so we don't carve the stem), with a leaf
/// directly above (sheltered beneath the canopy). Searches from the top log down
/// so hives sit up in the canopy. Returns `(position, facing)` where `facing`
/// points away from the trunk. `None` if the tree has no such nook.
fn find_hive_spot(trunk: Point2D, editor: &Editor) -> Option<(Point3D, Point2D)> {
    let base_y = editor.world().get_non_tree_height(trunk);

    // Walk up the trunk's logs to find the top of the stem.
    let mut top_y = base_y;
    let mut y = base_y;
    while editor.get_block(trunk.add_y(y)).id.is_log() {
        top_y = y;
        y += 1;
    }

    for ly in (base_y..=top_y).rev() {
        for d in CARDINALS_2D {
            let pos = Point3D::new(trunk.x + d.x, ly, trunk.y + d.y);
            let here = editor.get_block(pos).id;
            if !(here.is_air() || here.is_leaves()) {
                continue;
            }
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
    Block::new("minecraft:beehive".into(), Some(state), Some(beehive_nbt(bee_names, prefixes, suffixes, rng)))
}

/// Hangs a populated beehive in the canopy of a percentage of the area's trees —
/// beneath leaves, one block from a log. Uses the trunk-anchored tree recognition
/// (`group_trees`) so each tree is considered once.
///
/// Params: `percent` (f32, 0.0–1.0, default 0.3) — fraction of trees to nest.
async fn bee_area_production_painter(
    params: &serde_yaml::Value,
    free_cells: &HashSet<Point2D>,
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

    for tree in &selected {
        if let Some((pos, facing)) = find_hive_spot(tree.trunk, editor) {
            let hive = make_beehive(facing, bee_names, prefixes, suffixes, rng);
            // Forced so it can take a leaf cell as well as an air pocket.
            editor.place_block_forced(&hive, pos).await;
        }
    }

    // Claim all free cells for this production area.
    for &cell in free_cells {
        editor.world_mut().claim(cell, BuildClaim::ProductionArea(structure_id.clone()));
    }
}

// --- Mine painter tunables (edit freely to change the look) ---
/// Per-cell chance, in parts-per-1000, of seeding a rock outcrop.
const MINE_BOULDER_CHANCE_PERMILLE: i32 = 30;
/// Per-cell chance, in parts-per-1000, of an ore block poking through the surface.
const MINE_TERRAIN_ORE_PERMILLE: i32 = 20;
/// Percent of outcrops that carry ore.
const MINE_ORE_BOULDER_PERCENT: i32 = 40;
/// Within an ore-bearing outcrop, percent of blocks that are ore (vs rock).
const MINE_BOULDER_ORE_PERCENT: i32 = 30;
/// Outcrop horizontal radius and vertical height, in blocks.
const MINE_BOULDER_MAX_RADIUS: i32 = 2;
const MINE_BOULDER_MAX_HEIGHT: i32 = 3;
/// Local-rock sampling: how many cells to probe, and how far down each (blocks).
const MINE_GEOLOGY_SAMPLES: usize = 64;
const MINE_GEOLOGY_SCAN_DEPTH: i32 = 10;

/// Canonical natural rock id (no `minecraft:` prefix) if `id` is one, else `None`.
fn natural_rock_id(id: &BlockID) -> Option<&'static str> {
    let s = id.as_str().trim_start_matches("minecraft:");
    const ROCKS: [&str; 8] = [
        "stone", "deepslate", "granite", "diorite", "andesite", "tuff", "calcite", "basalt",
    ];
    ROCKS.iter().copied().find(|&r| r == s)
}

/// Samples the local geology: probes a scatter of cells, scanning down up to
/// `MINE_GEOLOGY_SCAN_DEPTH` for the first natural rock, and returns the most
/// common one plus whether it's deepslate. Defaults to stone if none is found.
fn detect_local_rock(ordered: &[Point2D], editor: &Editor) -> (String, bool) {
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    let step = (ordered.len() / MINE_GEOLOGY_SAMPLES).max(1);
    for c in ordered.iter().step_by(step) {
        let top = editor.world().get_non_tree_height(*c) - 1;
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

/// A weighted block mix for an outcrop of the given local rock: the rock itself,
/// a cobbled accent, and a mossy speck for age. Deepslate uses cobbled deepslate.
fn rock_palette(rock: &str) -> Vec<(Block, f32)> {
    let primary = format!("minecraft:{}", rock);
    let (accent, mossy) = if rock == "deepslate" {
        ("minecraft:cobbled_deepslate", "minecraft:cobbled_deepslate")
    } else {
        ("minecraft:cobblestone", "minecraft:mossy_cobblestone")
    };
    vec![
        (Block::from_id(primary.as_str().into()), 0.55),
        (Block::from_id(accent.into()), 0.35),
        (Block::from_id(mossy.into()), 0.10),
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
            let top = editor.world().get_non_tree_height(cell) - 1;
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

/// Mostly leaves the mine's terrain alone, dotting it with rock outcrops built
/// from the local stone (cobble/stone by default), some bearing the mine's ore,
/// plus occasional ore seams poking through the ground. The ore is resolved from
/// the gathered `resource` (`ore_block` in resources.yaml), so one painter serves
/// every mine — an iron mine seeds iron ore, a coal mine coal.
async fn mine_production_painter(
    _params: &serde_yaml::Value,
    free_cells: &HashSet<Point2D>,
    resource: &str,
    structure_id: &crate::generator::nbts::StructureID,
    data: &LoadedData,
    editor: &mut Editor,
    rng: &mut RNG,
) {
    let mut ordered: Vec<Point2D> = free_cells.iter().copied().collect();
    ordered.sort_by_key(|p| (p.x, p.y));

    let (rock_name, is_deepslate) = detect_local_rock(&ordered, editor);
    let rocks = rock_palette(&rock_name);

    let ore_block: Option<Block> = data
        .resource_registry
        .resources()
        .get(resource)
        .and_then(|def| def.ore_block.as_ref())
        .map(|id| ore_for_rock(id, is_deepslate));
    if ore_block.is_none() {
        warn!("mine_production_painter: resource '{}' has no ore_block; placing plain rock", resource);
    }

    let mut occupied: HashSet<Point2D> = HashSet::new();

    // 1. Rock outcrops, a fraction of them ore-bearing.
    for &c in &ordered {
        if occupied.contains(&c) {
            continue;
        }
        if rng.rand_i32_range(0, 1000) >= MINE_BOULDER_CHANCE_PERMILLE {
            continue;
        }
        let ore_bearing =
            ore_block.is_some() && rng.rand_i32_range(0, 100) < MINE_ORE_BOULDER_PERCENT;
        let ore = ore_block.clone().unwrap_or_else(|| rocks[0].0.clone());
        place_outcrop(c, &rocks, &ore, ore_bearing, &mut occupied, structure_id, editor, rng).await;
    }

    // 2. Ore seams poking through the surface, away from the outcrops.
    if let Some(ore) = &ore_block {
        for &c in &ordered {
            if occupied.contains(&c) {
                continue;
            }
            if rng.rand_i32_range(0, 1000) >= MINE_TERRAIN_ORE_PERMILLE {
                continue;
            }
            let top = editor.world().get_non_tree_height(c) - 1;
            editor.place_block_forced(ore, Point3D::new(c.x, top, c.y)).await;
        }
    }

    // 3. Claim the whole area.
    for &cell in free_cells {
        editor.world_mut().claim(cell, BuildClaim::ProductionArea(structure_id.clone()));
    }
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

/// Wraps a name in the single-quoted SNBT `CustomName` text component, escaping
/// the apostrophes in prefixes like "Ol'" / backslashes that would close it.
fn custom_name_snbt(name: &str) -> String {
    let escaped = name.replace('\\', "\\\\").replace('\'', "\\'");
    format!("CustomName:'{{\"text\":\"{}\"}}',CustomNameVisible:1b", escaped)
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

    // A few gates spaced around the ring (~1 per 15 perimeter cells, at least 2).
    let perimeter: Vec<Point2D> = perimeter_set.iter().copied().collect();
    const GATE_SPACING: usize = 15;
    let gate_count = (perimeter.len() / GATE_SPACING).max(2).min(perimeter.len());
    let gate_cells: HashSet<Point2D> =
        rng.choose_many(&perimeter, gate_count).into_iter().copied().collect();

    let (fence_block, gate_id) = resolve_fence_blocks(&p.palette, data, rng);

    for &cell in &fence_cells {
        let y = editor.world().get_non_tree_height(cell);
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
            let y = editor.world().get_non_tree_height(spot);
            let pos = Point3D::new(spot.x, y, spot.y);
            let nbt = animal_name_nbt(&reg.animal_names, &reg.animal_name_prefixes, &reg.animal_name_suffixes, rng);
            entities.push((pos, p.animal.clone(), nbt));
        }
        editor.spawn_entities(&entities).await;
    }

    // Claim every free cell for this production area.
    for &cell in free_cells {
        editor.world_mut().claim(cell, BuildClaim::ProductionArea(structure_id.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(cells: &[(i32, i32)]) -> HashSet<Point2D> {
        cells.iter().map(|&(x, z)| Point2D::new(x, z)).collect()
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
}
