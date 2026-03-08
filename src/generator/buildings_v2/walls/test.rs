use crate::geometry::{Cardinal, Point2D, Point3D, Rect2D};
use crate::editor::Editor;
use crate::generator::buildings_v2::floors::place_floors;
use crate::generator::buildings_v2::footprint::{Footprint, Plot, SizeClass, generate_footprint};
use crate::generator::buildings_v2::footprint::merge::outline_from_rects;
use crate::generator::buildings_v2::frame::{Frame, generate_frame};
use crate::generator::buildings_v2::foundation::place_foundation;
use crate::generator::buildings_v2::roof::gable::GablePitch;
use crate::generator::buildings_v2::roof::place_roof;
use crate::generator::data::LoadedData;
use crate::generator::materials::PaletteId;
use crate::editor::World;
use crate::http_mod::GDMCHTTPProvider;
use crate::minecraft::Block;
use crate::noise::RNG;
use crate::util::init_logger;
use super::{build_segments, place_doors, place_windows, place_frame, place_wall_infill, place_openings, segment_cells, WallSegment, OpeningKind, DoorStyle};

fn make_frame(rects: Vec<Rect2D>, floor_counts: Vec<u32>) -> Frame {
    let vertices = outline_from_rects(&rects);
    let footprint = Footprint::new(vertices, rects);
    Frame::new(footprint, 64, floor_counts, 3)
}

#[test]
fn single_rect_segments() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(4, 3));
    let frame = make_frame(vec![rect], vec![1]);
    let wall_segs = build_segments(&frame);

    // 1 floor, 4 edges = 4 segments
    assert_eq!(wall_segs.segments.len(), 4);

    // All segments should be floor 0
    assert!(wall_segs.segments.iter().all(|s| s.floor == 0));

    // Simple rect has no concave corners, so no extra cells.
    // Rect is 5x4, perimeter on corner grid = 2*(5+4) = 18
    let total_len: i32 = wall_segs.segments.iter().map(|s| s.length).sum();
    assert_eq!(total_len, 18);

    // Each segment should have a valid facing
    for seg in &wall_segs.segments {
        assert!(matches!(
            seg.facing,
            Cardinal::North | Cardinal::East | Cardinal::South | Cardinal::West
        ));
    }
}

#[test]
fn single_rect_two_floors() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(4, 4));
    let frame = make_frame(vec![rect], vec![2]);
    let wall_segs = build_segments(&frame);

    // 2 floors, 4 edges each = 8 segments
    assert_eq!(wall_segs.segments.len(), 8);

    let floor0: Vec<_> = wall_segs.segments_on_floor(0).collect();
    let floor1: Vec<_> = wall_segs.segments_on_floor(1).collect();
    assert_eq!(floor0.len(), 4);
    assert_eq!(floor1.len(), 4);

    // Floor Y positions
    assert!(floor0.iter().all(|s| s.base_y == 64));
    assert!(floor1.iter().all(|s| s.base_y == 68)); // 64 + 1*(4+1)
}

#[test]
fn l_shape_segments() {
    let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 4));
    let wing = Rect2D::from_points(Point2D::new(7, 2), Point2D::new(9, 4));
    let frame = make_frame(vec![core, wing], vec![1, 1]);
    let wall_segs = build_segments(&frame);

    // L-shape has 6 vertices = 6 edges = 6 segments
    assert_eq!(wall_segs.segments.len(), 6);
}

#[test]
fn multi_rect_different_heights_segment_count() {
    // Core is 2 floors, wing is 1 floor
    let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 6));
    let wing = Rect2D::from_points(Point2D::new(7, 1), Point2D::new(9, 5));
    let frame = make_frame(vec![core, wing], vec![2, 1]);
    let wall_segs = build_segments(&frame);

    // Floor 0: combined outline has more segments than a simple rect
    let floor0: Vec<_> = wall_segs.segments_on_floor(0).collect();
    assert!(floor0.len() > 4, "Floor 0 should have more than 4 segments");

    // Floor 1: only core (4 edges)
    let floor1: Vec<_> = wall_segs.segments_on_floor(1).collect();
    assert_eq!(floor1.len(), 4, "Floor 1 should have 4 segments (core only)");

    // Floor 1 base_y should be 69
    assert!(floor1.iter().all(|s| s.base_y == 68));
}

