

use crate::editor::Editor;
use crate::geometry::CARDINALS_2D;
use crate::geometry::DOWN;
use crate::geometry::Point3D;
use crate::minecraft::Biome;
use crate::minecraft::BlockID;
use crate::util::HashMap;

use super::data::HasParcelData;
use super::ParcelData;

#[derive(Debug, Clone)]
pub struct ParcelAnalysis {
    count : usize,
    roughness: f32,
    water_percentage: f32,
    forested_percentage: f32,
    surface_block_count: HashMap<BlockID, u32>,
    biome_count: HashMap<Biome, u32>,
    gradient: f32,
}


impl ParcelAnalysis {
    /// Construct a `ParcelAnalysis` from a biome distribution. All other fields
    /// (roughness, water, etc.) are zeroed. Useful for testing and synthetic parcels.
    pub fn from_biome_count(biome_count: HashMap<Biome, u32>) -> Self {
        let count = biome_count.values().sum::<u32>() as usize;
        ParcelAnalysis {
            count: count.max(1),
            roughness: 0.0,
            water_percentage: 0.0,
            forested_percentage: 0.0,
            surface_block_count: HashMap::default(),
            biome_count,
            gradient: 0.0,
        }
    }

    /// Returns all biomes that make up at least 30% of this parcel.
    pub fn major_biomes(&self) -> Vec<&Biome> {
        self.biome_count.iter()
            .filter(|(_, &count)| count as f32 / self.count as f32 >= 0.30)
            .map(|(biome, _)| biome)
            .collect()
    }

    pub fn roughness(&self) -> f32 {
        self.roughness
    }

    pub fn water_percentage(&self) -> f32 {
        self.water_percentage
    }

    pub fn forested_percentage(&self) -> f32 {
        self.forested_percentage
    }

    pub fn surface_block_count(&self) -> &HashMap<BlockID, u32> {
        &self.surface_block_count
    }

    pub fn biome_count(&self) -> &HashMap<Biome, u32> {
        &self.biome_count
    }

    pub fn biome_percentage(&self, biome: &Biome) -> f32 {
        if let Some(count) = self.biome_count.get(biome) {
            *count as f32 / self.count as f32
        } else {
            0.0
        }
    }

    pub fn gradient(&self) -> f32 {
        self.gradient
    }
}

/// Per-cell inputs to [`analyze_parcel`]: everything it reads from the world for one
/// surface point, so repeated analyses of the same cells (merge_down re-analyzes the
/// growing parent after every merge) don't redo the block/biome lookups. Biomes and
/// block ids are interned to indices into the cache's tables.
#[derive(Clone, Copy)]
struct CellInputs {
    /// The point's y — the cache is keyed by (x, z), so a different y is a miss.
    y: i32,
    /// `None` when the surface biome lookup failed (the point is skipped entirely).
    biome: Option<u32>,
    block: u32,
    is_water: bool,
    leaf_height: Option<i32>,
    /// Mean absolute height step to the 4 neighbours; `None` when the non-tree height is missing.
    neighbour_step: Option<f32>,
}

/// Lazily-filled per-cell [`CellInputs`] grid. Valid for as long as the world terrain
/// and the editor's block cache are unchanged (i.e. within one `generate_parcels`).
pub struct AnalysisCache {
    size_z: i32,
    size_x: i32,
    cells: Vec<Option<CellInputs>>,
    biomes: Vec<Biome>,
    biome_ids: HashMap<Biome, u32>,
    blocks: Vec<BlockID>,
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

    fn get(&mut self, point: Point3D, editor: &Editor) -> CellInputs {
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

/// Bump `id`'s count in a first-occurrence-ordered tally (`slot[id]` = its index in `tally`).
fn bump(tally: &mut Vec<(u32, u32)>, slot: &mut Vec<u32>, id: u32) {
    let id = id as usize;
    if id >= slot.len() {
        slot.resize(id + 1, u32::MAX);
    }
    if slot[id] == u32::MAX {
        slot[id] = tally.len() as u32;
        tally.push((id as u32, 0));
    }
    tally[slot[id] as usize].1 += 1;
}

pub fn analyze_parcel<'a, TID : 'a>(area: &ParcelData<TID>, editor: &Editor, cache: &mut AnalysisCache) -> ParcelAnalysis {
    let average = area.average();
    let average_height = average.y;
    let number_of_points = area.points().len() as f32;

    let mut water_blocks = 0;
    let mut leaf_blocks = 0;
    let mut neighbour_height_sum = 0.0;
    let mut root_mean_square_height = 0.0;

    // Tallies in first-occurrence order, turned into the HashMaps at the end by inserting
    // the keys in that same order — so the maps are laid out exactly as if built by
    // per-point `entry(..) += 1` (which is what this loop used to do).
    let (mut biome_tally, mut biome_slot) = (Vec::new(), Vec::new());
    let (mut block_tally, mut block_slot) = (Vec::new(), Vec::new());

    for point in area.points() {
        let cell = cache.get(*point, editor);
        let Some(biome) = cell.biome else { continue; };
        let Some(leaf_height) = cell.leaf_height else { continue; };

        root_mean_square_height += ((point.y - average_height) as f32).powi(2);

        let Some(average_neighbour_height) = cell.neighbour_step else { continue; };

        neighbour_height_sum += average_neighbour_height;

        bump(&mut biome_tally, &mut biome_slot, biome);
        bump(&mut block_tally, &mut block_slot, cell.block);

        if cell.is_water {
            water_blocks += 1;
        }
        if point.y < leaf_height {
            leaf_blocks += 1;
        }
    }

    let mut biome_count: HashMap<Biome, u32> = HashMap::default();
    for (id, count) in biome_tally {
        biome_count.insert(cache.biomes[id as usize].clone(), count);
    }
    let mut surface_block_count: HashMap<BlockID, u32> = HashMap::default();
    for (id, count) in block_tally {
        surface_block_count.insert(cache.blocks[id as usize].clone(), count);
    }

    let num_points = if number_of_points == 0.0 { 1.0 } else { number_of_points };
    ParcelAnalysis {
        count: area.points().len(),
        roughness: (root_mean_square_height / num_points).sqrt(),
        gradient: neighbour_height_sum / num_points,
        water_percentage: (water_blocks as f32 / num_points),
        forested_percentage: (leaf_blocks as f32 / num_points),
        surface_block_count,
        biome_count,
    }
}
