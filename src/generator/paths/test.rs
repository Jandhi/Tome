#[cfg(test)]
mod tests {
    use lerp::num_traits::Signed;
    use log::info;

    use crate::{editor::{self, World}, generator::paths::{a_star, routing::route_path}, geometry::Point3D, http_mod::GDMCHTTPProvider, minecraft::BlockID, util::init_logger};
use std::{sync::Mutex, time::Instant};

    #[tokio::test]
    async fn test_a_star() {

        init_logger();

        let target = (100, 100);

        let start_time = Instant::now();

        let path = a_star(
            vec![(0, 0)],
            |node| *node.last().unwrap() == target,
            |node| {
                let (x, y) = *node.last().unwrap();
                vec![
                    (x + 1, y), // Right
                    (x - 1, y), // Left
                    (x, y + 1), // Down
                    (x, y - 1), // Up
                ].iter().map(|pos| {
                    let mut vec = node.clone();
                    vec.push(*pos);
                    vec
                }).collect()
            },
            |prev_cost, node| {
                prev_cost + 1
            },
            |node| {
                let (x, y) = *node.last().unwrap();
                ((target.0 - x).abs() + (target.1 - y).abs()) as u64 // Heuristic: Manhattan distance to target
            },
            async |node| {},
        ).await.expect("A* algorithm failed to find a path");

        let duration = start_time.elapsed();

        println!("Path found: {:?}", path);
        println!("A* search took: {:?}", duration);
    }

    #[tokio::test]
    async fn test_route() {
        init_logger();

        let world = World::new(&GDMCHTTPProvider::new()).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let mut editor2 = World::new(&GDMCHTTPProvider::new()).await.expect("Failed to create world").get_editor();
        editor2.set_buffer_size(1);

        let rect = editor.world().world_rect_2d();

        let start = editor.world().add_height(rect.origin);
        let end = editor.world().add_height(rect.last());

        
        let path = route_path(&editor, start, end, async |point : &Vec<Point3D>| {
            editor2.place_block(&BlockID::PinkWool.into(), *point.iter().last().unwrap()).await
        }).await.expect("Failed to route path");

        for point in path {
            editor.place_block(&BlockID::RedWool.into(), point).await;
        }

        editor.flush_buffer().await;
    }
}