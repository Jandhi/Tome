#[cfg(test)]
mod tests {
    use log::info;

    use crate::{
        data::Loadable,
        editor::World,
        generator::{
            buildings_v2::{
                add_doors_to_frame, add_windows_to_frame, place_frame, place_roof, 
                generate_roof, DoorRules, Frame, Opening, WindowRules, RoofRules,
            },
            materials::Material,
        },
        geometry::Point3D,
        http_mod::GDMCHTTPProvider,
        noise::RNG,
        util::init_logger,
    };

    /// Test placing a simple rectangular building frame in Minecraft.
    /// Run with: cargo test buildings_v2::test::tests::place_simple_frame -- --nocapture
    #[tokio::test]
    async fn place_simple_frame() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let editor = world.get_editor();

        // Find placement point at world center
        let midpoint = editor.world().world_rect_2d().size / 2;
        let ground_y = editor.world().add_height(midpoint).y;

        info!("Placing building at ground level: {}", ground_y);

        // Create a simple 8x6 rectangular frame, 4 blocks tall, 1 floor
        let frame = Frame::rectangle(
            Point3D::new(midpoint.x - 4, ground_y, midpoint.y - 3),
            8,  // width (X)
            6,  // depth (Z)
            4,  // wall height
            1,  // floors
        );

        // Load materials and palette
        let materials = Material::load().expect("Failed to load materials");
        let data = crate::generator::data::LoadedData::load().expect("Failed to load data");
        let palette = data
            .palettes
            .get(&"medieval_spruce".into())
            .expect("Palette not found")
            .clone();

        let mut rng = RNG::new(42);

        // Place the frame
        place_frame(&frame, &editor, &palette, &materials, &mut rng).await;

        info!("Building frame placed successfully");
        editor.flush_buffer().await;
    }

    /// Test placing a two-story building with windows and a door.
    /// Run with: cargo test buildings_v2::test::tests::place_two_story_house -- --nocapture
    #[tokio::test]
    async fn place_two_story_house() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let editor = world.get_editor();

        let midpoint = editor.world().world_rect_2d().size / 2;
        let ground_y = editor.world().add_height(midpoint).y;

        info!("Placing two-story house at ground level: {}", ground_y);

        // Create a 10x8 frame, 5 blocks per floor, 2 floors
        let mut frame = Frame::rectangle(
            Point3D::new(midpoint.x - 5, ground_y, midpoint.y - 4),
            10, // width
            8,  // depth
            5,  // wall height per floor
            2,  // floors
        );

        // Add a door on the south wall (index 0 in edges)
        if let Some(south_wall) = frame.wall_segments_mut().get_mut(0) {
            south_wall
                .add_opening(Opening::double_door(4))
                .expect("Failed to add door");
        }

        // Add windows on the east wall (index 1)
        if let Some(east_wall) = frame.wall_segments_mut().get_mut(1) {
            east_wall
                .add_opening(Opening::large_window(2))
                .expect("Failed to add window");
            east_wall
                .add_opening(Opening::large_window(5))
                .expect("Failed to add window");
        }

        // Load materials and palette
        let materials = Material::load().expect("Failed to load materials");
        let data = crate::generator::data::LoadedData::load().expect("Failed to load data");
        let palette = data
            .palettes
            .get(&"medieval_spruce".into())
            .expect("Palette not found")
            .clone();

        let mut rng = RNG::new(123);

        // Place corners and floors (walls handled separately due to openings)
        crate::generator::buildings_v2::place_corner_posts(&frame, &editor, &palette, &materials, &mut rng).await;
        crate::generator::buildings_v2::place_floors(&frame, &editor, &palette, &materials, &mut rng).await;

        // Place walls with openings
        for floor in 0..frame.floors {
            let floor_y = frame.floor_y(floor);
            for segment in frame.wall_segments() {
                crate::generator::buildings_v2::place_wall_segment(
                    segment,
                    floor_y,
                    frame.wall_height,
                    &editor,
                    &palette,
                    &materials,
                    &mut rng,
                )
                .await;
            }
        }

        info!("Two-story house placed successfully");
        editor.flush_buffer().await;
    }

    /// Test placing multiple buildings in a row.
    /// Run with: cargo test buildings_v2::test::tests::place_building_row -- --nocapture
    #[tokio::test]
    async fn place_building_row() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let editor = world.get_editor();

        let midpoint = editor.world().world_rect_2d().size / 2;
        let ground_y = editor.world().add_height(midpoint).y;

        info!("Placing row of buildings at ground level: {}", ground_y);

        let materials = Material::load().expect("Failed to load materials");
        let data = crate::generator::data::LoadedData::load().expect("Failed to load data");
        let palette = data
            .palettes
            .get(&"desert_prismarine".into())
            .expect("Palette not found")
            .clone();

        let mut rng = RNG::new(999);

        // Place 3 buildings in a row with varying sizes
        // (width, depth, height, floors, use_double_door)
        let buildings = [
            (6, 5, 4, 1, false),   // small 1-story - single door
            (8, 6, 5, 2, false),   // medium 2-story - single door
            (10, 8, 4, 1, true),   // large 1-story - double door
        ];

        let single_door_rules = DoorRules::default();
        let double_door_rules = DoorRules {
            default_type: crate::generator::buildings_v2::DoorType::Double,
            ..DoorRules::default()
        };

        // Window rules for different building sizes
        let small_window_rules = WindowRules {
            density: 0.3,
            prefer_symmetry: true,
            consistent_type: true,
            default_type: crate::generator::buildings_v2::WindowType::Small,
        };

        let large_window_rules = WindowRules {
            density: 0.4,
            prefer_symmetry: true,
            consistent_type: true,
            default_type: crate::generator::buildings_v2::WindowType::Wide,
        };

        let mut x_offset = midpoint.x - 20;
        for (i, (width, depth, height, floors, use_double_door)) in buildings.iter().enumerate() {
            let mut frame = Frame::rectangle(
                Point3D::new(x_offset, ground_y, midpoint.y - depth / 2),
                *width,
                *depth,
                *height,
                *floors,
            );

            // Add doors to the frame
            let door_rules = if *use_double_door { &double_door_rules } else { &single_door_rules };
            add_doors_to_frame(&mut frame, door_rules, &mut rng);

            // Add windows to the frame
            let window_rules = if *width >= 10 { &large_window_rules } else { &small_window_rules };
            add_windows_to_frame(&mut frame, window_rules, &mut rng);

            place_frame(&frame, &editor, &palette, &materials, &mut rng).await;

            // Generate and place roof
            // First building gets gable, second gets hip, third uses auto-select
            let roof = if i == 0 {
                // Force gable roof for first building
                let mut rules = RoofRules::default();
                rules.prefer_hip = false;
                generate_roof(&frame, &rules)
            } else if i == 1 {
                // Force hip roof for second building
                let rules = RoofRules::default();
                generate_roof(&frame, &rules)
            } else {
                // Auto-select for third building
                let rules = RoofRules::default();
                generate_roof(&frame, &rules)
            };

            place_roof(&roof, &frame.footprint, &editor, &palette, &materials, &mut rng).await;

            x_offset += width + 3; // gap between buildings
        }

        info!("Building row placed successfully");
        editor.flush_buffer().await;
    }
}
