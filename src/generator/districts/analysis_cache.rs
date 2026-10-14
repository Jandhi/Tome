//! Memoised per-cell world lookups for [`analyze_parcel`](super::analysis::analyze_parcel).

use crate::editor::Editor;
use crate::geometry::{Point3D, CARDINALS_2D, DOWN};
use crate::minecraft::{Biome, BlockID};
use crate::util::HashMap;

/// Per-cell inputs to `analyze_parcel`: everything it reads from the world for one
/// surface point, so repeated analyses of the same cells (merge_down re-analyzes the
/// growing parent after every merge) don't redo the block/biome lookups. Biomes and
/// block ids are interned to indices into the cache's tables.
#[derive(Clone, Copy)]
pub(super) struct CellInputs {
    /// The point's y — the cache is keyed by (x, z), so a different y is a miss.
    pub(super) y: i32,
    /// `None` when the surface biome lookup failed (the point is skipped entirely).
    pub(super) biome: Option<u32>,
    pub(super) block: u32,
    pub(super) is_water: bool,
    pub(super) leaf_height: Option<i32>,
    /// Mean absolute height step to the 4 neighbours; `None` when the non-tree height is missing.
    pub(super) neighbour_step: Option<f32>,
}

/// Lazily-filled per-cell [`CellInputs`] grid. Valid for as long as the world terrain
/// and the editor's block cache are unchanged (i.e. within one `generate_parcels`).
pub struct AnalysisCache {
    size_z: i32,
    size_x: i32,
    cells: Vec<Option<CellInputs>>,
    pub(super) biomes: Vec<Biome>,
    biome_ids: HashMap<Biome, u32>,
    pub(super) blocks: Vec<BlockID>,
    block_ids: HashMap<BlockID, u32>,
}

impl AnalysisCache {
    pub fn new(editor: &Editor) -> Self {
        let size = editor.world().build_area.size;
        AnalysisCache {
            size_x: size.x,
            size_z: size.z,
            cells: vec![None; (size.x.max(0) as usize) * (size.z.max(0) as usize)],
            biomes: Vec::new(),
            biome_ids: HashMap::default(),
            blocks: Vec::new(),
            block_ids: HashMap::default(),
        }
    }

    fn intern_biome(&mut self, biome: Biome) -> u32 {
        if let Some(&id) = self.biome_ids.get(&biome) {
            return id;
        }
        let id = self.biomes.len() as u32;
        self.biomes.push(biome.clone());
        self.biome_ids.insert(biome, id);
        id
    }

    fn intern_block(&mut self, block: BlockID) -> u32 {
        if let Some(&id) = self.block_ids.get(&block) {
            return id;
        }
        let id = self.blocks.len() as u32;
        self.blocks.push(block.clone());
        self.block_ids.insert(block, id);
        id
    }

    /// Exactly the lookups the original per-point loop made, in the same order.
    fn compute(&mut self, point: Point3D, editor: &Editor) -> CellInputs {
        let mut inputs = CellInputs { y: point.y, biome: None, block: 0, is_water: false, leaf_height: None, neighbour_step: None };
        let Some(biome) = editor.world().get_surface_biome_at(point.drop_y()) else { return inputs; };
        inputs.biome = Some(self.intern_biome(biome));
        let block = editor.get_block(point + DOWN);
        inputs.is_water = block.id.is_water();
        inputs.block = self.intern_block(block.id);
        inputs.leaf_height = editor.world().get_motion_blocking_height_at(point.drop_y());
        if inputs.leaf_height.is_none() {
            return inputs;
        }
        let Some(height) = editor.world().get_non_tree_height(point.drop_y()) else { return inputs; };
        inputs.neighbour_step = Some(CARDINALS_2D.iter()
            .filter_map(|cardinal| {
                let neighbour = point.drop_y() + *cardinal;
                if editor.world().is_in_bounds_2d(neighbour) {
                    editor.world().get_non_tree_height(neighbour).map(|nh| (height - nh).abs())
                } else {
                    Some(0)
                }
            })
            .sum::<i32>() as f32 / 4.0);
        inputs
    }

    pub(super) fn get(&mut self, point: Point3D, editor: &Editor) -> CellInputs {
        if point.x < 0 || point.z < 0 || point.x >= self.size_x || point.z >= self.size_z {
            return self.compute(point, editor);
        }
        let index = point.x as usize * self.size_z as usize + point.z as usize;
        match self.cells[index] {
            Some(inputs) if inputs.y == point.y => inputs,
            Some(_) => self.compute(point, editor),
            None => {
                let inputs = self.compute(point, editor);
                self.cells[index] = Some(inputs);
                inputs
            }
        }
    }
}
