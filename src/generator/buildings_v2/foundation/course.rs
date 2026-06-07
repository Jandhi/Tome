//! Foundation course: analyze the terrain under a footprint, choose a base Y,
//! cut/fill columns to it, clear snow, and lay the foundation stone course.

use std::collections::{HashMap, HashSet};

use crate::editor::{Editor, World};
use crate::generator::buildings_v2::footprint::Footprint;
use crate::generator::buildings_v2::pipeline::BuildCtx;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Point2D, Point3D};
use crate::minecraft::{Block, BlockForm, BlockID};
use crate::noise::RNG;

/// Full foundation pipeline: analyze terrain, fill/cut, place foundation course,
/// update heightmap. Returns `base_y` so downstream modules know where the building starts.
///
/// `base_y_override` forces the floor level (e.g. to match an adjacent road) instead
/// of deriving it from the terrain percentile; cut/fill then reconciles the ground to it.
pub async fn place_foundation(
    ctx: &mut BuildCtx<'_>,
    footprint: &Footprint,
    base_y_override: Option<i32>,
) -> i32 {
    let profile = analyze_terrain(footprint, ctx.editor.world(), base_y_override);

    let columns = classify_columns(&profile);
    execute_columns(ctx.editor, &profile, &columns, ctx.data, ctx.palette, ctx.rng).await;
    clear_snow(ctx.editor, footprint, profile.base_y).await;

    place_foundation_course(ctx.editor, footprint, &profile, ctx.data, ctx.palette, ctx.rng).await;

    let height_points: HashSet<Point3D> = footprint
        .filled_points()
        .iter()
        .map(|&p| Point3D::new(p.x, profile.base_y, p.y))
        .collect();
    ctx.editor.world_mut().set_heights(&height_points);

    super::terraform::blend_terrain(ctx, footprint, profile.base_y).await;

    profile.base_y
}

/// Result of analyzing the terrain under a footprint.
pub struct TerrainProfile {
    /// Height at each footprint point (Point2D -> y).
    pub heights: HashMap<Point2D, i32>,
    pub min_height: i32,
    pub max_height: i32,
    /// The chosen Y level for the building floor.
    pub base_y: i32,
}

/// Analyzes terrain under the footprint and chooses a base Y level.
///
/// Footprint points use the same local coordinate system as the World heightmaps.
/// `base_y_override`, when set, is used verbatim as the floor level (the cut/fill
/// pass then reconciles the columns to it) instead of the terrain percentile.
pub fn analyze_terrain(footprint: &Footprint, world: &World, base_y_override: Option<i32>) -> TerrainProfile {
    let points = footprint.filled_points();

    let heights: HashMap<Point2D, i32> = points
        .iter()
        .map(|&p| {
            let h = world.get_ocean_floor_height_at(p);
            (p, h)
        })
        .collect();

    let min_height = *heights.values().min().expect("Footprint has no points");
    let max_height = *heights.values().max().unwrap();
    let slope = max_height - min_height;

    let base_y = base_y_override.unwrap_or_else(|| choose_base_y(&heights, slope));

    TerrainProfile {
        heights,
        min_height,
        max_height,
        base_y,
    }
}

/// Describes what to do at each column under the footprint.
enum ColumnAction {
    /// Terrain is above base_y. Cut down to base_y.
    Cut { terrain_y: i32 },
    /// Terrain is below base_y. Fill with blocks up to base_y - 1.
    Fill { terrain_y: i32 },
}

fn classify_columns(profile: &TerrainProfile) -> HashMap<Point2D, ColumnAction> {
    profile
        .heights
        .iter()
        .filter_map(|(&point, &terrain_y)| {
            let diff = profile.base_y - terrain_y;
            match diff {
                // terrain is above base_y — cut
                ..=-1 => Some((point, ColumnAction::Cut { terrain_y })),
                // terrain is at base_y — nothing to do
                0 => None,
                // terrain is below base_y — fill
                _ => Some((point, ColumnAction::Fill { terrain_y })),
            }
        })
        .collect()
}

/// Returns true if `id` is a vanilla stone-family block (stone, granite,
/// andesite, diorite, deepslate, tuff). Variants like cobblestone, polished,
/// and sandstone are excluded — only the natural-mountain stones qualify so
/// foundations on stone terrain blend visually with the rock they sit on.
fn is_vanilla_stone(id: &str) -> bool {
    let bare = id.strip_prefix("minecraft:").unwrap_or(id);
    matches!(bare, "stone" | "granite" | "andesite" | "diorite" | "deepslate" | "tuff")
}

/// Executes fill and cut operations for all columns.
///
/// - **Cut:** places air from `base_y` to `terrain_y`, copies the surface block to `base_y - 1`.
/// - **Fill:** fills solid from `terrain_y` to `base_y - 1`. Uses the native
///   terrain block if it's vanilla stone, otherwise palette PrimaryStone.
///
async fn execute_columns(
    editor: &Editor,
    profile: &TerrainProfile,
    columns: &HashMap<Point2D, ColumnAction>,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
) {
    let mut placer_rng = rng.derive();
    let mut stone_placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng),
        palette
            .get_material(MaterialRole::PrimaryStone)
            .expect("Primary stone material not found")
            .clone(),
    );

    let air = Block::new(BlockID::default(), None, None);

    for (&point, action) in columns {
        match *action {
            ColumnAction::Cut { terrain_y } => {
                // Copy the surface block down to base_y - 1
                let surface = editor
                    .world()
                    .get_block(point.add_y(terrain_y - 1))
                    .unwrap_or_else(|| Block::new("dirt".into(), None, None));
                editor
                    .place_block(&surface, point.add_y(profile.base_y - 1))
                    .await;

                // Clear everything from base_y up to terrain_y
                for y in profile.base_y..=terrain_y {
                    editor.place_block_forced(&air, point.add_y(y)).await;
                }
            }
            ColumnAction::Fill { terrain_y } => {
                let native = native_stone_at(editor, point, terrain_y);
                for y in terrain_y..profile.base_y {
                    if let Some(ref block) = native {
                        editor.place_block_forced(block, point.add_y(y)).await;
                    } else {
                        stone_placer
                            .place_block(editor, point.add_y(y), BlockForm::Block, None, None)
                            .await;
                    }
                }
            }
        }
    }
}

