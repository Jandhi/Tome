use crate::geometry::{Point2D, Rect2D};
use crate::noise::RNG;
use super::{Plot, Footprint, SizeClass, generate_footprint, maximal_rect::find_largest_rect, generate::{Layout, generate_layouts, select_layout, score_layout}};

/// Renders a plot and optional footprint as ASCII art.
/// '.' = usable, 'x' = unusable, '#' = footprint, '*' = candidate rect
fn render_ascii(plot: &Plot, candidate: Option<&Rect2D>, footprint: Option<&Footprint>) -> String {
    let min = plot.bounds.min();
    let w = plot.bounds.length();
    let h = plot.bounds.width();
    let mut lines = Vec::new();

    for z in 0..h {
        let mut row = String::new();
        for x in 0..w {
            let world_point = Point2D::new(min.x + x, min.y + z);
            let in_footprint = footprint.map_or(false, |f| f.contains(world_point));
            let in_candidate = candidate.map_or(false, |c| c.contains(world_point));
            let usable = plot.usable[x as usize][z as usize];

            if in_footprint {
                row.push('#');
            } else if in_candidate {
                row.push('*');
            } else if usable {
                row.push('.');
            } else {
                row.push('x');
            }
        }
        lines.push(row);
    }

    lines.join("\n")
}

#[test]
fn maximal_rect_fully_usable() {
    let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(14, 14));
    let plot = Plot::fully_usable(bounds);
    let rect = find_largest_rect(&plot.usable).unwrap();

    println!("Candidate rect: {:?}, area: {}", rect, rect.area());
    println!("{}", render_ascii(&plot, Some(&rect), None));

    assert_eq!(rect.area(), 225); // 15x15
}

#[test]
fn maximal_rect_with_obstacles() {
    let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(19, 19));
    let mut plot = Plot::fully_usable(bounds);

    // Block out a lake in one corner
    for x in 12..18 {
        for z in 0..6 {
            plot.usable[x][z] = false;
        }
    }
    // Block out some trees
    for x in 0..3 {
        for z in 15..20 {
            plot.usable[x][z] = false;
        }
    }

    let rect = find_largest_rect(&plot.usable).unwrap();

    println!("Candidate rect: {:?}, area: {}", rect, rect.area());
    println!("{}", render_ascii(&plot, Some(&rect), None));

    // The largest rect should avoid both obstacles
    assert!(rect.area() > 100);
    // Verify it's within usable area
    for point in rect.iter() {
        assert!(plot.usable[point.x as usize][point.y as usize],
            "Candidate rect contains unusable cell at {:?}", point);
    }
}

#[test]
fn maximal_rect_narrow_plot() {
    // A very narrow L-shaped usable area
    let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(19, 9));
    let mut plot = Plot::fully_usable(bounds);

    // Block the right half of the top rows
    for x in 10..20 {
        for z in 0..5 {
            plot.usable[x][z] = false;
        }
    }

    let rect = find_largest_rect(&plot.usable).unwrap();

    println!("Candidate rect: {:?}, area: {}", rect, rect.area());
    println!("{}", render_ascii(&plot, Some(&rect), None));

    // Should find either the bottom strip (20x5) or left strip (10x10)
    assert!(rect.area() >= 100);
}

#[test]
fn footprint_from_single_rect() {
    let core = Rect2D::from_points(Point2D::new(2, 2), Point2D::new(10, 8));
    let vertices = vec![
        Point2D::new(2, 2),
        Point2D::new(10, 2),
        Point2D::new(10, 8),
        Point2D::new(2, 8),
    ];
    let footprint = Footprint::new(vertices, vec![core]);

    let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(14, 14));
    let plot = Plot::fully_usable(bounds);

    println!("Single rect footprint:");
    println!("{}", render_ascii(&plot, None, Some(&footprint)));

    let filled = footprint.filled_points();
    assert_eq!(filled.len(), core.area() as usize);
}

