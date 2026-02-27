use std::collections::HashMap;

use crate::editor::Editor;
use crate::generator::materials::{Material, MaterialId, Palette};
use crate::geometry::{Cardinal, Point2D, Point3D};
use crate::minecraft::Block;
use crate::noise::RNG;

use super::super::footprint::Footprint;
use super::{RoofMaterials, RoofPitch};

/// A heightmap entry storing the height and facing direction for a roof position.
#[derive(Debug, Clone, Copy)]
struct HeightmapEntry {
    /// Distance from edge (determines height based on pitch).
    distance: i32,
    /// Direction the stair should face (toward nearest edge).
    facing: Cardinal,
}

/// Pre-computed heightmap for a composite roof.
#[derive(Debug)]
pub struct RoofHeightmap {
    entries: HashMap<Point2D, HeightmapEntry>,
    base_y: i32,
    pitch: RoofPitch,
}

impl RoofHeightmap {
    /// Create an empty heightmap.
    pub fn new(base_y: i32, pitch: RoofPitch) -> Self {
        Self {
            entries: HashMap::new(),
            base_y,
            pitch,
        }
    }

    /// Compute the heightmap for a single rectangular footprint.
    pub fn compute_for_rect(&mut self, footprint: &Footprint, overhang: i32) {
        let Some((bounds_min, bounds_max)) = footprint.bounds() else {
            return;
        };

        // Calculate roof dimensions with overhang
        let roof_min_x = bounds_min.x - overhang;
        let roof_max_x = bounds_max.x + overhang;
        let roof_min_z = bounds_min.y - overhang;
        let roof_max_z = bounds_max.y + overhang;

        for z in roof_min_z..=roof_max_z {
            for x in roof_min_x..=roof_max_x {
                let point = Point2D::new(x, z);

                // Skip if outside overhang range
                if !footprint.is_within_distance(point, overhang) {
                    continue;
                }

                // Calculate distance to nearest edge
                let edge_dist = footprint.distance_to_edge(point);

                // If inside footprint, add 1 for consistent slope from overhang
                let distance = if footprint.contains(point) {
                    edge_dist + 1
                } else {
                    0
                };

                // Find facing direction toward nearest edge
                let facing = find_facing_for_rect(point, bounds_min, bounds_max);

                // Merge with existing entry - take the maximum distance (higher roof)
                let entry = HeightmapEntry { distance, facing };
                self.merge_entry(point, entry);
            }
        }
    }

    /// Merge an entry, keeping the one with greater distance (higher roof).
    fn merge_entry(&mut self, point: Point2D, new_entry: HeightmapEntry) {
        match self.entries.get(&point) {
            Some(existing) if existing.distance >= new_entry.distance => {
                // Keep existing - it's higher or equal
            }
            _ => {
                // New entry is higher, use it
                self.entries.insert(point, new_entry);
            }
        }
    }

    /// Place the roof blocks based on the computed heightmap.
    pub async fn place(
        &self,
        editor: &Editor,
        palette: &Palette,
        materials: &HashMap<MaterialId, Material>,
        rng: &mut RNG,
    ) {
        let roof_mats = RoofMaterials::from_palette(palette, materials, rng);

        for (point, entry) in &self.entries {
            match self.pitch {
                RoofPitch::Shallow => {
                    self.place_shallow_block(editor, &roof_mats, point.x, point.y, entry.distance).await;
                }
                RoofPitch::Medium => {
                    self.place_medium_block(editor, &roof_mats, point.x, point.y, entry.distance, entry.facing).await;
                }
                RoofPitch::Steep => {
                    self.place_steep_block(editor, &roof_mats, point.x, point.y, entry.distance, entry.facing).await;
                }
            }
        }
    }

