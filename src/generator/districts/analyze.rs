use crate::editor::World;

use super::district::District;



fn district_analyze(district : &mut District, world : &mut World) {
    let average = district.average();
    let average_height = average.y;
    let mut water_blocks = 0;
    let mut leaf_blocks = 0;
    let mut neighbour_height = 0;
    let number_of_points = district.points().len();

    let root_mean_square_height = 0.0;

    for point in district.points() {
        let biome = world.get_surface_biome_at(point.drop_y());
        
    }
}