/// If the surface block at `(point, terrain_y - 1)` is vanilla stone, return
/// it. Otherwise None — callers should fall back to palette stone.
fn native_stone_at(editor: &Editor, point: Point2D, terrain_y: i32) -> Option<Block> {
    let surface = editor.world().get_block(point.add_y(terrain_y - 1))?;
    if is_vanilla_stone(surface.id.as_str()) {
        Some(surface)
    } else {
        None
    }
}

/// Clear snow layers above the footprint. The cut pass uses ocean-floor
/// heights which exclude snow, so snow sitting on top of terrain inside the
/// footprint survives. This pass removes it before walls/floors go in.
async fn clear_snow(editor: &Editor, footprint: &Footprint, base_y: i32) {
    let air = Block::new(BlockID::default(), None, None);
    for point in footprint.filled_points() {
        // Check a few blocks above base_y for snow layers.
        for y in base_y..(base_y + 4) {
            let pos = point.add_y(y);
            if let Some(block) = editor.try_get_block(pos) {
                if block.id.as_str().contains("snow") {
                    editor.place_block_forced(&air, pos).await;
                }
            }
        }
    }
}

/// Places a full stone layer at `base_y - 1` under the entire footprint. Per
/// column, if the original terrain surface is vanilla stone, the foundation
/// uses that same block so the building blends with the rock beneath.
async fn place_foundation_course(
    editor: &Editor,
    footprint: &Footprint,
    profile: &TerrainProfile,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
) {
    let mut placer_rng = rng.derive();
    let mut stone_placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng),
        palette
            .get_material(MaterialRole::PrimaryStone)
            .expect("Primary stone material not found")
            .clone(),
    );

    let base_y = profile.base_y;
    for point in footprint.filled_points() {
        let terrain_y = profile.heights.get(&point).copied().unwrap_or(base_y);
        let native = native_stone_at(editor, point, terrain_y);
        let pos = point.add_y(base_y - 1);
        if let Some(ref block) = native {
            editor.place_block_forced(block, pos).await;
        } else {
            stone_placer
                .place_block_forced(editor, pos, BlockForm::Block, None, None)
                .await;
        }
    }
}

fn choose_base_y(heights: &HashMap<Point2D, i32>, slope: i32) -> i32 {
    let mut sorted: Vec<i32> = heights.values().copied().collect();
    sorted.sort();

    match slope {
        0..=3 => percentile(&sorted, 50),
        _ => percentile(&sorted, 75),
    }
}

/// Returns the value at the given percentile (0-100) from a sorted slice.
fn percentile(sorted: &[i32], pct: u32) -> i32 {
    let idx = ((sorted.len() - 1) as f64 * pct as f64 / 100.0).round() as usize;
    sorted[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_median() {
        assert_eq!(percentile(&[1, 2, 3, 4, 5], 50), 3);
    }

    #[test]
    fn percentile_75th() {
        assert_eq!(percentile(&[1, 2, 3, 4, 5], 75), 4);
    }

    #[test]
    fn percentile_single() {
        assert_eq!(percentile(&[42], 50), 42);
        assert_eq!(percentile(&[42], 75), 42);
    }

    #[test]
    fn choose_base_y_flat() {
        let heights: HashMap<Point2D, i32> = (0..9)
            .map(|i| (Point2D::new(i, 0), 64))
            .collect();
        assert_eq!(choose_base_y(&heights, 0), 64);
    }

    #[test]
    fn choose_base_y_gentle_slope() {
        let mut heights = HashMap::new();
        for i in 0..5 {
            heights.insert(Point2D::new(i, 0), 64 + i);
        }
        // slope = 4, heights = [64, 65, 66, 67, 68], 75th percentile = 67
        assert_eq!(choose_base_y(&heights, 4), 67);
    }

    #[test]
    fn choose_base_y_uses_median_for_small_slope() {
        let mut heights = HashMap::new();
        heights.insert(Point2D::new(0, 0), 60);
        heights.insert(Point2D::new(1, 0), 62);
        heights.insert(Point2D::new(2, 0), 63);
        // slope = 3, median = 62
        assert_eq!(choose_base_y(&heights, 3), 62);
    }

    #[test]
    fn classify_columns_mixed() {
        let mut heights = HashMap::new();
        heights.insert(Point2D::new(0, 0), 60); // below base
        heights.insert(Point2D::new(1, 0), 64); // at base
        heights.insert(Point2D::new(2, 0), 67); // above base

        let profile = TerrainProfile {
            heights,
            min_height: 60,
            max_height: 67,
            base_y: 64,
        };

        let columns = classify_columns(&profile);
        assert_eq!(columns.len(), 2); // point at base_y is skipped

        assert!(matches!(
            columns.get(&Point2D::new(0, 0)),
            Some(ColumnAction::Fill { terrain_y: 60 })
        ));
        assert!(matches!(
            columns.get(&Point2D::new(2, 0)),
            Some(ColumnAction::Cut { terrain_y: 67 })
        ));
        assert!(columns.get(&Point2D::new(1, 0)).is_none());
    }
}
