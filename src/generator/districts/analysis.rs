

use crate::editor::Editor;
use crate::geometry::CARDINALS_2D;
use crate::geometry::DOWN;
use crate::minecraft::Biome;
use crate::minecraft::BlockID;
use std::collections::HashMap;

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
            surface_block_count: HashMap::new(),
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

pub async fn analyze_parcel<'a, TID : 'a>(area: &ParcelData<TID>, editor: &Editor) -> ParcelAnalysis {
    let average = area.average();
    let average_height = average.y;
    let number_of_points = area.points().len() as f32;

    let mut water_blocks = 0;
    let mut leaf_blocks = 0;
    let mut neighbour_height_sum = 0.0;
    let mut root_mean_square_height = 0.0;

    let mut biome_count: HashMap<Biome, u32> = HashMap::new();
    let mut surface_block_count: HashMap<BlockID, u32> = HashMap::new();


    for point in area.points() {
        let biome = editor.world().get_surface_biome_at(point.drop_y());
        let block = editor.get_block(*point + DOWN);
        let is_water = block.id.is_water();
        let leaf_height = editor.world().get_motion_blocking_height_at(point.drop_y());

        root_mean_square_height += ((point.y - average_height) as f32).powi(2);

        let height = editor.world().get_non_tree_height(point.drop_y());
        let average_neighbour_height = CARDINALS_2D.iter()
            .map(|cardinal| {
                let neighbour = point.drop_y() + *cardinal;
                if editor.world().is_in_bounds_2d(neighbour) {
                    (height - editor.world().get_non_tree_height(neighbour)).abs()
                } else {
                    0
                }
            })
            .sum::<i32>() as f32 / 4.0;

        neighbour_height_sum += average_neighbour_height;

        *biome_count.entry(biome).or_insert(0) += 1;
        *surface_block_count.entry(block.id).or_insert(0) += 1;

        if is_water {
            water_blocks += 1;
        }
        if point.y < leaf_height {
            leaf_blocks += 1;
        }
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