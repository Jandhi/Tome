#[cfg(test)]
mod tests {
    use lerp::num_traits::Signed;
    use crate::{editor::World, generator::{data::LoadedData, materials::MaterialId, paths::{a_star, building::build_path, path::PathPriority, routing::{get_path, route_path}}}, geometry::Point3D, http_mod::GDMCHTTPProvider, noise::RNG, util::init_logger};
    use std::time::Instant;

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
            |prev_cost, _| {
                prev_cost + 1
            },
            |node| {
                let (x, y) = *node.last().unwrap();
                ((target.0 - x).abs() + (target.1 - y).abs()) as u64 // Heuristic: Manhattan distance to target
            },
            async |_| {},
        ).await.expect("A* algorithm failed to find a path");

        let duration = start_time.elapsed();

        println!("Path found: {:?}", path);
        println!("A* search took: {:?}", duration);
    }

    #[tokio::test]
    async fn test_route() {
        init_logger();

        let world = World::new(&GDMCHTTPProvider::new()).await.expect("Failed to create world");
        let editor = world.get_editor();

        let mut editor2 = World::new(&GDMCHTTPProvider::new()).await.expect("Failed to create world").get_editor();
        editor2.set_buffer_size(1);

        let rect = editor.world().world_rect_2d();

        let start = editor.world().add_height(rect.origin);
        let end = editor.world().add_height(rect.max());

        
        let path = route_path(&editor, start, end, async |point : &Vec<Point3D>| {
            editor2.place_block(&"pink_wool".into(), *point.iter().last().unwrap()).await
        }).await.expect("Failed to route path");

        for point in path {
            editor.place_block(&"red_wool".into(), point).await;
        }

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn build() {
        init_logger();

        let world = World::new(&GDMCHTTPProvider::new()).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let rect = editor.world().world_rect_2d();

        let start = editor.world().add_height(rect.origin);
        let end = editor.world().add_height(rect.max());

        let data = LoadedData::load().expect("Failed to load data");

        
        let path = get_path(&editor, start, end, PathPriority::Medium, MaterialId::new("cobblestone".to_string()), async |_| {}).await.expect("Failed to route path");
        let mut rng = RNG::new(42);
        build_path(&mut editor, &data, &path, &mut rng).await;

        editor.flush_buffer().await;
    }

    /// Offline: lamps land just off the pavement, evenly spaced and staggered to
    /// both sides of a straight arterial.
    #[tokio::test]
    async fn street_lights_line_the_verge_offline() {
        use crate::editor::World;
        use crate::generator::materials::MaterialId;
        use crate::generator::paths::{place_street_lights, Path, PathPriority};
        use crate::geometry::Rect3D;

        let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
        let world = World::synthetic(build_area, 64);
        let editor = world.get_offline_editor();

        // Straight east-west arterial, width 3, along z = 32 from x=10..=60.
        let width = 3u32;
        let z = 32;
        let pts: Vec<Point3D> = (10..=60).map(|x| Point3D::new(x, 64, z)).collect();
        let road = Path::new(pts, width, MaterialId::new("cobblestone".to_string()), PathPriority::High);

        let lantern: crate::minecraft::Block = "minecraft:lantern".into();
        let lamps = place_street_lights(&editor, &[road], &lantern).await;

        // A 50-long road at spacing 7 should give a handful of lamps.
        assert!(lamps.len() >= 5, "expected several lamps, got {}", lamps.len());

        let paved_half = (width - 1) as i32; // widen reach from the centreline
        let mut saw_north = false;
        let mut saw_south = false;
        for lamp in &lamps {
            // Off the pavement: perpendicular offset clears the widened shoulder.
            let perp = (lamp.y - z).abs();
            assert!(
                perp > paved_half,
                "lamp at {:?} sits on the pavement (perp {} <= {})",
                lamp, perp, paved_half
            );
            // Beside the road, not wandered off down its length.
            assert!(lamp.x >= 10 && lamp.x <= 60, "lamp at {:?} off the road span", lamp);
            if lamp.y < z { saw_north = true; }
            if lamp.y > z { saw_south = true; }
        }
        assert!(saw_north && saw_south, "lamps should stagger to both sides");

        // No two lamps crowd each other.
        for (i, a) in lamps.iter().enumerate() {
            for b in &lamps[i + 1..] {
                assert!(
                    a.distance_squared(b) >= 16,
                    "lamps {:?} and {:?} are too close", a, b
                );
            }
        }
    }

    /// Offline: alley-tier (Low priority, width 1) roads are lit too.
    #[tokio::test]
    async fn street_lights_light_alleys_offline() {
        use crate::editor::World;
        use crate::generator::materials::MaterialId;
        use crate::generator::paths::{place_street_lights, Path, PathPriority};
        use crate::geometry::Rect3D;

        let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
        let world = World::synthetic(build_area, 64);
        let editor = world.get_offline_editor();

        let pts: Vec<Point3D> = (10..=60).map(|x| Point3D::new(x, 64, 32)).collect();
        let alley = Path::new(pts, 1, MaterialId::new("cobblestone".to_string()), PathPriority::Low);

        let lantern: crate::minecraft::Block = "minecraft:lantern".into();
        let lamps = place_street_lights(&editor, &[alley], &lantern).await;
        assert!(!lamps.is_empty(), "alleys should be lit");
        // A width-1 alley pavement is just the centreline (z=32); lamps sit one
        // cell off it.
        for lamp in &lamps {
            assert!((lamp.y - 32).abs() >= 1, "lamp at {:?} on the alley centreline", lamp);
        }
    }
}