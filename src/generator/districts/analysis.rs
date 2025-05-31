

use crate::editor::World;
use crate::geometry::CARDINALS_2D;
use crate::minecraft::Biome;
use crate::minecraft::BlockID;
use std::collections::HashMap;

use super::data::HasDistrictData;
use super::DistrictData;

#[derive(Debug, Clone)]
pub struct DistrictAnalysis {
    count : usize,
    roughness: f32,
    water_percentage: f32,
    forested_percentage: f32,
    surface_block_count: HashMap<BlockID, u32>,
    biome_count: HashMap<Biome, u32>,
    gradient: f32,
}


impl DistrictAnalysis {
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
            (count * 100) as f32 / self.count as f32
        } else {
            0.0
        }
    }

    pub fn gradient(&self) -> f32 {
        self.gradient
    }
}

pub async fn analyze_district<'a, TID : 'a>(area: &DistrictData<TID>, world: &mut World) -> DistrictAnalysis {
    let average = area.average();
    let average_height = average.y;
    let number_of_points = area.points().len() as f32;

    let mut water_blocks = 0;
    let mut leaf_blocks = 0;
    let mut neighbour_height_sum = 0.0;
    let mut root_mean_square_height = 0.0;

    let mut biome_count: HashMap<Biome, u32> = HashMap::new();
    let mut surface_block_count: HashMap<BlockID, u32> = HashMap::new();

    let mut editor = world.get_editor();
    
    for point in area.points() {
        let biome = world.get_surface_biome_at(point.drop_y());
        let block = editor.get_block(*point, &world);
        let is_water = block.id.is_water();
        let leaf_height = world.get_motion_blocking_height_at(point.drop_y());

        root_mean_square_height += ((point.y - average_height) as f32).powi(2);

        let height = world.get_height_at(point.drop_y());
        let average_neighbour_height = CARDINALS_2D.iter()
            .map(|cardinal| {
                let neighbour = point.drop_y() + *cardinal;
                if world.is_in_bounds_2d(neighbour) {
                    world.get_height_at(neighbour)
                } else {
                    height
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
    DistrictAnalysis {
        count: area.points().len(),
        roughness: (root_mean_square_height / num_points).sqrt(),
        gradient: neighbour_height_sum / num_points,
        water_percentage: (water_blocks as f32 / num_points) * 100.0,
        forested_percentage: (leaf_blocks as f32 / num_points) * 100.0,
        surface_block_count,
        biome_count,
    }
}