#[test]
fn footprint_l_shape() {
    // Core rect
    let core = Rect2D::from_points(Point2D::new(2, 2), Point2D::new(10, 8));
    // Wing attached to the right side, bottom-flush
    let wing = Rect2D::from_points(Point2D::new(11, 5), Point2D::new(14, 8));

    // L-shape polygon (clockwise)
    let vertices = vec![
        Point2D::new(2, 2),
        Point2D::new(10, 2),
        Point2D::new(10, 5),
        Point2D::new(14, 5),
        Point2D::new(14, 8),
        Point2D::new(2, 8),
    ];
    let footprint = Footprint::new(vertices, vec![core, wing]);

    let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(17, 11));
    let plot = Plot::fully_usable(bounds);

    println!("L-shape footprint:");
    println!("{}", render_ascii(&plot, None, Some(&footprint)));

    let filled = footprint.filled_points();
    let expected = core.area() + wing.area();
    assert_eq!(filled.len(), expected as usize);
}

/// Renders a single layout into a fixed-size grid.
fn render_layout_small(layout: &Layout, width: i32, height: i32) -> Vec<String> {
    let mut lines = Vec::new();
    for z in 0..height {
        let mut row = String::new();
        for x in 0..width {
            let point = Point2D::new(x, z);
            let in_core = layout.core.contains(point);
            let wing_idx = layout.wings.iter().position(|w| w.contains(point));

            if in_core {
                row.push('#');
            } else if let Some(idx) = wing_idx {
                row.push((b'1' + idx as u8) as char);
            } else {
                row.push('.');
            }
        }
        lines.push(row);
    }
    lines
}

/// Renders multiple layouts side by side in a grid arrangement.
/// If scores is provided, shows the score in the header for each layout.
fn render_gallery(layouts: &[Layout], scores: Option<&[f32]>, cols: usize, cell_w: i32, cell_h: i32) -> String {
    let rows = (layouts.len() + cols - 1) / cols;
    let mut output = String::new();

    for row in 0..rows {
        // Render each layout in this row
        let row_layouts: Vec<Vec<String>> = (0..cols)
            .map(|col| {
                let idx = row * cols + col;
                if idx < layouts.len() {
                    render_layout_small(&layouts[idx], cell_w, cell_h)
                } else {
                    vec![" ".repeat(cell_w as usize); cell_h as usize]
                }
            })
            .collect();

        // Header line
        for col in 0..cols {
            let idx = row * cols + col;
            if idx < layouts.len() {
                let l = &layouts[idx];
                let label = if let Some(s) = scores {
                    format!("{}x{} +{}w a={} s={:.2}",
                        l.core.length(), l.core.width(),
                        l.wings.len(), l.total_area(), s[idx])
                } else {
                    format!("{}: {}x{} +{}w a={}",
                        idx, l.core.length(), l.core.width(),
                        l.wings.len(), l.total_area())
                };
                output += &format!("{:<width$} ", label, width = cell_w as usize);
            }
        }
        output += "\n";

        // Grid lines
        for line_idx in 0..cell_h as usize {
            for col in 0..cols {
                output += &row_layouts[col][line_idx];
                output += " ";
            }
            output += "\n";
        }
        output += "\n";
    }

    output
}

#[test]
fn layout_gallery() {
    let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(19, 19));
    let plot = Plot::fully_usable(bounds);

    for (name, class) in [
        ("COTTAGE", SizeClass::Cottage),
        ("HOUSE", SizeClass::House),
        ("HALL", SizeClass::Hall),
        ("MANOR", SizeClass::Manor),
    ] {
        let mut rng = RNG::new(42);
        if let Some(result) = generate_layouts(&mut rng, &plot, &class, 4, 4, 0) {
            println!("\n========== {} ==========", name);
            println!("{}", render_gallery(&result.layouts, None, 4, 20, 20));
        }
    }
}

#[test]
fn layout_gallery_varied_seeds() {
    let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(19, 19));
    let plot = Plot::fully_usable(bounds);

    for (name, class) in [
        ("HALL", SizeClass::Hall),
        ("MANOR", SizeClass::Manor),
    ] {
        println!("\n========== {} - varied seeds ==========", name);
        let mut all_layouts = Vec::new();
        for seed in [1, 17, 42, 77, 99, 123, 256, 512, 1000, 2024, 3333, 9999] {
            let mut rng = RNG::new(seed);
            if let Some(result) = generate_layouts(&mut rng, &plot, &class, 1, 1, 0) {
                if let Some(layout) = result.layouts.into_iter().next() {
                    all_layouts.push(layout);
                }
            }
        }
        println!("{}", render_gallery(&all_layouts, None, 4, 20, 20));
    }
}

