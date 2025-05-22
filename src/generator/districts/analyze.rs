use crate::{editor::{self, World}, geometry::{CARDINALS_2D, NORTH_2D}};

use super::district::District;



async fn district_analyze(district : &mut District, world : &mut World) {
    let average = district.average();
    let average_height = average.y;
    let mut water_blocks = 0;
    let mut leaf_blocks = 0;
    let mut neighbour_height = 0.0;
    let number_of_points = district.points().len();

    let mut root_mean_square_height = 0.0;

    let mut editor = world.get_editor();

    for point in district.points() {
        let biome = world.get_surface_biome_at(point.drop_y());
        let block = editor.get_block(*point).await;
        let is_water = block.id.is_water();
        let leaf_height = world.get_motion_blocking_height_at(point.drop_y());

        root_mean_square_height += f64::powi((point.y - average_height) as f64, 2);

        let height = world.get_height_at(point.drop_y());
        let mut average_neighbour_height = CARDINALS_2D.iter()
            .map(|cardinal| {
                let neighbour = point.drop_y() + *cardinal;
                if world.is_in_bounds_2d(neighbour) {
                    world.get_height_at(neighbour)
                } else {
                    height
                }
            })
            .sum::<i32>() as f32 / 4.0;
        
        neighbour_height += average_neighbour_height;

        
    }
}