#[test]
fn place_door_picks_nearest_plot_edge() {
    let rect = Rect2D::from_points(Point2D::new(5, 5), Point2D::new(12, 10));
    let frame = make_frame(vec![rect], vec![1]);
    let mut wall_segs = build_segments(&frame);
    let mut rng = RNG::new(1);

    // Plot south edge at z=4, very close to the south-facing wall (z=5 dual).
    // Plot extends far to the north so north-facing wall is further from any edge.
    let plot_bounds = Rect2D::from_points(Point2D::new(0, 4), Point2D::new(20, 25));
    let area = 8 * 6; // 48
    place_doors(&mut wall_segs, &plot_bounds, area, &mut rng);

    let doors: Vec<_> = wall_segs.doors().collect();
    assert_eq!(doors.len(), 1, "Small building should get 1 door");

    // Door should be on the south-facing segment (closest to plot south edge)
    let (seg, _opening) = &doors[0];
    assert_eq!(seg.facing, Cardinal::South, "Door should face the nearest plot edge (south)");
}

#[test]
fn place_door_large_building_gets_two() {
    let rect = Rect2D::from_points(Point2D::new(2, 2), Point2D::new(16, 12));
    let frame = make_frame(vec![rect], vec![1]);
    let mut wall_segs = build_segments(&frame);
    let mut rng = RNG::new(1);

    let plot_bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(20, 15));
    let area = 15 * 11; // 165, > 100
    place_doors(&mut wall_segs, &plot_bounds, area, &mut rng);

    let doors: Vec<_> = wall_segs.doors().collect();
    assert_eq!(doors.len(), 2, "Large building should get 2 doors");

    // Two doors should face different directions
    let (seg0, _) = &doors[0];
    let (seg1, _) = &doors[1];
    assert_ne!(seg0.facing, seg1.facing, "Doors should face different directions");
}

#[test]
fn place_door_double_style_for_huge_building() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(14, 14));
    let frame = make_frame(vec![rect], vec![1]);
    let mut wall_segs = build_segments(&frame);
    let mut rng = RNG::new(1);

    let plot_bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(20, 20));
    let area = 200; // > 150
    place_doors(&mut wall_segs, &plot_bounds, area, &mut rng);

    let doors: Vec<_> = wall_segs.doors().collect();
    assert!(!doors.is_empty());
    let (_, opening) = &doors[0];
    assert!(matches!(opening.kind, OpeningKind::Door(DoorStyle::Double)),
        "Huge building primary door should be Double");
}

#[test]
fn windows_placed_on_segments() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(9, 9));
    let frame = make_frame(vec![rect], vec![1]);
    let mut wall_segs = build_segments(&frame);
    let mut rng = RNG::new(1);

    place_windows(&mut wall_segs, &mut rng);

    let windows: Vec<_> = wall_segs.windows().collect();
    assert!(!windows.is_empty(), "Should place at least one window");

    // All windows should have y_offset = 1
    for (_, opening) in &windows {
        assert_eq!(opening.y_offset, 1);
    }
}

#[test]
fn windows_avoid_doors() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(9, 9));
    let frame = make_frame(vec![rect], vec![1]);
    let mut wall_segs = build_segments(&frame);
    let mut rng = RNG::new(1);

    let plot_bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(12, 12));
    place_doors(&mut wall_segs, &plot_bounds, 50, &mut rng);
    place_windows(&mut wall_segs, &mut rng);

    // Check no window overlaps a door on the same segment
    for seg in &wall_segs.segments {
        let doors: Vec<_> = seg.openings.iter()
            .filter(|o| matches!(o.kind, OpeningKind::Door(_)))
            .collect();
        let windows: Vec<_> = seg.openings.iter()
            .filter(|o| matches!(o.kind, OpeningKind::Window(_)))
            .collect();
        for door in &doors {
            for win in &windows {
                let door_end = door.offset + door.width;
                let win_end = win.offset + win.width;
                assert!(
                    win_end <= door.offset || win.offset >= door_end,
                    "Window at {} overlaps door at {}", win.offset, door.offset
                );
            }
        }
    }
}