#[test]
fn select_layout_gallery() {
    let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(19, 19));
    let plot = Plot::fully_usable(bounds);

    for (name, class) in [
        ("HOUSE", SizeClass::House),
        ("HALL", SizeClass::Hall),
        ("MANOR", SizeClass::Manor),
    ] {
        println!("\n========== {} - selected winners ==========", name);
        let mut winners = Vec::new();
        let mut scores = Vec::new();
        for seed in [1, 17, 42, 77, 99, 123, 256, 512, 1000, 2024, 3333, 9999] {
            let mut rng = RNG::new(seed);
            if let Some(result) = generate_layouts(&mut rng, &plot, &class, 4, 4, 0) {
                let mut select_rng = rng.derive();
                if let Some(winner) = select_layout(&mut select_rng, &result.layouts, result.target_area, &result.candidate, class.min_side() * class.min_side()) {
                    let score = score_layout(&winner, result.target_area, &result.candidate);
                    winners.push(winner);
                    scores.push(score);
                }
            }
        }
        println!("{}", render_gallery(&winners, Some(&scores), 4, 20, 20));
    }
}

#[cfg(test)]
mod minecraft_tests {
    use super::*;
    use crate::{editor::World, http_mod::GDMCHTTPProvider, util::init_logger, noise::RNG};

    use crate::minecraft::Block;

    /// Generate footprints in a plot, marking each as unusable for the next.
    fn fill_plot(rng: &mut RNG, plot: &mut Plot, size_class: &SizeClass, max: usize) -> Vec<Footprint> {
        let mut footprints = Vec::new();
        let plot_min = plot.bounds.min();
        for _ in 0..max {
            let footprint = match generate_footprint(rng, plot, size_class) {
                Some(f) => f,
                None => break,
            };
            for point in footprint.filled_points() {
                for dx in -1..=1 {
                    for dz in -1..=1 {
                        let p = Point2D::new(point.x + dx, point.y + dz);
                        let lx = (p.x - plot_min.x) as usize;
                        let lz = (p.y - plot_min.y) as usize;
                        if lx < plot.usable.len() && lz < plot.usable[0].len() {
                            plot.usable[lx][lz] = false;
                        }
                    }
                }
            }
            footprints.push(footprint);
        }
        footprints
    }

    fn sign_block(text: &str) -> Block {
        let data = format!(
            "{{front_text:{{messages:['\"{}\"','\"\"','\"\"','\"\"']}}}}",
            text
        );
        Block::new("oak_sign".into(), None, Some(data))
    }

    #[tokio::test]
    async fn visualize_footprint() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let editor = world.get_editor();

        let world_rect = editor.world().world_rect_2d();
        let center = world_rect.midpoint();

        let colors = [
            "lime_wool", "orange_wool", "light_blue_wool", "magenta_wool", "yellow_wool",
            "cyan_wool", "pink_wool", "blue_wool", "purple_wool", "red_wool",
        ];

        // 2x2 grid of 64x64 areas: NW=Hut, NE=House, SW=Townhouse, SE=Manor
        let quadrants: [(i32, i32, &str, SizeClass); 4] = [
            (-64, -64, "Cottages",  SizeClass::Cottage),
            (  0, -64, "Houses",   SizeClass::House),
            (-64,   0, "Halls",    SizeClass::Hall),
            (  0,   0, "Manors",   SizeClass::Manor),
        ];

        let mut rng = RNG::new(777);