    async fn place_shallow_block(
        &self,
        editor: &Editor,
        mats: &RoofMaterials,
        x: i32,
        z: i32,
        min_dist: i32,
    ) {
        let y_offset = min_dist / 2;
        let is_top_slab_row = min_dist % 2 == 1;
        let y = self.base_y + y_offset;
        let pos = Point3D::new(x, y, z);

        if min_dist == 0 {
            // Edge: just a bottom slab
            let mut state = HashMap::new();
            state.insert("type".to_string(), "bottom".to_string());
            let slab = Block::new(mats.slab.clone(), Some(state), None);
            editor.place_block(&slab, pos).await;
        } else if is_top_slab_row {
            // Top slab rows become full blocks
            let block = Block::from(mats.solid.clone());
            editor.place_block(&block, pos).await;
        } else {
            // Bottom slab rows: place bottom slab + top slab below
            let mut state = HashMap::new();
            state.insert("type".to_string(), "bottom".to_string());
            let slab = Block::new(mats.slab.clone(), Some(state), None);
            editor.place_block(&slab, pos).await;

            // Place top slab one block below
            let pos_below = Point3D::new(x, y - 1, z);
            let mut state_below = HashMap::new();
            state_below.insert("type".to_string(), "top".to_string());
            let slab_below = Block::new(mats.slab.clone(), Some(state_below), None);
            editor.place_block(&slab_below, pos_below).await;
        }
    }

    async fn place_medium_block(
        &self,
        editor: &Editor,
        mats: &RoofMaterials,
        x: i32,
        z: i32,
        min_dist: i32,
        facing: Cardinal,
    ) {
        // Stairs, +1 y per distance
        let y_offset = min_dist;
        let y = self.base_y + y_offset;
        let pos = Point3D::new(x, y, z);

        let mut state = HashMap::new();
        state.insert("facing".to_string(), facing.to_string());
        let stair = Block::new(mats.stairs.clone(), Some(state), None);
        editor.place_block(&stair, pos).await;
    }

    async fn place_steep_block(
        &self,
        editor: &Editor,
        mats: &RoofMaterials,
        x: i32,
        z: i32,
        min_dist: i32,
        facing: Cardinal,
    ) {
        // Block + stair per distance unit, +2 y per distance
        let y_offset = min_dist * 2;
        let y = self.base_y + y_offset;

        // Place solid block
        let pos_block = Point3D::new(x, y, z);
        let solid = Block::from(mats.solid.clone());
        editor.place_block(&solid, pos_block).await;

        // Place stair above
        let pos_stair = Point3D::new(x, y + 1, z);
        let mut state = HashMap::new();
        state.insert("facing".to_string(), facing.to_string());
        let stair = Block::new(mats.stairs.clone(), Some(state), None);
        editor.place_block(&stair, pos_stair).await;
    }
}

/// Find the facing direction toward the nearest edge of a rectangular footprint.
fn find_facing_for_rect(point: Point2D, bounds_min: Point2D, bounds_max: Point2D) -> Cardinal {
    let dist_west = point.x - bounds_min.x;
    let dist_east = bounds_max.x - point.x;
    let dist_north = point.y - bounds_min.y;
    let dist_south = bounds_max.y - point.y;

    let min_dist = dist_west.min(dist_east).min(dist_north).min(dist_south);

    if min_dist == dist_west {
        Cardinal::West
    } else if min_dist == dist_east {
        Cardinal::East
    } else if min_dist == dist_north {
        Cardinal::North
    } else {
        Cardinal::South
    }
}

/// Place a composite hip roof over multiple rectangular footprints.
/// Uses a heightmap approach: computes heights for each rectangle,
/// takes the maximum at each position, then places blocks.
pub async fn place_composite_hip_roof(
    rectangles: &[Footprint],
    base_y: i32,
    pitch: RoofPitch,
    overhang: i32,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    // Build the heightmap from all rectangles
    let mut heightmap = RoofHeightmap::new(base_y, pitch);

    for rect in rectangles {
        heightmap.compute_for_rect(rect, overhang);
    }

    // Place the roof blocks
    heightmap.place(editor, palette, materials, rng).await;
}

/// Gable-specific heightmap entry.
#[derive(Debug, Clone, Copy)]
struct GableHeightmapEntry {
    /// Distance from slope edge (determines height based on pitch).
    distance: i32,
    /// Direction the stair should face (toward slope edge).
    facing: Cardinal,
    /// Whether this position is at or beyond the ridge (needs slab cap).
    is_ridge: bool,
}