#[test]
fn short_segment_gets_no_windows() {
    // Rect is only 3 blocks wide — segments are tiny
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(2, 2));
    let frame = make_frame(vec![rect], vec![1]);
    let mut wall_segs = build_segments(&frame);
    let mut rng = RNG::new(1);

    place_windows(&mut wall_segs, &mut rng);

    let windows: Vec<_> = wall_segs.windows().collect();
    assert!(windows.is_empty(), "Tiny segments should get no windows");
}

#[test]
fn upper_floors_get_more_windows() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(14, 14));
    let frame = make_frame(vec![rect], vec![2]);
    let mut wall_segs = build_segments(&frame);
    let mut rng = RNG::new(1);

    place_windows(&mut wall_segs, &mut rng);

    let floor0_windows = wall_segs.segments.iter()
        .filter(|s| s.floor == 0)
        .flat_map(|s| s.openings.iter())
        .filter(|o| matches!(o.kind, OpeningKind::Window(_)))
        .count();
    let floor1_windows = wall_segs.segments.iter()
        .filter(|s| s.floor == 1)
        .flat_map(|s| s.openings.iter())
        .filter(|o| matches!(o.kind, OpeningKind::Window(_)))
        .count();

    assert!(floor1_windows >= floor0_windows,
        "Upper floor ({}) should have >= windows than ground floor ({})",
        floor1_windows, floor0_windows);
}

#[test]
fn facing_directions_simple_rect() {
    // A simple rect should have one segment facing each direction
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(4, 4));
    let frame = make_frame(vec![rect], vec![1]);
    let wall_segs = build_segments(&frame);

    let facings: Vec<Cardinal> = wall_segs.segments.iter().map(|s| s.facing).collect();
    assert!(facings.contains(&Cardinal::North));
    assert!(facings.contains(&Cardinal::East));
    assert!(facings.contains(&Cardinal::South));
    assert!(facings.contains(&Cardinal::West));
}

/// Render a wall segment as ASCII art. Y=0 is the floor (bottom), rendered bottom-up.
/// '#' = wall, 'D' = door, 'W' = window, 'P' = corner post column
fn render_segment(seg: &super::WallSegment) -> String {
    let w = seg.length as usize;
    let h = seg.height as usize;
    // Grid[y][x], y=0 is floor level
    let mut grid = vec![vec!['#'; w]; h];

    // Mark corner post columns
    if w > 0 {
        for y in 0..h {
            grid[y][0] = 'P';
            grid[y][w - 1] = 'P';
        }
    }

    for opening in &seg.openings {
        let ch = match opening.kind {
            OpeningKind::Door(_) => 'D',
            OpeningKind::Window(_) => 'W',
        };
        for dy in 0..opening.height {
            for dx in 0..opening.width {
                let x = (opening.offset + dx) as usize;
                let y = (opening.y_offset + dy) as usize;
                if x < w && y < h {
                    grid[y][x] = ch;
                }
            }
        }
    }

    let mut out = String::new();
    // Render top-down so highest Y is first line
    for y in (0..h).rev() {
        let row: String = grid[y].iter().collect();
        out.push_str(&format!("  y{} |{}|\n", y, row));
    }
    out.push_str(&format!("     +{}+\n", "-".repeat(w)));
    out
}

#[test]
fn ascii_visualize_simple_house() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(9, 7));
    let frame = make_frame(vec![rect], vec![1]);
    let mut wall_segs = build_segments(&frame);
    let mut rng = RNG::new(42);

    let plot_bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(12, 12));
    let area = 10 * 8;
    place_doors(&mut wall_segs, &plot_bounds, area, &mut rng);
    place_windows(&mut wall_segs, &mut rng);

    println!("\n=== Simple 10x8 house, 1 floor ===");
    for seg in &wall_segs.segments {
        println!("{:?} wall (len={}, floor={}):", seg.facing, seg.length, seg.floor);
        print!("{}", render_segment(seg));
    }
}