        for &(ox, oz, name, ref size_class) in &quadrants {
            let area_min = Point2D::new(center.x + ox, center.y + oz);
            let area_max = Point2D::new(center.x + ox + 63, center.y + oz + 63);
            let bounds = Rect2D::from_points(area_min, area_max);
            let mut plot = Plot::fully_usable(bounds);

            let footprints = fill_plot(&mut rng, &mut plot, size_class, 40);
            let winged = footprints.iter().filter(|f| f.rects().len() > 1).count();
            let areas: Vec<usize> = footprints.iter().map(|f| f.filled_points().len()).collect();
            let avg_area = if areas.is_empty() { 0 } else { areas.iter().sum::<usize>() / areas.len() };
            println!("{}: placed {} ({} with wings), avg area {}, range {}-{}",
                name, footprints.len(), winged, avg_area,
                areas.iter().min().unwrap_or(&0), areas.iter().max().unwrap_or(&0));

            // Build lookup sets
            let mut cell_owner: std::collections::HashMap<Point2D, usize> = std::collections::HashMap::new();
            let mut edge_set: std::collections::HashSet<Point2D> = std::collections::HashSet::new();

            for (idx, footprint) in footprints.iter().enumerate() {
                let filled: std::collections::HashSet<Point2D> = footprint.filled_points().into_iter().collect();
                for &p in &filled {
                    cell_owner.insert(p, idx);
                }
                for (a, b) in footprint.edges() {
                    if a.x == b.x {
                        let (z0, z1) = if a.y < b.y { (a.y, b.y) } else { (b.y, a.y) };
                        for z in z0..z1 {
                            let left = Point2D::new(a.x - 1, z);
                            let right = Point2D::new(a.x, z);
                            if filled.contains(&left) { edge_set.insert(left); }
                            if filled.contains(&right) { edge_set.insert(right); }
                        }
                    } else {
                        let (x0, x1) = if a.x < b.x { (a.x, b.x) } else { (b.x, a.x) };
                        for x in x0..x1 {
                            let top = Point2D::new(x, a.y - 1);
                            let bot = Point2D::new(x, a.y);
                            if filled.contains(&top) { edge_set.insert(top); }
                            if filled.contains(&bot) { edge_set.insert(bot); }
                        }
                    }
                }
            }

            // Place blocks
            for x in 0..64i32 {
                for z in 0..64i32 {
                    let world_point = Point2D::new(area_min.x + x, area_min.y + z);
                    let y = editor.world().get_height_at(world_point).expect("test cell in bounds");

                    let block: Block = if edge_set.contains(&world_point) {
                        "stone_bricks".into()
                    } else if let Some(&idx) = cell_owner.get(&world_point) {
                        colors[idx % colors.len()].into()
                    } else {
                        "white_wool".into()
                    };

                    editor.place_block(&block, world_point.add_y(y)).await;
                }
            }

            // Place sign at the inner corner (toward center of the 2x2 grid)
            let sign_x = if ox < 0 { area_max.x } else { area_min.x };
            let sign_z = if oz < 0 { area_max.y } else { area_min.y };
            let sign_point = Point2D::new(sign_x, sign_z);
            let sign_y = editor.world().get_height_at(sign_point).expect("test cell in bounds");
            editor.place_block(&sign_block(name), sign_point.add_y(sign_y + 1)).await;
        }

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn visualize_maximal_rect() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let editor = world.get_editor();

        let world_rect = editor.world().world_rect_2d();
        let center = world_rect.midpoint();
        let y = editor.world().get_height_at(center).expect("test cell in bounds");

        // Create a 30x30 plot with some obstacles
        let plot_min = Point2D::new(center.x - 15, center.y - 15);
        let plot_max = Point2D::new(center.x + 14, center.y + 14);
        let bounds = Rect2D::from_points(plot_min, plot_max);
        let mut plot = Plot::fully_usable(bounds);

        // Add some obstacles
        let mut rng = RNG::new(42);
        for _ in 0..15 {
            let ox = rng.rand_i32(25) as usize;
            let oz = rng.rand_i32(25) as usize;
            for dx in 0..rng.rand_i32(5) as usize + 1 {
                for dz in 0..rng.rand_i32(5) as usize + 1 {
                    if ox + dx < 30 && oz + dz < 30 {
                        plot.usable[ox + dx][oz + dz] = false;
                    }
                }
            }
        }

        let rect = find_largest_rect(&plot.usable);

        // Place blocks in Minecraft
        for x in 0..30i32 {
            for z in 0..30i32 {
                let world_point = Point2D::new(plot_min.x + x, plot_min.y + z);
                let usable = plot.usable[x as usize][z as usize];
                let in_rect = rect.as_ref().map_or(false, |r| {
                    r.contains(Point2D::new(x, z))
                });

                let block = if in_rect {
                    "lime_wool"
                } else if usable {
                    "white_wool"
                } else {
                    "red_wool"
                };

                editor.place_block(&block.into(), world_point.add_y(y)).await;
            }
        }

        // Print ASCII too
        println!("{}", render_ascii(&plot, rect.as_ref(), None));

        editor.flush_buffer().await;
    }
}