/// Pre-computed heightmap for a composite gable roof.
#[derive(Debug)]
pub struct GableRoofHeightmap {
    entries: HashMap<Point2D, GableHeightmapEntry>,
    base_y: i32,
    pitch: RoofPitch,
}

impl GableRoofHeightmap {
    /// Create an empty gable heightmap.
    pub fn new(base_y: i32, pitch: RoofPitch) -> Self {
        Self {
            entries: HashMap::new(),
            base_y,
            pitch,
        }
    }

    /// Compute the gable heightmap for a single rectangular footprint.
    /// Ridge runs along the longest axis.
    pub fn compute_for_rect(&mut self, footprint: &Footprint, overhang: i32) {
        let Some((bounds_min, bounds_max)) = footprint.bounds() else {
            return;
        };

        let width = bounds_max.x - bounds_min.x + 1;
        let depth = bounds_max.y - bounds_min.y + 1;

        // Ridge runs along the longest axis
        let ridge_along_z = depth >= width;

        // Calculate roof dimensions with overhang
        let roof_min_x = bounds_min.x - overhang;
        let roof_max_x = bounds_max.x + overhang;
        let roof_min_z = bounds_min.y - overhang;
        let roof_max_z = bounds_max.y + overhang;

        // Slope dimension (perpendicular to ridge)
        let (slope_min, slope_max) = if ridge_along_z {
            (roof_min_x, roof_max_x)
        } else {
            (roof_min_z, roof_max_z)
        };

        let slope_span = slope_max - slope_min + 1;
        let half_span = slope_span / 2;
        let center = slope_min + half_span;

        for z in roof_min_z..=roof_max_z {
            for x in roof_min_x..=roof_max_x {
                let point = Point2D::new(x, z);

                // Skip if outside overhang range
                if !footprint.is_within_distance(point, overhang) {
                    continue;
                }

                // Calculate distance from slope edge (perpendicular to ridge)
                let slope_pos = if ridge_along_z { x } else { z };
                let dist_to_low = slope_pos - slope_min;
                let dist_to_high = slope_max - slope_pos;
                let distance = dist_to_low.min(dist_to_high);

                // Determine if at ridge
                let is_ridge = slope_pos == center || (slope_span % 2 == 0 && slope_pos == center - 1);

                // Facing toward the nearest slope edge
                let facing = if ridge_along_z {
                    if dist_to_low <= dist_to_high { Cardinal::West } else { Cardinal::East }
                } else {
                    if dist_to_low <= dist_to_high { Cardinal::North } else { Cardinal::South }
                };

                let entry = GableHeightmapEntry { distance, facing, is_ridge };
                self.merge_entry(point, entry);
            }
        }
    }

    /// Merge an entry, keeping the one with greater distance (higher roof).
    fn merge_entry(&mut self, point: Point2D, new_entry: GableHeightmapEntry) {
        match self.entries.get(&point) {
            Some(existing) if existing.distance >= new_entry.distance => {
                // Keep existing - it's higher or equal
            }
            _ => {
                // New entry is higher, use it
                self.entries.insert(point, new_entry);
            }
        }
    }

    /// Place the gable roof blocks based on the computed heightmap.
    pub async fn place(
        &self,
        editor: &Editor,
        palette: &Palette,
        materials: &HashMap<MaterialId, Material>,
        rng: &mut RNG,
    ) {
        let roof_mats = RoofMaterials::from_palette(palette, materials, rng);

        for (point, entry) in &self.entries {
            match self.pitch {
                RoofPitch::Shallow => {
                    self.place_shallow_gable_block(editor, &roof_mats, point.x, point.y, entry).await;
                }
                RoofPitch::Medium => {
                    self.place_medium_gable_block(editor, &roof_mats, point.x, point.y, entry).await;
                }
                RoofPitch::Steep => {
                    self.place_steep_gable_block(editor, &roof_mats, point.x, point.y, entry).await;
                }
            }
        }
    }