#[test]
fn ascii_visualize_large_hall() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(14, 10));
    let frame = make_frame(vec![rect], vec![2]);
    let mut wall_segs = build_segments(&frame);
    let mut rng = RNG::new(42);

    let plot_bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(20, 20));
    let area = 15 * 11; // 165 > 150 so Double door, > 100 so 2 doors
    place_doors(&mut wall_segs, &plot_bounds, area, &mut rng);
    place_windows(&mut wall_segs, &mut rng);

    println!("\n=== Large 15x11 hall, 2 floors ===");
    for seg in &wall_segs.segments {
        println!("{:?} wall (len={}, floor={}):", seg.facing, seg.length, seg.floor);
        print!("{}", render_segment(seg));
    }
}

#[test]
fn ascii_visualize_l_shape() {
    let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 6));
    let wing = Rect2D::from_points(Point2D::new(9, 0), Point2D::new(12, 3));
    let frame = make_frame(vec![core, wing], vec![2, 1]);
    let mut wall_segs = build_segments(&frame);
    let mut rng = RNG::new(42);

    let plot_bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(16, 10));
    let area = 9 * 7 + 4 * 4; // 79
    place_doors(&mut wall_segs, &plot_bounds, area, &mut rng);
    place_windows(&mut wall_segs, &mut rng);

    println!("\n=== L-shape (9x7 core + 4x4 wing), core=2 floors, wing=1 ===");
    for seg in &wall_segs.segments {
        println!("{:?} wall (len={}, floor={}):", seg.facing, seg.length, seg.floor);
        print!("{}", render_segment(seg));
    }
}

/// Render a top-down ASCII grid showing wall segment cells.
/// Each segment gets a unique letter (A, B, C...). Cells claimed by multiple
/// segments show '+'. Empty cells show '.'.
fn render_overhead(wall_segs: &super::WallSegments, floor: u32) {
    use std::collections::HashMap;
    let mut cell_owners: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    let mut min_x = i32::MAX;
    let mut min_z = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_z = i32::MIN;

    let floor_segs: Vec<_> = wall_segs.segments.iter().enumerate()
        .filter(|(_, s)| s.floor == floor)
        .collect();

    for (idx, seg) in &floor_segs {
        let cells = segment_cells(seg);
        for cell in &cells {
            min_x = min_x.min(cell.x);
            min_z = min_z.min(cell.y);
            max_x = max_x.max(cell.x);
            max_z = max_z.max(cell.y);
            cell_owners.entry((cell.x, cell.y)).or_default().push(*idx);
        }
    }

    let labels: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars().collect();

    // Header: segment index → label + facing
    for (local_i, (idx, seg)) in floor_segs.iter().enumerate() {
        let label = labels[local_i % labels.len()];
        println!("  {} = Seg {} {:?} (len={})", label, idx, seg.facing, seg.length);
    }

    // Build index map: global seg index → local label
    let idx_to_label: HashMap<usize, char> = floor_segs.iter().enumerate()
        .map(|(local_i, (idx, _))| (*idx, labels[local_i % labels.len()]))
        .collect();

    println!("     {}", (min_x..=max_x).map(|x| format!("{}", (x % 10).abs())).collect::<String>());
    for z in min_z..=max_z {
        let mut row = String::new();
        for x in min_x..=max_x {
            let ch = match cell_owners.get(&(x, z)) {
                None => '.',
                Some(owners) if owners.len() == 1 => idx_to_label[&owners[0]],
                Some(_) => '+',
            };
            row.push(ch);
        }
        println!("  z{:<2} {}", z, row);
    }
}

#[test]
fn ascii_overhead_l_shape() {
    let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 6));
    let wing = Rect2D::from_points(Point2D::new(9, 0), Point2D::new(12, 3));
    let frame = make_frame(vec![core, wing], vec![2, 1]);
    let wall_segs = build_segments(&frame);

    println!("\n=== L-shape overhead, floor 0 ===");
    render_overhead(&wall_segs, 0);

    println!("\n=== L-shape overhead, floor 1 (core only) ===");
    render_overhead(&wall_segs, 1);
}

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


const COLORS: &[&str] = &[
    "white_concrete", "orange_concrete", "magenta_concrete", "light_blue_concrete",
    "yellow_concrete", "lime_concrete", "pink_concrete", "cyan_concrete",
    "purple_concrete", "blue_concrete", "brown_concrete", "green_concrete",
    "red_concrete", "black_concrete", "gray_concrete", "light_gray_concrete",
];

