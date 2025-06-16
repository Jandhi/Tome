#[cfg(test)]
mod tests {
    use lerp::num_traits::Signed;
    use log::info;

    use crate::{generator::paths::a_star, util::init_logger};
use std::time::Instant;

    #[test]
    fn test_a_star() {

        init_logger();

        let target = (1000, 1000);

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
            |node| {
                node.len() as u64 // Cost: number of steps taken
            },
            |node| {
                let (x, y) = *node.last().unwrap();
                ((target.0 - x).abs() + (target.1 - y).abs()) as u64 // Heuristic: Manhattan distance to target
            },
            |node| {},
        ).expect("A* algorithm failed to find a path");

        let duration = start_time.elapsed();

        println!("Path found: {:?}", path);
        println!("A* search took: {:?}", duration);
    }
}