    async fn place_shallow_gable_block(
        &self,
        editor: &Editor,
        mats: &RoofMaterials,
        x: i32,
        z: i32,
        entry: &GableHeightmapEntry,
    ) {
        let y_offset = entry.distance / 2;
        let y = self.base_y + y_offset;
        let pos = Point3D::new(x, y, z);

        // Ridge cap is always bottom slab
        let is_top_slab = entry.distance % 2 == 1 && !entry.is_ridge;

        let mut state = HashMap::new();
        state.insert("type".to_string(), if is_top_slab { "top" } else { "bottom" }.to_string());
        let slab = Block::new(mats.slab.clone(), Some(state), None);
        editor.place_block(&slab, pos).await;
    }

    async fn place_medium_gable_block(
        &self,
        editor: &Editor,
        mats: &RoofMaterials,
        x: i32,
        z: i32,
        entry: &GableHeightmapEntry,
    ) {
        let y_offset = entry.distance;
        let y = self.base_y + y_offset;
        let pos = Point3D::new(x, y, z);

        if entry.is_ridge {
            // Ridge cap: bottom slab
            let mut state = HashMap::new();
            state.insert("type".to_string(), "bottom".to_string());
            let slab = Block::new(mats.slab.clone(), Some(state), None);
            editor.place_block(&slab, pos).await;
        } else {
            // Regular stair
            let mut state = HashMap::new();
            state.insert("facing".to_string(), entry.facing.to_string());
            let stair = Block::new(mats.stairs.clone(), Some(state), None);
            editor.place_block(&stair, pos).await;
        }
    }

    async fn place_steep_gable_block(
        &self,
        editor: &Editor,
        mats: &RoofMaterials,
        x: i32,
        z: i32,
        entry: &GableHeightmapEntry,
    ) {
        let y_offset = entry.distance * 2;
        let y = self.base_y + y_offset;

        if entry.is_ridge {
            // Ridge cap: bottom slab
            let pos = Point3D::new(x, y, z);
            let mut state = HashMap::new();
            state.insert("type".to_string(), "bottom".to_string());
            let slab = Block::new(mats.slab.clone(), Some(state), None);
            editor.place_block(&slab, pos).await;
        } else if entry.distance == 0 {
            // Edge row: upside-down stair + normal stair
            let pos_bottom = Point3D::new(x, y, z);
            let mut state_bottom = HashMap::new();
            state_bottom.insert("facing".to_string(), entry.facing.to_string());
            state_bottom.insert("half".to_string(), "top".to_string());
            let stair_bottom = Block::new(mats.stairs.clone(), Some(state_bottom), None);
            editor.place_block(&stair_bottom, pos_bottom).await;

            let pos_top = Point3D::new(x, y + 1, z);
            let mut state_top = HashMap::new();
            state_top.insert("facing".to_string(), entry.facing.to_string());
            let stair_top = Block::new(mats.stairs.clone(), Some(state_top), None);
            editor.place_block(&stair_top, pos_top).await;
        } else {
            // Interior: block + stair
            let pos_block = Point3D::new(x, y, z);
            let solid = Block::from(mats.solid.clone());
            editor.place_block(&solid, pos_block).await;

            let pos_stair = Point3D::new(x, y + 1, z);
            let mut state = HashMap::new();
            state.insert("facing".to_string(), entry.facing.to_string());
            let stair = Block::new(mats.stairs.clone(), Some(state), None);
            editor.place_block(&stair, pos_stair).await;
        }
    }
}

/// Place a composite gable roof over multiple rectangular footprints.
/// Uses a heightmap approach: computes heights for each rectangle,
/// takes the maximum at each position, then places blocks.
/// Ridge direction is determined per-rectangle based on aspect ratio.
pub async fn place_composite_gable_roof(
    rectangles: &[Footprint],
    base_y: i32,
    pitch: RoofPitch,
    overhang: i32,
    editor: &Editor,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
) {
    // Build the heightmap from all rectangles
    let mut heightmap = GableRoofHeightmap::new(base_y, pitch);

    for rect in rectangles {
        heightmap.compute_for_rect(rect, overhang);
    }

    // Place the roof blocks
    heightmap.place(editor, palette, materials, rng).await;
}