#[tokio::test]
async fn debug_single_manor_segments() {
    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let editor = world.get_editor();

    let world_rect = editor.world().world_rect_2d();
    let center = world_rect.midpoint();

    let plot_min = Point2D::new(center.x - 16, center.y - 16);
    let plot_max = Point2D::new(center.x + 15, center.y + 15);
    let bounds = Rect2D::from_points(plot_min, plot_max);
    let mut plot = Plot::fully_usable(bounds);

    let mut rng = RNG::new(42);
    let footprint = generate_footprint(&mut rng, &mut plot, &SizeClass::MANOR)
        .expect("Failed to generate manor footprint");

    let base_y = editor.world().get_ocean_floor_height_at(center);
    let frame_footprint = Footprint::new(
        outline_from_rects(footprint.rects()),
        footprint.rects().to_vec(),
    );
    let frame = generate_frame(frame_footprint, base_y, &SizeClass::MANOR, &mut rng);
    let wall_segs = build_segments(&frame);

    println!("\n=== Manor debug: {} segments across {} floors ===", wall_segs.segments.len(), frame.max_floors());

    for (i, seg) in wall_segs.segments.iter().enumerate() {
        let color = COLORS[i % COLORS.len()];
        let block: Block = color.into();
        let cells = segment_cells(seg);

        println!(
            "  Seg {}: {:?} facing={:?} len={} cells={} floor={} start=({},{}) end=({},{})",
            i, color, seg.facing, seg.length, cells.len(),
            seg.floor, seg.start.x, seg.start.y, seg.end.x, seg.end.y
        );

        for cell in &cells {
            for y in (seg.base_y - 1)..=(seg.base_y + seg.height as i32) {
                editor.place_block(&block, Point3D::new(cell.x, y, cell.y)).await;
            }
        }
    }

    editor.flush_buffer().await;
    println!("Done — manor with colored segments placed");
}

#[tokio::test]
async fn build_walls_in_world() {
    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette_id: PaletteId = "medieval_spruce".into();
    let palette = data.palettes.get(&palette_id).expect("Palette not found").clone();

    let world_rect = editor.world().world_rect_2d();
    let center = world_rect.midpoint();

    // Central 32x32 area
    let plot_min = Point2D::new(center.x - 16, center.y - 16);
    let plot_max = Point2D::new(center.x + 15, center.y + 15);
    let bounds = Rect2D::from_points(plot_min, plot_max);
    let mut plot = Plot::fully_usable(bounds);

    let mut rng = RNG::new(77);
    let footprints = fill_plot(&mut rng, &mut plot, &SizeClass::HALL, 20);
    println!("Placed {} house footprints in 32x32 area", footprints.len());

    for (i, footprint) in footprints.iter().enumerate() {
        let base_y = place_foundation(&mut editor, footprint, &data, &palette, &mut rng).await;

        let frame_footprint = Footprint::new(
            outline_from_rects(footprint.rects()),
            footprint.rects().to_vec(),
        );
        let frame = generate_frame(frame_footprint, base_y, &SizeClass::HALL, &mut rng);

        // Build segments and plan openings
        let mut wall_segs = build_segments(&frame);
        let footprint_area = footprint.filled_points().len() as i32;
        place_doors(&mut wall_segs, &bounds, footprint_area, &mut rng);
        place_windows(&mut wall_segs, &mut rng);

        // Upper floor slabs
        place_floors(&editor, &frame, &data, &palette, &mut rng).await;

        // Wall infill first (gets overwritten by frame at corners/beams)
        place_wall_infill(&editor, &wall_segs, &data, &palette, &mut rng).await;

        // Timber frame (posts + crossbeams override infill)
        place_frame(&editor, &frame, &data, &palette, &mut rng).await;

        // Opening blocks (doors/windows override infill)
        place_openings(&editor, &wall_segs, &data, &palette, &mut rng).await;

        // Roof
        place_roof(&editor, &frame, GablePitch::Double, &data, &palette, &mut rng).await;

        println!(
            "  Building {}: base_y={}, floors={}, doors={}, windows={}",
            i, base_y, frame.max_floors(),
            wall_segs.doors().count(),
            wall_segs.windows().count(),
        );
    }

    editor.flush_buffer().await;
    println!("Done — {} buildings placed", footprints.len());
}
