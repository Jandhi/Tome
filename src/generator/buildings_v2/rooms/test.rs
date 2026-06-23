use std::collections::HashMap;
use crate::geometry::{Point2D, Point3D, Rect2D};
use crate::noise::RNG;
use crate::minecraft::Block;
use crate::generator::buildings_v2::RoomType;
use crate::generator::buildings_v2::footprint::{Footprint, Plot, SizeClass, generate_footprint, generate_footprint_biased, find_boundaries};
use crate::generator::buildings_v2::footprint::merge::outline_from_rects;
use crate::generator::buildings_v2::frame::{Frame, generate_frame};
use super::{assign_types_to_rooms, RoomRole, RoomPlan, Room, ConstraintMap};
use super::assign::assign_roles;

// --- Unit tests for pure logic ---

#[test]
fn find_boundaries_l_shape() {
    // Core on the left, wing on the right
    let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 6));
    let wing = Rect2D::from_points(Point2D::new(9, 2), Point2D::new(13, 6));
    let rects = vec![core, wing];

    let boundaries = find_boundaries(&rects);
    assert_eq!(boundaries.len(), 1, "L-shape should have 1 boundary");

    let b = &boundaries[0];
    assert_eq!(b.rect_a, 0);
    assert_eq!(b.rect_b, 1);
    // Wall at x=8 (core's last column), z from 2 to 6
    assert_eq!(b.wall_cells.len(), 5);
    assert_eq!(b.wall_cells[0], Point2D::new(8, 2));
    assert_eq!(b.wall_cells[4], Point2D::new(8, 6));
}

#[test]
fn find_boundaries_t_shape() {
    // Core on bottom, wing on top center
    let core = Rect2D::from_points(Point2D::new(0, 4), Point2D::new(10, 8));
    let wing = Rect2D::from_points(Point2D::new(3, 0), Point2D::new(7, 3));
    let rects = vec![core, wing];

    let boundaries = find_boundaries(&rects);
    assert_eq!(boundaries.len(), 1);

    let b = &boundaries[0];
    // Wall at y=4 (core's first row), x from 3 to 7
    assert_eq!(b.wall_cells.len(), 5);
    assert_eq!(b.wall_cells[0], Point2D::new(3, 4));
    assert_eq!(b.wall_cells[4], Point2D::new(7, 4));
}

#[test]
fn find_boundaries_u_shape() {
    let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(10, 4));
    let wing_l = Rect2D::from_points(Point2D::new(0, 5), Point2D::new(3, 8));
    let wing_r = Rect2D::from_points(Point2D::new(7, 5), Point2D::new(10, 8));
    let rects = vec![core, wing_l, wing_r];

    let boundaries = find_boundaries(&rects);
    assert_eq!(boundaries.len(), 2, "U-shape should have 2 boundaries");

    // Both walls at y=4 (core's last row)
    for b in &boundaries {
        assert!(b.wall_cells.iter().all(|c| c.y == 4));
    }
}

#[test]
fn find_boundaries_no_adjacency() {
    // Two rects far apart
    let a = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(4, 4));
    let b = Rect2D::from_points(Point2D::new(10, 10), Point2D::new(14, 14));
    let rects = vec![a, b];

    let boundaries = find_boundaries(&rects);
    assert_eq!(boundaries.len(), 0);
}

#[test]
fn find_boundaries_single_rect() {
    let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 6));
    let rects = vec![core];

    let boundaries = find_boundaries(&rects);
    assert_eq!(boundaries.len(), 0);
}

#[test]
fn assign_roles_ground_floor_with_entry() {
    let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 6));
    let wing = Rect2D::from_points(Point2D::new(9, 2), Point2D::new(13, 6));
    let rects = vec![core, wing];

    let assignments = assign_roles(&rects, &[0, 1], 0, Some(1));
    assert_eq!(assignments.len(), 2);

    // Wing has the door → Entry
    let wing_role = assignments.iter().find(|(i, _)| *i == 1).unwrap().1;
    assert_eq!(wing_role, RoomRole::Entry);

    // Core is larger → Main
    let core_role = assignments.iter().find(|(i, _)| *i == 0).unwrap().1;
    assert_eq!(core_role, RoomRole::Main);
}

#[test]
fn assign_roles_ground_floor_no_door() {
    let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 6));
    let wing = Rect2D::from_points(Point2D::new(9, 2), Point2D::new(13, 6));
    let rects = vec![core, wing];

    let assignments = assign_roles(&rects, &[0, 1], 0, None);

    // Largest (core) becomes Entry when no door found
    let core_role = assignments.iter().find(|(i, _)| *i == 0).unwrap().1;
    assert_eq!(core_role, RoomRole::Entry);

    let wing_role = assignments.iter().find(|(i, _)| *i == 1).unwrap().1;
    assert_eq!(wing_role, RoomRole::Secondary);
}

#[test]
fn assign_roles_upper_floor() {
    let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 6));
    let wing = Rect2D::from_points(Point2D::new(9, 2), Point2D::new(13, 6));
    let rects = vec![core, wing];

    let assignments = assign_roles(&rects, &[0, 1], 1, Some(1));

    // All upper-floor rooms are Upper regardless of door
    for (_, role) in &assignments {
        assert_eq!(*role, RoomRole::Upper);
    }
}

#[test]
fn assign_roles_three_rects() {
    let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(10, 8));
    let wing_a = Rect2D::from_points(Point2D::new(11, 0), Point2D::new(15, 4));
    let wing_b = Rect2D::from_points(Point2D::new(0, 9), Point2D::new(4, 13));
    let rects = vec![core, wing_a, wing_b];

    let assignments = assign_roles(&rects, &[0, 1, 2], 0, Some(1));

    let find_role = |idx: usize| assignments.iter().find(|(i, _)| *i == idx).unwrap().1;

    assert_eq!(find_role(1), RoomRole::Entry);     // has door
    assert_eq!(find_role(0), RoomRole::Main);       // largest remaining
    assert_eq!(find_role(2), RoomRole::Secondary);  // the rest
}

/// Helper to make a Frame from hand-built rects and floor counts.
fn make_test_frame(rects: Vec<Rect2D>, floor_counts: Vec<u32>) -> Frame {
    let outline = outline_from_rects(&rects);
    let footprint = Footprint::new(outline, rects);
    Frame::new(footprint, 64, floor_counts, 3)
}

/// Build a minimal RoomPlan from a frame, assign types, and return (rect_idx, floor, RoomType) tuples.
fn test_assign_types(frame: &Frame, size_class: SizeClass, has_attic: bool, rng: &mut RNG) -> Vec<(usize, u32, RoomType)> {
    let rects = frame.footprint().rects();
    let mut rooms = Vec::new();

    for floor in frame.floors() {
        for &idx in frame.active_rects(floor) {
            let role = if floor > 0 { RoomRole::Upper } else { RoomRole::Secondary };
            let interior = super::compute_room_interior(rects, idx);
            rooms.push(Room {
                rect: rects[idx],
                rect_index: idx,
                floor,
                role,
                room_type: RoomType::Storage,
                interior,
                constraints: ConstraintMap::new(&interior),
                furniture: Vec::new(),
                floor_type: None,
            });
        }
    }
    if has_attic {
        for i in 0..rects.len() {
            let attic_floor = frame.floor_counts()[i];
            let interior = super::compute_room_interior(rects, i);
            rooms.push(Room {
                rect: rects[i],
                rect_index: i,
                floor: attic_floor,
                role: RoomRole::Attic,
                room_type: RoomType::Storage,
                interior,
                constraints: ConstraintMap::new(&interior),
                furniture: Vec::new(),
                floor_type: None,
            });
        }
    }

    let mut plan = RoomPlan { rooms, interior_doors: Vec::new() };
    assign_types_to_rooms(&mut plan, frame, size_class, rng);
    plan.rooms.iter().map(|r| (r.rect_index, r.floor, r.room_type)).collect()
}

#[test]
fn cottage_single_rect() {
    let mut rng = RNG::new(13);
    let frame = make_test_frame(
        vec![Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 6))],
        vec![1],
    );
    let rooms = test_assign_types(&frame, SizeClass::Cottage, false, &mut rng);
    assert_eq!(rooms.len(), 1);
    assert_eq!(rooms[0].2, RoomType::Common);
}

#[test]
fn cottage_with_wing() {
    let mut rng = RNG::new(13);
    let frame = make_test_frame(
        vec![
            Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 6)),
            Rect2D::from_points(Point2D::new(9, 2), Point2D::new(13, 6)),
        ],
        vec![1, 1],
    );
    let rooms = test_assign_types(&frame, SizeClass::Cottage, false, &mut rng);
    assert_eq!(rooms.len(), 2);
    assert_eq!(rooms[0].2, RoomType::Common);
    // Wing becomes Bedroom if budget target > 0, otherwise Study/Storage
    assert!(matches!(rooms[1].2, RoomType::Bedroom | RoomType::Storage | RoomType::Study));
}

#[test]
fn house_single_floor_no_wing() {
    let mut rng = RNG::new(13);
    let frame = make_test_frame(
        vec![Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 6))],
        vec![1],
    );
    let rooms = test_assign_types(&frame, SizeClass::House, false, &mut rng);
    assert_eq!(rooms.len(), 1);
    assert_eq!(rooms[0].2, RoomType::Common);
}

#[test]
fn house_single_floor_with_wing() {
    let mut rng = RNG::new(13);
    let frame = make_test_frame(
        vec![
            Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 6)),
            Rect2D::from_points(Point2D::new(9, 2), Point2D::new(13, 6)),
        ],
        vec![1, 1],
    );
    let rooms = test_assign_types(&frame, SizeClass::House, false, &mut rng);
    assert_eq!(rooms[0].2, RoomType::Hearth);
    assert_eq!(rooms[1].2, RoomType::Bedroom);
}

#[test]
fn house_two_floors() {
    let mut rng = RNG::new(13);
    let frame = make_test_frame(
        vec![
            Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 6)),
            Rect2D::from_points(Point2D::new(9, 2), Point2D::new(13, 6)),
        ],
        vec![2, 2],
    );
    let rooms = test_assign_types(&frame, SizeClass::House, false, &mut rng);
    // Floor 0: core=Hearth, wing=Storage
    assert_eq!(rooms[0], (0, 0, RoomType::Hearth));
    assert_eq!(rooms[1], (1, 0, RoomType::Storage));
    // Floor 1: all Bedroom
    assert_eq!(rooms[2], (0, 1, RoomType::Bedroom));
    assert_eq!(rooms[3], (1, 1, RoomType::Bedroom));
}

#[test]
fn house_no_grand_types() {
    for seed in 0..100 {
        let mut r = RNG::new(seed);
        let frame = make_test_frame(
            vec![
                Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 6)),
                Rect2D::from_points(Point2D::new(9, 2), Point2D::new(13, 6)),
            ],
            vec![2, 2],
        );
        let rooms = test_assign_types(&frame, SizeClass::House, false, &mut r);
        for (_, _, rt) in &rooms {
            assert!(!matches!(rt,
                RoomType::Dining | RoomType::Library | RoomType::Studio | RoomType::Armory
                | RoomType::GreatRoom | RoomType::Kitchen | RoomType::Pantry
                | RoomType::MultiBedroom | RoomType::MasterBedroom),
                "Grand type {:?} in House", rt);
        }
    }
}

#[test]
fn hall_ground_floor_by_size() {
    let mut rng = RNG::new(13);
    // Wing idx 1 is small (5x5=25), wing idx 2 is large (7x5=35)
    let frame = make_test_frame(
        vec![
            Rect2D::from_points(Point2D::new(0, 0), Point2D::new(10, 8)),   // core
            Rect2D::from_points(Point2D::new(11, 0), Point2D::new(15, 4)),  // small wing
            Rect2D::from_points(Point2D::new(0, 9), Point2D::new(6, 13)),   // large wing
        ],
        vec![2, 2, 2],
    );
    let rooms = test_assign_types(&frame, SizeClass::Hall, false, &mut rng);
    let ground: Vec<_> = rooms.iter().filter(|(_, f, _)| *f == 0).collect();

    assert_eq!(ground.len(), 3);
    // Core is always GreatRoom
    let core = ground.iter().find(|r| r.0 == 0).unwrap();
    assert_eq!(core.2, RoomType::GreatRoom);
    // Larger wing (idx 2) gets Kitchen
    let large = ground.iter().find(|r| r.0 == 2).unwrap();
    assert_eq!(large.2, RoomType::Kitchen);
    // Smaller wing (idx 1) gets Pantry
    let small = ground.iter().find(|r| r.0 == 1).unwrap();
    assert_eq!(small.2, RoomType::Pantry);
}

#[test]
fn hall_upper_floor_by_size() {
    for seed in 0..50 {
        let mut r = RNG::new(seed);
        // Wing idx 1 is small (5x5), wing idx 2 is large (7x5)
        let frame = make_test_frame(
            vec![
                Rect2D::from_points(Point2D::new(0, 0), Point2D::new(10, 8)),
                Rect2D::from_points(Point2D::new(11, 0), Point2D::new(15, 4)),  // small
                Rect2D::from_points(Point2D::new(0, 9), Point2D::new(6, 13)),   // large
            ],
            vec![2, 2, 2],
        );
        let rooms = test_assign_types(&frame, SizeClass::Hall, false, &mut r);
        let upper: Vec<_> = rooms.iter().filter(|(_, f, _)| *f == 1).collect();

        let core = upper.iter().find(|r| r.0 == 0).unwrap();
        // Core gets MultiBedroom if budget allows, otherwise Study
        assert!(matches!(core.2, RoomType::MultiBedroom | RoomType::Study));
        // Wings get bedroom types or non-bedroom fallbacks depending on budget
        let large = upper.iter().find(|r| r.0 == 2).unwrap();
        assert!(matches!(large.2, RoomType::MasterBedroom | RoomType::Study | RoomType::Storage));
        let small = upper.iter().find(|r| r.0 == 1).unwrap();
        assert!(matches!(small.2, RoomType::Bedroom | RoomType::Study | RoomType::Storage));
    }
}

#[test]
fn manor_first_upper_is_bedroom() {
    for seed in 0..50 {
        let mut r = RNG::new(seed);
        let frame = make_test_frame(
            vec![
                Rect2D::from_points(Point2D::new(0, 0), Point2D::new(12, 10)),
                Rect2D::from_points(Point2D::new(13, 0), Point2D::new(17, 6)),
                Rect2D::from_points(Point2D::new(0, 11), Point2D::new(6, 15)),
            ],
            vec![3, 2, 3],
        );
        let rooms = test_assign_types(&frame, SizeClass::Manor, false, &mut r);
        let first_upper = rooms.iter().find(|(_, f, _)| *f == 1).unwrap();
        assert_eq!(first_upper.2, RoomType::Bedroom, "First upper in Manor must be Bedroom");
    }
}

#[test]
fn hall_study_one_wing() {
    // Study should appear on at most one wing (but on every floor that wing is active)
    for seed in 0..50 {
        let mut r = RNG::new(seed);
        let frame = make_test_frame(
            vec![
                Rect2D::from_points(Point2D::new(0, 0), Point2D::new(10, 8)),
                Rect2D::from_points(Point2D::new(11, 0), Point2D::new(15, 4)),
                Rect2D::from_points(Point2D::new(0, 9), Point2D::new(4, 13)),
            ],
            vec![3, 3, 3],
        );
        let rooms = test_assign_types(&frame, SizeClass::Hall, false, &mut r);
        // Study should not appear in more wing rects than there are wings
        let study_wing_rects: std::collections::HashSet<usize> = rooms.iter()
            .filter(|(idx, _, rt)| *idx != 0 && *rt == RoomType::Study)
            .map(|(idx, _, _)| *idx)
            .collect();
        assert!(study_wing_rects.len() <= 2, "Study in too many wings: {:?} (seed={})", study_wing_rects, seed);
    }
}

#[test]
fn hall_no_manor_types() {
    for seed in 0..100 {
        let mut r = RNG::new(seed);
        let frame = make_test_frame(
            vec![
                Rect2D::from_points(Point2D::new(0, 0), Point2D::new(10, 8)),
                Rect2D::from_points(Point2D::new(11, 0), Point2D::new(15, 4)),
                Rect2D::from_points(Point2D::new(0, 9), Point2D::new(4, 13)),
            ],
            vec![3, 3, 3],
        );
        let rooms = test_assign_types(&frame, SizeClass::Hall, false, &mut r);
        for (_, _, rt) in &rooms {
            assert!(!matches!(rt, RoomType::Library | RoomType::Studio | RoomType::Armory),
                "Manor type {:?} in Hall", rt);
        }
    }
}

/// Generate buildings of all size classes and print ASCII floor plans with room type labels.
#[test]
fn room_type_ascii() {
    let mut rng = RNG::new(13);

    for (name, size_class, count) in [
        ("Cottage", SizeClass::Cottage, 3),
        ("House",   SizeClass::House,   3),
        ("Hall",    SizeClass::Hall,    3),
        ("Manor",   SizeClass::Manor,   3),
    ] {
        println!("\n########## {} ##########", name);
        let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(63, 63));
        let mut plot = Plot::fully_usable(bounds);

        for i in 0..count {
            let footprint = match generate_footprint(&mut rng, &plot, &size_class) {
                Some(f) => f,
                None => break,
            };

            let frame = generate_frame(footprint, 64, &size_class, &mut rng);
            let rects = frame.footprint().rects();

            let assignments = test_assign_types(&frame, size_class, false, &mut rng);

            println!("\n=== {} {} ===", name, i);
            println!("  Rects: {}, Floors: {} (counts: {:?})", rects.len(), frame.max_floors(), frame.floor_counts());

            let fp_bounds = frame.footprint().bounds();
            let min_x = fp_bounds.min().x;
            let min_z = fp_bounds.min().y;
            let max_x = fp_bounds.max().x;
            let max_z = fp_bounds.max().y;

            for floor in frame.floors() {
                println!("  Floor {}:", floor);

                let floor_rooms: Vec<_> = assignments.iter()
                    .filter(|(_, f, _)| *f == floor)
                    .collect();

                let w = (max_x - min_x + 1) as usize;
                let h = (max_z - min_z + 1) as usize;
                let mut grid = vec![vec![' '; w]; h];

                let active = frame.active_rects(floor);
                for &idx in active {
                    let rect = &rects[idx];
                    for x in rect.min().x..=rect.max().x {
                        for z in rect.min().y..=rect.max().y {
                            let gx = (x - min_x) as usize;
                            let gz = (z - min_z) as usize;
                            grid[gz][gx] = '.';
                        }
                    }
                }

                for (rect_idx, _, room_type) in &floor_rooms {
                    let rect = &rects[*rect_idx];
                    let cx = ((rect.min().x + rect.max().x) / 2 - min_x) as usize;
                    let cz = ((rect.min().y + rect.max().y) / 2 - min_z) as usize;
                    let label = room_type.label();
                    let start = cx.saturating_sub(label.len() / 2);
                    for (j, ch) in label.chars().enumerate() {
                        if start + j < w {
                            grid[cz][start + j] = ch;
                        }
                    }
                    println!("    rect[{}] {:?} ({}x{})",
                        rect_idx, room_type,
                        rect.length(), rect.width());
                }

                for row in &grid {
                    let line: String = row.iter().collect();
                    println!("    |{}|", line);
                }
            }

            // Mark footprint as used
            for point in frame.footprint().filled_points() {
                for dx in -2..=2 {
                    for dz in -2..=2 {
                        let lx = (point.x + dx) as usize;
                        let lz = (point.y + dz) as usize;
                        if lx < plot.usable.len() && lz < plot.usable[0].len() {
                            plot.usable[lx][lz] = false;
                        }
                    }
                }
            }
        }
    }
}

/// Make a sign block with text on the front face.
fn sign_block(line1: &str, line2: &str) -> Block {
    let nbt = format!(
        "{{front_text:{{messages:['\"{}\"','\"{}\"','\"\"','\"\"']}}}}",
        line1, line2
    );
    Block::new(
        "minecraft:oak_sign".into(),
        Some(HashMap::from([("rotation".to_string(), "0".to_string())])),
        Some(nbt),
    )
}

/// Generate halls in Minecraft with signs labeling each room.
#[tokio::test]
async fn build_halls_with_signs() {
    build_single_class_with_signs("Hall", SizeClass::Hall, 20, 13).await;
}

#[tokio::test]
async fn build_cottages_with_signs() {
    build_single_class_with_signs("Cottage", SizeClass::Cottage, 30, 13).await;
}

/// Shared helper for `build_halls/cottages/houses_with_signs` — builds N
/// buildings of a single size class and places a label sign at each room center.
async fn build_single_class_with_signs(label: &str, size_class: SizeClass, max: usize, seed: i64) {
    use crate::editor::World;
    use crate::http_mod::GDMCHTTPProvider;
    use crate::util::init_logger;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    
    use crate::generator::buildings_v2::{BuildCtx, build_house};

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette_id: PaletteId = "medieval_spruce".into();
    let palette = data.palettes.get(&palette_id).expect("Palette not found").clone();

    let world_rect = editor.world().world_rect_2d();
    let center = world_rect.midpoint();
    let bounds = Rect2D::from_points(
        Point2D::new(center.x - 64, center.y - 64),
        Point2D::new(center.x + 63, center.y + 63),
    );

    let mut rng = RNG::new(seed);
    let mut plot = Plot::fully_usable(bounds);
    use crate::generator::buildings_v2::{Culture, BuildingContext};
    
    let culture = Culture::Medieval;
    let styles = culture.roof_styles();

    let footprints = fill_plot_multi(&mut rng, &mut plot, &[size_class], max, culture.square_bias());
    let n = footprints.len();

    let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
    for (i, (footprint, _)) in footprints.into_iter().enumerate() {
        let pitch = styles[i % styles.len()];
        let bctx = BuildingContext::new(culture, size_class, pitch);
        let house = build_house(&mut ctx, footprint, &bctx, bounds)
            .await
            .expect("build_house failed");

        for room in &house.room_plan.rooms {
            let cx = (room.rect.min().x + room.rect.max().x) / 2;
            let cz = (room.rect.min().y + room.rect.max().y) / 2;
            let y = house.frame.floor_y(room.floor) + 1;

            let line1 = format!("F{} R{}", room.floor, room.rect_index);
            let line2 = format!("{:?}", room.room_type);
            let sign = sign_block(&line1, &line2);
            ctx.editor.place_block_forced(&sign, Point3D::new(cx, y, cz)).await;
        }

        println!(
            "{} {}: rects={}, floors={}, pitch={:?}, rooms={}",
            label, i, house.footprint.rects().len(), house.frame.max_floors(),
            pitch, house.room_plan.rooms.len(),
        );
    }

    editor.flush_buffer().await;
    println!("Done — {} {}s placed with room signs", n, label.to_lowercase());
}

#[tokio::test]
async fn build_houses_with_signs() {
    build_single_class_with_signs("House", SizeClass::House, 30, 13).await;
}

#[tokio::test]
async fn build_manors_with_signs() {
    build_single_class_with_signs("Manor", SizeClass::Manor, 12, 13).await;
}

/// Builds exactly one Manor for debugging. Change `SEED` and re-run to inspect
/// different layouts in-game: `cargo test build_single_manor -- --nocapture`.
/// Asserts the manor got a cellar so a dud seed fails loudly.
/// Places a single Japanese house with the irimoya (hip-and-gable) roof on the
/// LIVE server at the build-area centre. Run against a running GDMC server:
/// `cargo test build_irimoya_house_live -- --nocapture`.
#[tokio::test]
async fn build_irimoya_house_live() {
    use crate::editor::World;
    use crate::http_mod::GDMCHTTPProvider;
    use crate::util::init_logger;
    use crate::generator::data::LoadedData;
    use crate::generator::buildings_v2::footprint::Footprint;
    use crate::generator::buildings_v2::roof::RoofStyle;
    use crate::generator::buildings_v2::{BuildCtx, build_house, BuildingContext, Culture};

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette = data.palettes.get(&Culture::Japanese.palette_id()).expect("palette").clone();

    let world_rect = editor.world().world_rect_2d();
    let center = world_rect.midpoint();

    // A clearly rectangular footprint (15×11) so the gable ridge runs along its
    // long axis; bounds give the foundation room to level terrain around it.
    let rect = Rect2D::from_points(
        Point2D::new(center.x - 7, center.y - 5),
        Point2D::new(center.x + 7, center.y + 5),
    );
    let bounds = Rect2D::from_points(
        Point2D::new(center.x - 20, center.y - 20),
        Point2D::new(center.x + 20, center.y + 20),
    );

    let mut rng = RNG::new(7);
    let footprint = Footprint::from_rect(rect);
    let bctx = BuildingContext::new(Culture::Japanese, SizeClass::House, RoofStyle::Irimoya);
    let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
    let house = build_house(&mut ctx, footprint, &bctx, bounds).await.expect("build_house failed");

    editor.flush_buffer().await;

    println!(
        "Irimoya house placed live at rect {:?}..{:?}, roof_y={}",
        rect.min(), rect.max(), house.frame.roof_y(0),
    );
}

/// Japanese floors are bamboo mosaic, not the frame wood. Guards both silent
/// failure modes: the `bamboo_mosaic` material file must actually load (a bad
/// JSON would be skipped at debug level), and the palette's `ground_floor` role
/// must resolve to it (an unknown id would leave the floor unplaced). `UpperFloor`
/// falls back to `GroundFloor`, so this covers every interior floor.
#[test]
fn japanese_floor_is_bamboo_mosaic() {
    use crate::generator::data::LoadedData;
    use crate::generator::materials::MaterialRole;
    use crate::generator::buildings_v2::Culture;
    use crate::minecraft::BlockForm;

    let data = LoadedData::load().expect("Failed to load data");
    let palette = data
        .palettes
        .get(&Culture::Japanese.palette_id())
        .expect("japanese palette");

    let floor_mat = palette
        .get_material(MaterialRole::GroundFloor)
        .expect("ground floor material");
    assert_eq!(floor_mat.as_str(), "bamboo_mosaic", "ground floor should be bamboo");
    assert_eq!(
        palette.get_material(MaterialRole::UpperFloor).unwrap().as_str(),
        "bamboo_mosaic",
        "upper floor should fall back to bamboo",
    );
    // The frame wood is untouched — bamboo is the floor, not the timber.
    assert_ne!(
        palette.get_material(MaterialRole::PrimaryWood).unwrap().as_str(),
        "bamboo_mosaic",
        "primary wood must stay the frame timber",
    );

    // The material loaded and its block form resolves to the real MC block.
    let material = data.materials.get(floor_mat).expect("bamboo_mosaic material loaded");
    let mut rng = RNG::new(0);
    let block = material.get_block(&BlockForm::Block, &mut rng).expect("block form");
    assert_eq!(block.as_str().trim_start_matches("minecraft:"), "bamboo_mosaic");
}

/// Live: fill the build area with Japanese irimoya houses of varied sizes
/// (Cottage / House / Hall / Manor). Run against a running GDMC server:
/// `cargo test build_japanese_houses_live -- --nocapture`.
#[tokio::test]
async fn build_japanese_houses_live() {
    use crate::editor::World;
    use crate::http_mod::GDMCHTTPProvider;
    use crate::util::init_logger;
    use crate::generator::data::LoadedData;
    use crate::generator::buildings_v2::roof::RoofStyle;
    use crate::generator::buildings_v2::{BuildCtx, build_house, BuildingContext, Culture};

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let culture = Culture::Japanese;
    let palette = data.palettes.get(&culture.palette_id()).expect("palette").clone();

    // Inset the build area a little so foundations have room to level.
    let wr = editor.world().world_rect_2d();
    let bounds = Rect2D::from_points(
        Point2D::new(wr.min().x + 4, wr.min().y + 4),
        Point2D::new(wr.max().x - 4, wr.max().y - 4),
    );

    let mut rng = RNG::new(21);
    let mut plot = Plot::fully_usable(bounds);
    let sizes = [SizeClass::Cottage, SizeClass::House, SizeClass::Hall, SizeClass::Manor];
    let footprints = fill_plot_multi(&mut rng, &mut plot, &sizes, 24, culture.square_bias());
    let n = footprints.len();

    let mut placed = 0;
    let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
    for (i, (footprint, size_class)) in footprints.into_iter().enumerate() {
        let bctx = BuildingContext::new(culture, size_class, RoofStyle::Irimoya);
        match build_house(&mut ctx, footprint, &bctx, bounds).await {
            Ok(house) => {
                placed += 1;
                println!(
                    "  {i}: {size_class:?} rects={} floors={}",
                    house.footprint.rects().len(), house.frame.max_floors(),
                );
            }
            Err(e) => println!("  {i}: {size_class:?} FAILED: {e}"),
        }
    }
    editor.flush_buffer().await;
    println!("Placed {placed}/{n} Japanese irimoya houses (live)");
}

#[tokio::test]
async fn build_single_manor() {
    use crate::editor::World;
    use crate::http_mod::GDMCHTTPProvider;
    use crate::util::init_logger;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::buildings_v2::{BuildCtx, build_house, BuildingContext, Culture};
    use crate::generator::buildings_v2::blueprint::{build_blueprint, render_ascii};

    const SEED: i64 = 27;

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette_id: PaletteId = "medieval_spruce".into();
    let palette = data.palettes.get(&palette_id).expect("Palette not found").clone();

    let world_rect = editor.world().world_rect_2d();
    let center = world_rect.midpoint();
    let bounds = Rect2D::from_points(
        Point2D::new(center.x - 64, center.y - 64),
        Point2D::new(center.x + 63, center.y + 63),
    );

    let mut rng = RNG::new(SEED);
    let mut plot = Plot::fully_usable(bounds);
    let culture = Culture::Medieval;
    let pitch = culture.roof_styles()[0];

    let footprints = fill_plot_multi(&mut rng, &mut plot, &[SizeClass::Manor], 1, culture.square_bias());
    let (footprint, _) = footprints.into_iter().next().expect("no footprint generated");

    let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
    let bctx = BuildingContext::new(culture, SizeClass::Manor, pitch);
    let house = build_house(&mut ctx, footprint, &bctx, bounds)
        .await
        .expect("build_house failed");

    for room in &house.room_plan.rooms {
        let cx = (room.rect.min().x + room.rect.max().x) / 2;
        let cz = (room.rect.min().y + room.rect.max().y) / 2;
        let y = house.frame.floor_y(room.floor) + 1;
        let sign = sign_block(&format!("F{} R{}", room.floor, room.rect_index), &format!("{:?}", room.room_type));
        ctx.editor.place_block_forced(&sign, Point3D::new(cx, y, cz)).await;
    }

    editor.flush_buffer().await;

    let blueprint = build_blueprint(&house.frame, &house.wall_segs, &house.floor_plan, &house.room_plan, house.has_attic);
    let ascii = render_ascii(&blueprint);
    std::fs::write("output/single_manor.txt", &ascii).expect("Failed to write blueprint ASCII");
    println!("{ascii}");
    println!(
        "Manor seed={SEED}: rects={}, floors={}, pitch={:?}, rooms={}, cellar={}",
        house.footprint.rects().len(), house.frame.max_floors(),
        pitch, house.room_plan.rooms.len(), house.has_cellar,
    );
    println!("Cellar stair cells: {:?}", house.cellar_stair);
    let f0_doors: Vec<_> = house.room_plan.interior_doors.iter()
        .filter(|(floor, ..)| *floor == 0)
        .map(|(_, a, b, c)| (*a, *b, c.x, c.y))
        .collect();
    println!("Floor-0 interior doors (rect_a, rect_b, x, z): {f0_doors:?}");
    assert!(house.has_cellar, "seed {SEED} manor has no cellar — terrain too wet or stair didn't fit; try another seed");
}

#[tokio::test]
async fn build_mixed_sizes_with_random_roofs() {
    use crate::editor::World;
    use crate::http_mod::GDMCHTTPProvider;
    use crate::util::init_logger;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    
    use crate::generator::buildings_v2::{BuildCtx, build_house};

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let base_palette_id: PaletteId = "medieval_spruce".into();
    let base_palette = data.palettes.get(&base_palette_id).expect("Base palette not found").clone();

    let roof_ids: Vec<PaletteId> = vec![
        "acacia_wood_roof".into(),
        "blackstone_roof".into(),
        "blue_wood_roof".into(),
        "brick_roof".into(),
        "oak_wood_roof".into(),
        "red_wood_roof".into(),
    ];

    let world_rect = editor.world().world_rect_2d();
    let center = world_rect.midpoint();
    let bounds = Rect2D::from_points(
        Point2D::new(center.x - 64, center.y - 64),
        Point2D::new(center.x + 63, center.y + 63),
    );

    let mut rng = RNG::new(13);
    let mut plot = Plot::fully_usable(bounds);
    use crate::generator::buildings_v2::{Culture, BuildingContext};
    let culture = Culture::Medieval;
    let styles = culture.roof_styles();

    let size_classes = [
        SizeClass::Cottage, SizeClass::Cottage, SizeClass::Cottage,
        SizeClass::House, SizeClass::House, SizeClass::House,
        SizeClass::Hall, SizeClass::Manor,
    ];

    let footprints_with_class = fill_plot_multi(&mut rng, &mut plot, &size_classes, 40, 0);
    let n = footprints_with_class.len();

    for (i, (footprint, size_class)) in footprints_with_class.into_iter().enumerate() {
        let roof_idx = rng.rand_i32_range(0, roof_ids.len() as i32) as usize;
        let roof_palette = data.palettes.get(&roof_ids[roof_idx]).expect("Roof palette not found");
        let palette = base_palette.clone().merged_with(roof_palette);

        let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
        let pitch = styles[i % styles.len()];
        let bctx = BuildingContext::new(culture, size_class, pitch);
        let house = build_house(&mut ctx, footprint, &bctx, bounds)
            .await
            .expect("build_house failed");

        for room in &house.room_plan.rooms {
            let cx = (room.rect.min().x + room.rect.max().x) / 2;
            let cz = (room.rect.min().y + room.rect.max().y) / 2;
            let y = house.frame.floor_y(room.floor) + 1;

            let line1 = format!("F{} R{}", room.floor, room.rect_index);
            let line2 = format!("{:?}", room.room_type);
            let sign = sign_block(&line1, &line2);
            editor.place_block_forced(&sign, Point3D::new(cx, y, cz)).await;
        }

        println!(
            "{:?} {}: rects={}, floors={}, pitch={:?}, roof={:?}, rooms={}",
            size_class, i, house.footprint.rects().len(), house.frame.max_floors(), pitch,
            roof_ids[roof_idx], house.room_plan.rooms.len(),
        );
    }

    editor.flush_buffer().await;
    println!("Done — {} buildings with random roof materials", n);
}

/// Run the buildings_v2 pipeline for a grid of 12 mixed-size buildings inside
/// `bounds` and write a blueprint SVG per building to `output/`. Shared by the
/// online (`build_furnished_houses`) and offline (`build_furnished_houses_offline`)
/// tests — the only difference between them is how the `Editor` was constructed.
async fn run_furnished_houses_pipeline(
    editor: &mut crate::editor::Editor,
    bounds: Rect2D,
    seed: i64,
    write_blueprints: bool,
    culture: crate::generator::buildings_v2::Culture,
) -> usize {
    run_furnished_houses_pipeline_jettied(editor, bounds, seed, write_blueprints, culture, false).await
}

/// Variant of the pipeline runner with an explicit `force_jetty` flag. Used by
/// the jetty property test to exercise upper-floor overhangs across many seeds
/// without disturbing the existing test's RNG stream.
async fn run_furnished_houses_pipeline_jettied(
    editor: &mut crate::editor::Editor,
    bounds: Rect2D,
    seed: i64,
    write_blueprints: bool,
    culture: crate::generator::buildings_v2::Culture,
    force_jetty: bool,
) -> usize {
    use crate::generator::data::LoadedData;
    use crate::generator::buildings_v2::blueprint::{build_blueprint, render_svg, render_ascii};
    use crate::generator::buildings_v2::{BuildCtx, BuildingContext, build_house};

    let data = LoadedData::load().expect("Failed to load data");
    let palette_id = culture.palette_id();
    let palette = data.palettes.get(&palette_id).expect("Palette not found").clone();
    let roof_styles = culture.roof_styles();

    let mut rng = RNG::new(seed);
    let mut plot = Plot::fully_usable(bounds);
    let size_classes = [SizeClass::Cottage, SizeClass::House, SizeClass::Hall];

    let footprints = fill_plot_multi(&mut rng, &mut plot, &size_classes, 12, culture.square_bias());
    let n = footprints.len();

    let mut ctx = BuildCtx::new(editor, &data, &palette, &mut rng);
    for (i, (footprint, size_class)) in footprints.into_iter().enumerate() {
        let roof_style = roof_styles[i % roof_styles.len()];
        let mut bctx = BuildingContext::new(culture, size_class, roof_style);
        bctx.jetty = force_jetty;
        let house = build_house(&mut ctx, footprint, &bctx, bounds)
            .await
            .unwrap_or_else(|msg| panic!("Seed {} building {} violated invariant: {}", seed, i, msg));

        if write_blueprints {
            let blueprint = build_blueprint(&house.frame, &house.wall_segs, &house.floor_plan, &house.room_plan, house.has_attic);
            let svg = render_svg(&blueprint);
            let ascii = render_ascii(&blueprint);
            let svg_path = format!("output/blueprint_{}.svg", i);
            let ascii_path = format!("output/blueprint_{}.txt", i);
            std::fs::create_dir_all("output").ok();
            std::fs::write(&svg_path, &svg).expect("Failed to write blueprint SVG");
            std::fs::write(&ascii_path, &ascii).expect("Failed to write blueprint ASCII");

            let win_count = house.wall_segs.windows().count();
            println!(
                "Building {}: {:?}, rects={}, floors={}, roof={:?}, rooms={}, timber={:?}, windows={}, blueprint={}",
                i, size_class, house.footprint.rects().len(), house.frame.max_floors(),
                roof_style, house.room_plan.rooms.len(), house.timber_pattern, win_count, svg_path,
            );
        }
    }

    editor.flush_buffer().await;
    n
}

/// Fill a plot with footprints of mixed size classes, marking each placed
/// footprint (plus 1-cell buffer) as unusable so the next doesn't overlap.
fn fill_plot_multi(
    rng: &mut RNG,
    plot: &mut Plot,
    size_classes: &[SizeClass],
    max: usize,
    square_bias: i32,
) -> Vec<(Footprint, SizeClass)> {
    let mut out = Vec::new();
    for i in 0..max {
        let size_class = size_classes[i % size_classes.len()];
        let fp = match generate_footprint_biased(rng, plot, &size_class, square_bias) {
            Some(f) => f,
            None => break,
        };
        let plot_min = plot.bounds.min();
        for point in fp.filled_points() {
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
        out.push((fp, size_class));
    }
    out
}

/// Full settlement pipeline using buildings_v2 instead of the original building system.
/// Requires a live Minecraft server with the GDMC HTTP mod.
#[tokio::test]
async fn build_furnished_houses() {
    use crate::editor::World;
    use crate::http_mod::GDMCHTTPProvider;
    use crate::util::init_logger;

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let world_rect = editor.world().world_rect_2d();
    let center = world_rect.midpoint();
    let bounds = Rect2D::from_points(
        Point2D::new(center.x - 64, center.y - 64),
        Point2D::new(center.x + 63, center.y + 63),
    );

    use crate::generator::buildings_v2::Culture;
    let count = run_furnished_houses_pipeline(&mut editor, bounds, 42, true, Culture::Medieval).await;
    println!("Done — {} furnished buildings placed", count);
}

/// Live twin of `build_furnished_houses` with jetty forced on: places the
/// seed-42 set into a live Minecraft world so the upper-floor overhangs can be
/// seen in-game. Eligible buildings (≥2 floors, plot room) jetty; cottages and
/// single-floor houses fall back to flush walls. Requires a live server.
#[tokio::test]
async fn build_furnished_jetty_houses() {
    use crate::editor::World;
    use crate::http_mod::GDMCHTTPProvider;
    use crate::util::init_logger;

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let world_rect = editor.world().world_rect_2d();
    let center = world_rect.midpoint();
    let bounds = Rect2D::from_points(
        Point2D::new(center.x - 64, center.y - 64),
        Point2D::new(center.x + 63, center.y + 63),
    );

    use crate::generator::buildings_v2::Culture;
    let count = run_furnished_houses_pipeline_jettied(
        &mut editor, bounds, 42, true, Culture::Medieval, true,
    ).await;
    println!("Done — {} furnished buildings (jetty forced) placed", count);
}

/// Online desert variant: places desert_sandstone houses in a live Minecraft
/// world. Requires a live Minecraft server with the GDMC HTTP mod.
#[tokio::test]
async fn build_furnished_desert_houses() {
    use crate::editor::World;
    use crate::http_mod::GDMCHTTPProvider;
    use crate::util::init_logger;
    

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let world_rect = editor.world().world_rect_2d();
    let center = world_rect.midpoint();
    let bounds = Rect2D::from_points(
        Point2D::new(center.x - 64, center.y - 64),
        Point2D::new(center.x + 63, center.y + 63),
    );

    use crate::generator::buildings_v2::Culture;
    let count = run_furnished_houses_pipeline(&mut editor, bounds, 42, true, Culture::Desert).await;
    println!("Done — {} furnished desert buildings placed", count);
}

/// Online Japanese variant: places the new hipped-roof houses in a live
/// Minecraft world so the upturned eave corners can be inspected. Requires a
/// live Minecraft server with the GDMC HTTP mod.
#[tokio::test]
async fn build_furnished_japanese_houses() {
    use crate::editor::World;
    use crate::http_mod::GDMCHTTPProvider;
    use crate::util::init_logger;

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let world_rect = editor.world().world_rect_2d();
    let center = world_rect.midpoint();
    let bounds = Rect2D::from_points(
        Point2D::new(center.x - 64, center.y - 64),
        Point2D::new(center.x + 63, center.y + 63),
    );

    use crate::generator::buildings_v2::Culture;
    let count = run_furnished_houses_pipeline(&mut editor, bounds, 42, true, Culture::Japanese).await;
    println!("Done — {} furnished japanese buildings placed", count);
}

/// Offline / dry-run variant: runs the same buildings_v2 pipeline against a
/// synthetic flat world, without any HTTP traffic. Produces the same blueprint
/// SVGs under `output/` as `build_furnished_houses` but does not require a
/// Minecraft server. Use this for iterating on generator logic locally.
#[tokio::test]
async fn build_furnished_houses_offline() {
    use crate::editor::World;
    use crate::geometry::Rect3D;
    use crate::util::init_logger;

    init_logger();

    // Synthetic build area: 256×256 with flat ground at y=64. The 128×128
    // building bounds sit in the middle so buildings near the edge have room
    // for roof overhangs without hitting the build-area boundary.
    let build_area = Rect3D::from_points(
        Point3D::new(0, 0, 0),
        Point3D::new(255, 127, 255),
    );
    let world = World::synthetic(build_area, 64);
    let mut editor = world.get_offline_editor();

    let bounds = Rect2D::from_points(
        Point2D::new(64, 64),
        Point2D::new(191, 191),
    );

    use crate::generator::buildings_v2::Culture;
    let count = run_furnished_houses_pipeline(&mut editor, bounds, 42, true, Culture::Medieval).await;
    println!("Done — {} furnished buildings placed (offline)", count);
}

/// Jetty variant of `build_furnished_houses_offline`: writes SVG/ASCII blueprints
/// for jettied houses so the overhangs can be eyeballed. Eligible buildings
/// (single-rect, ≥2 floors, with plot room) get upper-floor extents grown by 1
/// on each side; others silently fall back to flush walls.
#[tokio::test]
async fn build_furnished_jetty_houses_offline() {
    use crate::editor::World;
    use crate::geometry::Rect3D;
    use crate::util::init_logger;

    init_logger();

    let build_area = Rect3D::from_points(
        Point3D::new(0, 0, 0),
        Point3D::new(255, 127, 255),
    );
    let world = World::synthetic(build_area, 64);
    let mut editor = world.get_offline_editor();

    let bounds = Rect2D::from_points(
        Point2D::new(64, 64),
        Point2D::new(191, 191),
    );

    use crate::generator::buildings_v2::Culture;
    let count = run_furnished_houses_pipeline_jettied(
        &mut editor, bounds, 42, true, Culture::Medieval, true,
    ).await;
    println!("Done — {} furnished buildings (jetty forced) placed offline", count);
}

/// Offline desert variant: same as `build_furnished_houses_offline` but uses the
/// `desert_sandstone` palette — smooth_sandstone walls and cut_sandstone framing
/// instead of wool walls and spruce-log frames.
#[tokio::test]
async fn build_furnished_desert_houses_offline() {
    use crate::editor::World;
    use crate::geometry::Rect3D;
    use crate::util::init_logger;
    

    init_logger();

    let build_area = Rect3D::from_points(
        Point3D::new(0, 0, 0),
        Point3D::new(255, 127, 255),
    );
    let world = World::synthetic(build_area, 64);
    let mut editor = world.get_offline_editor();

    let bounds = Rect2D::from_points(
        Point2D::new(64, 64),
        Point2D::new(191, 191),
    );

    use crate::generator::buildings_v2::Culture;
    let count = run_furnished_houses_pipeline(&mut editor, bounds, 42, true, Culture::Desert).await;
    println!("Done — {} furnished desert buildings placed (offline)", count);
}

/// Offline Japanese variant: blackstone palette and the new hipped roof with
/// upturned eave corners. Use this to eyeball the new RoofStyle::Hipped output
/// via the ASCII/SVG blueprints written under `output/`.
#[tokio::test]
async fn build_furnished_japanese_houses_offline() {
    use crate::editor::World;
    use crate::geometry::Rect3D;
    use crate::util::init_logger;

    init_logger();

    let build_area = Rect3D::from_points(
        Point3D::new(0, 0, 0),
        Point3D::new(255, 127, 255),
    );
    let world = World::synthetic(build_area, 64);
    let mut editor = world.get_offline_editor();

    let bounds = Rect2D::from_points(
        Point2D::new(64, 64),
        Point2D::new(191, 191),
    );

    use crate::generator::buildings_v2::Culture;
    let count = run_furnished_houses_pipeline(&mut editor, bounds, 42, true, Culture::Japanese).await;
    println!("Done — {} furnished japanese buildings placed (offline)", count);
}

/// Irimoya heightmap geometry: a hipped skirt opened into a long-axis gable.
/// Pure geometry, no editor — asserts the three defining features:
/// (1) a long horizontal ridge across the central gable span, (2) hip-style
/// taper down to the eave in the short-end caps, and (3) the upturned corner curl.
#[test]
fn irimoya_heightmap_long_ridge_hip_caps_and_curl() {
    use crate::generator::buildings_v2::roof::irimoya::{
        IRIMOYA_RISE, LongAxis, gable_inset, irimoya_heightmap, pick_long_axis,
    };

    // 13×9 footprint (X long), so the ridge runs along X.
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(12, 8));
    assert_eq!(pick_long_axis(&rect), LongAxis::X);

    let inset = gable_inset(&rect, LongAxis::X);
    assert_eq!(inset, 3, "central half gabled: along_extent 12 / 4");

    let hm = irimoya_heightmap(&rect, &[], IRIMOYA_RISE, LongAxis::X, inset);

    // across_extent = 8 → cap_h = floor(8/2 * 1.0) = 4. The ridge sits at z=4.
    let ridge_z = 4;
    let cap_h = 4.0;
    for x in [3, 6, 9] {
        assert_eq!(
            hm.get(x, ridge_z), cap_h,
            "central span should be a flat ridge at cap_h, x={x}",
        );
    }

    // Short-end caps taper the ridge line down toward the eave (hip behaviour):
    // at x=1 (d_along=1 < inset) the surface is well below the ridge.
    assert!(
        hm.get(1, ridge_z) < cap_h,
        "end cap at x=1 should taper below the ridge, got {}", hm.get(1, ridge_z),
    );

    // Sorihafu curl: the diagonal corner overhang cell is lifted above its
    // cardinal eave neighbour.
    assert!(
        hm.get(-1, -1) > hm.get(0, -1),
        "corner overhang should be curled up above the cardinal eave: corner={}, cardinal={}",
        hm.get(-1, -1), hm.get(0, -1),
    );
}

/// Places a single Japanese house with the new `RoofStyle::Irimoya` through the
/// full offline pipeline, writes ASCII + SVG blueprints under `output/`, and
/// asserts the roof is present and a gable pediment wall closes the ridge end.
#[tokio::test]
async fn build_irimoya_house_offline() {
    use crate::editor::World;
    use crate::geometry::Rect3D;
    use crate::util::init_logger;
    use crate::generator::data::LoadedData;
    use crate::generator::buildings_v2::footprint::Footprint;
    use crate::generator::buildings_v2::roof::RoofStyle;
    use crate::generator::buildings_v2::roof::irimoya::{LongAxis, gable_inset};
    use crate::generator::buildings_v2::blueprint::{build_blueprint, render_ascii, render_svg};
    use crate::generator::buildings_v2::{BuildCtx, BuildingContext, Culture, build_house};

    init_logger();

    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
    let world = World::synthetic(build_area, 64);
    let mut editor = world.get_offline_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette = data.palettes.get(&Culture::Japanese.palette_id()).expect("palette").clone();

    // A clearly rectangular rect: 15 (X) × 11 (Z), so the gable ridge runs along X.
    let rect = Rect2D::from_points(Point2D::new(70, 70), Point2D::new(84, 80));
    let bounds = Rect2D::from_points(Point2D::new(56, 56), Point2D::new(98, 98));

    let mut rng = RNG::new(7);
    let footprint = Footprint::from_rect(rect);
    let bctx = BuildingContext::new(Culture::Japanese, SizeClass::House, RoofStyle::Irimoya);
    let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
    let house = build_house(&mut ctx, footprint, &bctx, bounds).await.expect("build_house failed");

    // Dump blueprints for eyeballing.
    let blueprint = build_blueprint(&house.frame, &house.wall_segs, &house.floor_plan, &house.room_plan, house.has_attic);
    std::fs::write("output/irimoya_house.txt", render_ascii(&blueprint)).expect("write ascii");
    std::fs::write("output/irimoya_house.svg", render_svg(&blueprint)).expect("write svg");

    let wall_top = house.frame.roof_y(0);
    let center = rect.midpoint();
    let at = |p: Point2D, y: i32| editor.try_get_block(Point3D::new(p.x, y, p.y));
    let is_air = |b: &Block| b.id == "air".into() || b.id == "minecraft:air".into();
    // Topmost solid block in a column, scanning the roof band.
    let top_solid = |p: Point2D| -> Option<i32> {
        (wall_top - 1..wall_top + 12)
            .filter(|&y| at(p, y).as_ref().map_or(false, |b| !is_air(b)))
            .max()
    };

    // The ridge perches well above the wall top at the centre (a tall gable,
    // not a shallow hip apex).
    let ridge_top = top_solid(center).expect("no roof over the centre");
    assert!(
        ridge_top >= wall_top + 3,
        "expected a tall central ridge, got top y={} (wall top {})", ridge_top, wall_top,
    );

    // The gable pediment: the inset plane that closes the ridge is a vertical
    // wall — a stack of solid blocks rising from the wall top at the
    // across-centre. A plain hip would be open air there.
    let inset = gable_inset(&rect, LongAxis::X);
    let pediment = Point2D::new(rect.min().x + inset, center.y);
    let pediment_count = (wall_top..wall_top + 12)
        .filter(|&y| at(pediment, y).as_ref().map_or(false, |b| !is_air(b)))
        .count();
    assert!(
        pediment_count >= 2,
        "expected a stacked gable pediment wall at {:?}, found {} solid blocks", pediment, pediment_count,
    );

    println!(
        "Irimoya house OK: wall_top={wall_top}, ridge_top={ridge_top}, inset={inset}, \
         pediment_blocks={pediment_count}, blueprints=output/irimoya_house.{{txt,svg}}",
    );
}

/// Engawa regression: a large Japanese building with `bctx.engawa = true` insets
/// its cellar/ground floor by 2 and its upper floors by 1 (so the upper storeys
/// overhang the ground floor), raises onto a platform, and lays a 2-deep wooden
/// deck with a pent-roof skirt over the outer ring. Builds a single-rect Manor
/// (always ≥2 floors → guaranteed overhang), dumps blueprints, and asserts the
/// differential inset, the 2-deep deck, and that the skirt only rings the outer
/// deck (the cells not tucked under the overhang).
#[tokio::test]
async fn build_engawa_house_offline() {
    use crate::editor::World;
    use crate::geometry::Rect3D;
    use crate::util::init_logger;
    use crate::generator::data::LoadedData;
    use crate::generator::buildings_v2::footprint::Footprint;
    use crate::generator::buildings_v2::roof::RoofStyle;
    use crate::generator::buildings_v2::roof::hipped::HippedPitch;
    use crate::generator::buildings_v2::blueprint::{build_blueprint, render_ascii, render_svg};
    use crate::generator::buildings_v2::{BuildCtx, BuildingContext, Culture, build_house};

    init_logger();

    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
    let world = World::synthetic(build_area, 64);
    let mut editor = world.get_offline_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette = data.palettes.get(&Culture::Japanese.palette_id()).expect("palette").clone();

    // 19 (X) × 15 (Z): ground insets to 15×11, upper to 17×13 — both well above
    // MIN_INSET_SIDE, and large enough that a Manor stays a single rect.
    let rect = Rect2D::from_points(Point2D::new(70, 70), Point2D::new(88, 84));
    let bounds = Rect2D::from_points(Point2D::new(52, 52), Point2D::new(106, 106));

    let mut rng = RNG::new(7);
    let footprint = Footprint::from_rect(rect);
    let mut bctx = BuildingContext::new(Culture::Japanese, SizeClass::Manor, RoofStyle::Hipped(HippedPitch::Stairs));
    bctx.engawa = true;
    let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
    let house = build_house(&mut ctx, footprint, &bctx, bounds).await.expect("build_house failed");

    let blueprint = build_blueprint(&house.frame, &house.wall_segs, &house.floor_plan, &house.room_plan, house.has_attic);
    std::fs::write("output/engawa_house.txt", render_ascii(&blueprint)).expect("write ascii");
    std::fs::write("output/engawa_house.svg", render_svg(&blueprint)).expect("write svg");

    // Ground floor inset by exactly 2 on every side.
    let ground = house.frame.rect_at(0, 0).expect("ground floor extent");
    assert_eq!(
        (ground.min().x, ground.min().y, ground.max().x, ground.max().y),
        (rect.min().x + 2, rect.min().y + 2, rect.max().x - 2, rect.max().y - 2),
        "ground floor should inset by 2 from {:?}, got {:?}", rect, ground,
    );

    // Tapered profile: the TOP floor also insets by 2 (roof pulled back in).
    let floors = house.frame.floor_counts()[0];
    assert!(floors > 1, "Manor must be multi-floor; got {} floors", floors);
    let top = house.frame.rect_at(0, floors - 1).expect("top floor extent");
    assert_eq!(
        (top.min().x, top.min().y, top.max().x, top.max().y),
        (rect.min().x + 2, rect.min().y + 2, rect.max().x - 2, rect.max().y - 2),
        "top floor should inset by 2 (matching the ground), got {:?}", top,
    );
    // Any MIDDLE floor insets by only 1 → it bulges out over the ground floor.
    if floors >= 3 {
        let mid = house.frame.rect_at(0, 1).expect("middle floor extent");
        assert_eq!(
            (mid.min().x, mid.min().y, mid.max().x, mid.max().y),
            (rect.min().x + 1, rect.min().y + 1, rect.max().x - 1, rect.max().y - 1),
            "middle floor should inset by 1 (bulging over the ground floor), got {:?}", mid,
        );
    }

    let at = |p: Point2D, y: i32| editor.try_get_block(Point3D::new(p.x, y, p.y));
    let is_air = |b: &Block| b.id == "air".into() || b.id == "minecraft:air".into();
    let is_stair = |b: &Block| b.id.as_str().contains("stairs");
    let is_slab = |b: &Block| b.id.as_str().contains("slab");
    let deck_y = house.frame.floor_y(0) - 1;
    let roof_y = house.frame.ceiling_y(0);
    let mid_x = (rect.min().x + rect.max().x) / 2;

    // The deck is 2 deep: both the nominal edge cell and the cell one in carry
    // wooden planks.
    let outer_deck = Point2D::new(mid_x, rect.min().y);       // nominal edge, outer ring
    let inner_deck = Point2D::new(mid_x, rect.min().y + 1);   // against the wall, inner ring
    for cell in [outer_deck, inner_deck] {
        assert!(
            at(cell, deck_y).as_ref().map_or(false, |b| !is_air(b)),
            "expected a 2-deep wooden deck plank at {:?} y={}, got {:?}", cell, deck_y, at(cell, deck_y),
        );
    }

    // Engawa roof: slabs on the inner ring (against the wall), stairs on the
    // outer ring (the dripping eave edge).
    assert!(
        at(inner_deck, roof_y).as_ref().map_or(false, is_slab),
        "expected an inner-ring roof slab at {:?} y={}, got {:?}", inner_deck, roof_y, at(inner_deck, roof_y),
    );
    assert!(
        at(outer_deck, roof_y).as_ref().map_or(false, is_stair),
        "expected an outer-ring roof stair at {:?} y={}, got {:?}", outer_deck, roof_y, at(outer_deck, roof_y),
    );

    // Diagonal corner: the outer corner cell (off the nominal corner) must be
    // roofed too — the eave wraps all the way around, not just the flat sides.
    let corner = Point2D::new(rect.min().x, rect.min().y);
    let corner_block = at(corner, roof_y);
    assert!(
        corner_block.as_ref().map_or(false, |b| is_stair(b) || is_slab(b)),
        "expected the diagonal corner to be roofed at {:?} y={}, got {:?}", corner, roof_y, corner_block,
    );

    // A fence-post support holds up the roof at the veranda corner: a wooden
    // fence column between the deck surface and the eave.
    let is_fence = |b: &Block| b.id.as_str().contains("fence") && !b.id.as_str().contains("gate");
    assert!(
        at(corner, deck_y + 1).as_ref().map_or(false, is_fence),
        "expected a fence-post support at the corner {:?} y={}, got {:?}", corner, deck_y + 1, at(corner, deck_y + 1),
    );

    println!(
        "Engawa house OK: nominal={:?}, ground={:?}, top={:?}, floors={floors}, \
         deck_y={deck_y}, roof_y={roof_y}, blueprints=output/engawa_house.{{txt,svg}}",
        rect, ground, top,
    );
}

/// Engawa cellar regression: a cellar under an engawa house must shrink to the
/// inset walls, not the nominal footprint — otherwise it carves a void out under
/// the veranda deck. Searches seeds for a single-rect Japanese House that both
/// insets *and* rolls a cellar, then asserts the cellar void reaches the inset
/// interior but the ground under a deck-ring cell stays solid.
#[tokio::test]
async fn engawa_cellar_shrinks_to_inset_offline() {
    use crate::editor::World;
    use crate::geometry::Rect3D;
    use crate::util::init_logger;
    use crate::generator::data::LoadedData;
    use crate::generator::buildings_v2::footprint::Footprint;
    use crate::generator::buildings_v2::frame::CELLAR_FLOOR;
    use crate::generator::buildings_v2::roof::RoofStyle;
    use crate::generator::buildings_v2::roof::hipped::HippedPitch;
    use crate::generator::buildings_v2::engawa::plan_engawa;
    use crate::generator::buildings_v2::{BuildCtx, BuildingContext, Culture, build_house};

    init_logger();

    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
    let data = LoadedData::load().expect("Failed to load data");
    let palette = data.palettes.get(&Culture::Japanese.palette_id()).expect("palette").clone();

    // A single-rect 13×11 footprint: insets to 11×9 (cellar-worthy) and is large
    // enough that the House class keeps it one rect across seeds.
    let rect = Rect2D::from_points(Point2D::new(70, 70), Point2D::new(82, 80));
    let bounds = Rect2D::from_points(Point2D::new(56, 56), Point2D::new(98, 98));
    assert!(plan_engawa(&Footprint::from_rect(rect)).is_some(), "test setup: rect must be engawa-eligible");

    // Search seeds for one that rolls a cellar (House = 2/5 chance).
    let mut built = None;
    for seed in 0..80i64 {
        let world = World::synthetic(build_area, 64);
        let mut editor = world.get_offline_editor();
        let mut rng = RNG::new(seed);
        let footprint = Footprint::from_rect(rect);
        let mut bctx = BuildingContext::new(Culture::Japanese, SizeClass::House, RoofStyle::Hipped(HippedPitch::Stairs));
        bctx.engawa = true;
        let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
        let house = build_house(&mut ctx, footprint, &bctx, bounds).await.expect("build_house failed");
        if house.has_cellar {
            built = Some((editor, house));
            break;
        }
    }
    let (editor, house) = built.expect("no engawa House rolled a cellar in 80 seeds");

    let inset = house.frame.rect_at(0, 0).expect("ground extent");
    let cellar_floor_y = house.frame.floor_y(CELLAR_FLOOR);
    let at = |p: Point2D, y: i32| editor.try_get_block(Point3D::new(p.x, y, p.y));
    let is_air = |b: &Block| b.id == "air".into() || b.id == "minecraft:air".into();
    let air_at = |p: Point2D| at(p, cellar_floor_y).as_ref().map_or(false, is_air);

    // Count void (air) cells at cellar-floor level, region by region. (We count
    // over regions rather than probing one cell because cellar furniture lands on
    // arbitrary interior cells — `HashMap` iteration order makes which ones
    // non-deterministic across runs — so a single-cell air probe is flaky.)

    // Inside the inset core → the cellar void. Several cells must be air (the
    // cellar was excavated here), even if furniture occupies some.
    let interior_air = inset.iter().filter(|&p| air_at(p)).count();
    assert!(
        interior_air >= 4,
        "cellar should be excavated under the inset core (got {} air cells at y={})",
        interior_air, cellar_floor_y,
    );

    // Inside the nominal footprint but OUTSIDE the inset core — the ring the
    // engawa pulled the walls in from. A cellar tracking the nominal footprint
    // would excavate void here; shrunk to the inset walls, there must be none.
    let overreach_air = rect.iter()
        .filter(|p| !inset.contains(*p))
        .filter(|&p| air_at(p))
        .count();
    assert_eq!(
        overreach_air, 0,
        "cellar over-reached: {} void cells outside the inset core at y={} (should track the inset, not nominal, footprint)",
        overreach_air, cellar_floor_y,
    );

    println!(
        "Engawa cellar OK: inset={:?}, cellar_floor_y={}, interior_air={interior_air}, overreach_air=0",
        inset, cellar_floor_y,
    );
}

/// Dome regression: a desert (flat-roof) house on a square footprint must grow
/// a dome — a stepped hemisphere rising above the wall-top deck — rather than a
/// flat slab deck. Builds a 7×7 single-rect house and asserts the centre column
/// has solid roof blocks well above the deck, capped by air.
#[tokio::test]
async fn desert_dome_built_on_square_rect_offline() {
    use crate::editor::World;
    use crate::geometry::Rect3D;
    use crate::util::init_logger;
    use crate::generator::data::LoadedData;
    use crate::generator::buildings_v2::footprint::Footprint;
    use crate::generator::buildings_v2::roof::RoofStyle;
    use crate::generator::buildings_v2::roof::dome::is_dome_eligible;
    use crate::generator::buildings_v2::{BuildCtx, BuildingContext, Culture, build_house};

    init_logger();

    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(127, 127, 127));
    let world = World::synthetic(build_area, 64);
    let mut editor = world.get_offline_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette = data.palettes.get(&Culture::Desert.palette_id()).expect("palette").clone();

    // A 7×7 square rect → dome-eligible (odd side, ≥ MIN_DOME_SIDE).
    let square = Rect2D::from_points(Point2D::new(40, 40), Point2D::new(46, 46));
    assert!(is_dome_eligible(&square), "test setup: 7×7 rect must be dome-eligible");
    let bounds = Rect2D::from_points(Point2D::new(30, 30), Point2D::new(56, 56));

    let mut rng = RNG::new(7);
    let footprint = Footprint::from_rect(square);
    let bctx = BuildingContext::new(Culture::Desert, SizeClass::House, RoofStyle::Flat);
    let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
    let house = build_house(&mut ctx, footprint, &bctx, bounds).await.expect("build_house failed");

    let center = square.midpoint();
    let roof_y = house.frame.roof_y(0);
    let deck_y = roof_y - 2;

    let at = |p: Point2D, y: i32| editor.try_get_block(Point3D::new(p.x, y, p.y));
    let is_air = |b: &Block| b.id == "air".into() || b.id == "minecraft:air".into();
    let is_prismarine = |b: &Block| b.id.as_str().contains("dark_prismarine");

    // Flat prismarine layer seals the room at wall-top.
    assert!(
        at(center, deck_y).as_ref().map_or(false, is_prismarine),
        "flat layer at y={deck_y} should be dark prismarine, got {:?}", at(center, deck_y),
    );
    // Square base course at deck_y+1 fills the whole square — including corners
    // (the "square not circle" layer the dome sits on).
    let corner = square.min();
    assert!(
        at(corner, deck_y + 1).as_ref().map_or(false, is_prismarine),
        "square base corner at y={} should be dark prismarine, got {:?}", deck_y + 1, at(corner, deck_y + 1),
    );
    // But the dome curve does not reach the corner — air above the base there.
    assert!(
        at(corner, deck_y + 2).as_ref().map_or(true, is_air),
        "corner should be bare above the base course, got {:?}", at(corner, deck_y + 2),
    );
    // A 7×7 hemisphere (r=3.5) curves up the centre column to deck_y+3, slab crown
    // at deck_y+4.
    let apex_y = deck_y + 3;
    assert!(
        at(center, apex_y).as_ref().map_or(false, is_prismarine),
        "dome apex at y={apex_y} should be dark prismarine, got {:?}", at(center, apex_y),
    );
    // And the dome is finite — nothing solid well above the crown.
    assert!(
        at(center, deck_y + 6).as_ref().map_or(true, is_air),
        "no roof blocks expected at y={}, got {:?}", deck_y + 6, at(center, deck_y + 6),
    );

    println!("Desert dome OK: deck_y={deck_y}, apex_y={apex_y}, center={center:?}");
}

/// Cellar regression: a Manor always rolls a cellar (size chance = always) and
/// the synthetic world is dry, so `has_cellar` must be true and the carved
/// volume must be present in the editor — air at the cellar floor surface and a
/// solid stone slab one block below it, beneath the core rect.
#[tokio::test]
async fn cellar_built_under_manor_offline() {
    use crate::editor::World;
    use crate::geometry::Rect3D;
    use crate::util::init_logger;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::buildings_v2::frame::CELLAR_FLOOR;
    use crate::generator::buildings_v2::roof::RoofStyle;
    use crate::generator::buildings_v2::roof::gable::GablePitch;
    use crate::generator::buildings_v2::{BuildCtx, BuildingContext, Culture, build_house};

    init_logger();

    let build_area = Rect3D::from_points(
        Point3D::new(0, 0, 0),
        Point3D::new(255, 127, 255),
    );
    let world = World::synthetic(build_area, 64);
    let mut editor = world.get_offline_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette_id: PaletteId = "medieval_spruce".into();
    let palette = data.palettes.get(&palette_id).expect("Palette not found").clone();

    let bounds = Rect2D::from_points(
        Point2D::new(96, 96),
        Point2D::new(159, 159),
    );

    let mut rng = RNG::new(42);
    let plot = Plot::fully_usable(bounds);
    let footprint = generate_footprint(&mut rng, &plot, &SizeClass::Manor)
        .expect("Failed to generate Manor footprint");

    let bctx = BuildingContext::new(Culture::Medieval, SizeClass::Manor, RoofStyle::Gable(GablePitch::Double));
    let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
    let house = build_house(&mut ctx, footprint, &bctx, bounds)
        .await
        .expect("build_house failed");

    assert!(house.has_cellar, "Manor in a dry synthetic world must get a cellar");

    let floor_y = house.frame.floor_y(CELLAR_FLOOR);
    let slab_y = floor_y - 1;
    let core = house.footprint.rects()[0];

    // Probe the stair landing, not the core centroid: the centroid can host
    // cellar furniture (a barrel, a hay pile), which would sit at floor_y and
    // fail the "floor surface is air" check even though the void was carved
    // correctly. The landing (cellar_stair position 0) is reserved walkable and
    // carries no stair block, so it's reliably the carved void — air at floor_y,
    // stone slab below.
    let probe = house.cellar_stair.as_ref().expect("cellar must have a stair")[0];

    let at = |y: i32| editor.try_get_block(Point3D::new(probe.x, y, probe.y));
    let is_air = |b: &Block| b.id == "air".into() || b.id == "minecraft:air".into();
    let floor_block = at(floor_y);
    let slab_block = at(slab_y);

    assert!(
        floor_block.as_ref().map_or(true, is_air),
        "cellar floor surface at y={} should be air, got {:?}", floor_y, floor_block,
    );
    assert!(
        slab_block.as_ref().map_or(false, |b| !is_air(b)),
        "cellar floor slab at y={} should be solid stone, got {:?}", slab_y, slab_block,
    );

    println!("Manor cellar carved: floor_y={}, slab_y={}, probe={:?}, core={:?}", floor_y, slab_y, probe, core);
}

/// Property test: run the offline pipeline across many seeds and assert that
/// every building satisfies the structural invariants. This is the canonical
/// regression guard for the furnish/rooms/walls pipeline — any change that
/// breaks wall-slot adjacency or connectivity will fail here.
#[tokio::test]
async fn pipeline_invariants_property_test() {
    use crate::editor::World;
    use crate::geometry::Rect3D;
    use crate::util::init_logger;

    init_logger();

    let build_area = Rect3D::from_points(
        Point3D::new(0, 0, 0),
        Point3D::new(255, 127, 255),
    );
    let bounds = Rect2D::from_points(
        Point2D::new(64, 64),
        Point2D::new(191, 191),
    );

    // A sweep across seeds. Each seed produces up to 12 buildings spanning
    // Cottage / House / Hall with varying pitches, so the space exercised
    // per run is ~60 buildings per 5 seeds.
    let seeds: [i64; 20] = [
        1, 7, 13, 42, 99, 123, 256, 777, 1000, 2000,
        3000, 4000, 5000, 6000, 7000, 8000, 9000, 12345, 54321, 98765,
    ];

    use crate::generator::buildings_v2::Culture;
    let mut total_buildings = 0;
    // Sweep Medieval (gable roofs) and Desert (flat roofs + square-rect domes)
    // so both roof families are exercised against the invariant checks.
    for culture in [Culture::Medieval, Culture::Desert] {
        for &seed in &seeds {
            // Fresh synthetic world + editor per seed so block caches and build
            // claims from one seed don't contaminate the next.
            let world = World::synthetic(build_area, 64);
            let mut editor = world.get_offline_editor();
            total_buildings += run_furnished_houses_pipeline(&mut editor, bounds, seed, false, culture).await;
        }
    }

    println!("Property test: {} buildings across {} seeds × 2 cultures, all invariants hold",
             total_buildings, seeds.len());
}

/// Jetty property test: same sweep as `pipeline_invariants_property_test`, but
/// every building has `bctx.jetty = true`. Most footprints are multi-rect or
/// single-floor and silently fall back to flush walls; the eligible single-rect
/// 2+ floor houses get jettied upper extents. Verifies invariants hold on the
/// jettied subset (walls/roof/stairs all sane around the overhang).
#[tokio::test]
async fn pipeline_invariants_property_test_jetty() {
    use crate::editor::World;
    use crate::geometry::Rect3D;
    use crate::util::init_logger;

    init_logger();

    let build_area = Rect3D::from_points(
        Point3D::new(0, 0, 0),
        Point3D::new(255, 127, 255),
    );
    let bounds = Rect2D::from_points(
        Point2D::new(64, 64),
        Point2D::new(191, 191),
    );

    let seeds: [i64; 10] = [1, 7, 13, 42, 99, 123, 256, 777, 1000, 2000];

    let mut total_buildings = 0;
    for &seed in &seeds {
        let world = World::synthetic(build_area, 64);
        let mut editor = world.get_offline_editor();
        use crate::generator::buildings_v2::Culture;
        total_buildings += run_furnished_houses_pipeline_jettied(
            &mut editor, bounds, seed, false, Culture::Medieval, true,
        ).await;
    }

    println!("Jetty property test: {} buildings across {} seeds, all invariants hold",
             total_buildings, seeds.len());
}

/// Phase 3 guard: run multi-rect Manors and Halls through the offline pipeline
/// with jetty forced on, across many seeds. `build_house` applies the jetty and
/// runs `check_building_invariants` internally, so any overhang that breaks a
/// wall/reachability invariant panics here. Also asserts the jetty actually
/// triggered on some multi-rect building, so a regression that silently stopped
/// jettying can't let this pass vacuously.
#[tokio::test]
async fn pipeline_invariants_property_test_jetty_multirect() {
    use crate::editor::World;
    use crate::geometry::Rect3D;
    use crate::util::init_logger;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::buildings_v2::roof::RoofStyle;
    use crate::generator::buildings_v2::roof::gable::GablePitch;
    use crate::generator::buildings_v2::{BuildCtx, BuildingContext, Culture, build_house};

    init_logger();

    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
    let bounds = Rect2D::from_points(Point2D::new(64, 64), Point2D::new(191, 191));
    let styles = [
        RoofStyle::Gable(GablePitch::Slab),
        RoofStyle::Gable(GablePitch::Stairs),
        RoofStyle::Gable(GablePitch::Double),
    ];

    let data = LoadedData::load().expect("Failed to load data");
    let palette_id: PaletteId = "medieval_spruce".into();
    let palette = data.palettes.get(&palette_id).expect("Palette not found").clone();

    let seeds: [i64; 10] = [1, 7, 13, 42, 99, 123, 256, 777, 1000, 2000];
    let mut total = 0usize;
    let mut multi_rect = 0usize;
    let mut jettied = 0usize;

    for &seed in &seeds {
        let world = World::synthetic(build_area, 64);
        let mut editor = world.get_offline_editor();
        let mut rng = RNG::new(seed);
        let mut plot = Plot::fully_usable(bounds);
        let footprints = fill_plot_multi(&mut rng, &mut plot, &[SizeClass::Manor, SizeClass::Hall], 8, 0);

        let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
        for (i, (footprint, size_class)) in footprints.into_iter().enumerate() {
            let mut bctx = BuildingContext::new(Culture::Medieval, size_class, styles[i % styles.len()]);
            bctx.jetty = true;
            let house = build_house(&mut ctx, footprint, &bctx, bounds)
                .await
                .unwrap_or_else(|msg| panic!("seed {} building {} ({:?}) invariant: {}", seed, i, size_class, msg));

            total += 1;
            let rects = house.footprint.rects().len();
            if rects > 1 { multi_rect += 1; }
            // Jetty triggered if any rect's top-floor extent grew past its ground extent.
            let grew = (0..rects).any(|r| match (house.frame.rect_at(r, 0), house.frame.rect_at_top(r)) {
                (Some(g), Some(t)) => t.area() > g.area(),
                _ => false,
            });
            if grew { jettied += 1; }
        }
        ctx.editor.flush_buffer().await;
    }

    assert!(multi_rect > 0, "test exercised no multi-rect buildings");
    assert!(jettied > 0, "jetty never triggered — multi-rect compensation may be broken");
    println!(
        "Multi-rect jetty property test: {} buildings ({} multi-rect, {} jettied) across {} seeds, all invariants hold",
        total, multi_rect, jettied, seeds.len(),
    );
}

/// Engawa property test: run Japanese Cottages/Houses through the offline
/// pipeline with `bctx.engawa = true` across many seeds. `build_house` applies
/// the inset/raise/deck/skirt and runs `check_building_invariants` internally,
/// so any veranda that breaks a wall/reachability invariant panics here. Asserts
/// the engawa actually triggered on some building (ground extent inset below the
/// nominal footprint), so a regression that silently stopped insetting can't let
/// this pass vacuously.
#[tokio::test]
async fn pipeline_invariants_property_test_engawa() {
    use crate::editor::World;
    use crate::geometry::Rect3D;
    use crate::util::init_logger;
    use crate::generator::data::LoadedData;
    use crate::generator::buildings_v2::roof::RoofStyle;
    use crate::generator::buildings_v2::roof::hipped::HippedPitch;
    use crate::generator::buildings_v2::{BuildCtx, BuildingContext, Culture, build_house};

    init_logger();

    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
    let bounds = Rect2D::from_points(Point2D::new(64, 64), Point2D::new(191, 191));
    let styles = [
        RoofStyle::Hipped(HippedPitch::Slab),
        RoofStyle::Hipped(HippedPitch::Stairs),
        RoofStyle::Irimoya,
    ];

    let data = LoadedData::load().expect("Failed to load data");
    let palette = data.palettes.get(&Culture::Japanese.palette_id()).expect("palette").clone();

    let seeds: [i64; 10] = [1, 7, 13, 42, 99, 123, 256, 777, 1000, 2000];
    let mut total = 0usize;
    let mut engawad = 0usize;

    for &seed in &seeds {
        let world = World::synthetic(build_area, 64);
        let mut editor = world.get_offline_editor();
        let mut rng = RNG::new(seed);
        let mut plot = Plot::fully_usable(bounds);
        // Large classes only — the deep ground inset (2 per side) needs sizeable
        // footprints, matching how engawa is actually gated in the settlement.
        let footprints = fill_plot_multi(&mut rng, &mut plot, &[SizeClass::Hall, SizeClass::Manor], 8, 0);

        let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
        for (i, (footprint, size_class)) in footprints.into_iter().enumerate() {
            let mut bctx = BuildingContext::new(Culture::Japanese, size_class, styles[i % styles.len()]);
            bctx.engawa = true;
            let house = build_house(&mut ctx, footprint, &bctx, bounds)
                .await
                .unwrap_or_else(|msg| panic!("seed {} building {} ({:?}) invariant: {}", seed, i, size_class, msg));

            total += 1;
            // Engawa triggered if the frame's ground extent is inset below the
            // nominal footprint rect it was built from.
            let inset = (0..house.footprint.rects().len()).any(|r| {
                match house.frame.rect_at(r, 0) {
                    Some(g) => g.area() < house.footprint.rects()[r].area(),
                    None => false,
                }
            });
            if inset { engawad += 1; }
        }
        ctx.editor.flush_buffer().await;
    }

    assert!(engawad > 0, "engawa never triggered — the inset transform may be broken");
    println!(
        "Engawa property test: {} buildings ({} engawa'd) across {} seeds, all invariants hold",
        total, engawad, seeds.len(),
    );
}

/// Manor-only offline reproducer for the invariant (a) failure seen in
/// `walls::test::build_village`. The main property test only covers
/// Cottage/House/Hall, so Manor regressions slip through. Iterates seeds and
/// dumps the first failing seed + building index + frame outline + room
/// interior so we can pinpoint which Storage room reaches a missing wall.
#[tokio::test]
async fn manor_invariant_repro() {
    use crate::editor::World;
    use crate::geometry::Rect3D;
    use crate::util::init_logger;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::buildings_v2::roof::RoofStyle;
    use crate::generator::buildings_v2::roof::gable::GablePitch;
    use crate::generator::buildings_v2::{BuildCtx, BuildingContext, Culture, build_house};
    use super::invariants::wall_cells_on_floor;

    init_logger();

    let build_area = Rect3D::from_points(
        Point3D::new(0, 0, 0),
        Point3D::new(255, 127, 255),
    );
    let bounds = Rect2D::from_points(Point2D::new(64, 64), Point2D::new(191, 191));
    let styles = [
        RoofStyle::Gable(GablePitch::Slab),
        RoofStyle::Gable(GablePitch::Stairs),
        RoofStyle::Gable(GablePitch::Double),
    ];

    let data = LoadedData::load().expect("Failed to load data");
    let palette_id: PaletteId = "medieval_spruce".into();
    let palette = data.palettes.get(&palette_id).expect("Palette not found").clone();

    let seeds: [i64; 10] = [1, 7, 13, 42, 99, 123, 256, 777, 1000, 2000];
    let mut total = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for &seed in &seeds {
        let world = World::synthetic(build_area, 64);
        let mut editor = world.get_offline_editor();
        let mut rng = RNG::new(seed);
        let mut plot = Plot::fully_usable(bounds);
        let footprints = fill_plot_multi(&mut rng, &mut plot, &[SizeClass::Manor], 8, 0);
        let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);

        for (i, (footprint, size_class)) in footprints.into_iter().enumerate() {
            let pitch = styles[(seed as usize + i) % styles.len()];
            let bctx = BuildingContext::new(Culture::Medieval, size_class, pitch);
            let rects_dbg: Vec<_> = footprint.rects().to_vec();
            match build_house(&mut ctx, footprint, &bctx, bounds).await {
                Ok(_) => { total += 1; }
                Err(msg) => {
                    failures.push(format!(
                        "seed={} manor#{} pitch={:?} rects={:?}: {}",
                        seed, i, pitch, rects_dbg, msg,
                    ));
                }
            }
        }
    }

    if !failures.is_empty() {
        eprintln!("Manor invariant failures ({} / {} attempts):", failures.len(), total + failures.len());
        for f in &failures {
            eprintln!("  - {}", f);
        }
        // Re-trigger the first failing seed/building with extra dump so we can
        // see the frame outline and the room interior the invariant complains
        // about.
        let first = &failures[0];
        let seed_marker = first.split_whitespace().next().unwrap();
        let seed: i64 = seed_marker.trim_start_matches("seed=").parse().unwrap();
        let world = World::synthetic(build_area, 64);
        let mut editor = world.get_offline_editor();
        let mut rng = RNG::new(seed);
        let mut plot = Plot::fully_usable(bounds);
        let footprints = fill_plot_multi(&mut rng, &mut plot, &[SizeClass::Manor], 8, 0);
        let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
        for (i, (footprint, size_class)) in footprints.into_iter().enumerate() {
            let pitch = styles[(seed as usize + i) % styles.len()];
            let bctx = BuildingContext::new(Culture::Medieval, size_class, pitch);
            let footprint_clone = footprint.clone();
            if let Err(msg) = build_house(&mut ctx, footprint, &bctx, bounds).await {
                eprintln!("--- DUMP seed={} manor#{} ---", seed, i);
                eprintln!("error: {}", msg);
                eprintln!("footprint rects: {:?}", footprint_clone.rects());
                use crate::generator::buildings_v2::frame::generate_frame;
                let mut dump_rng = RNG::new(seed);
                // skip prior buildings' worth of RNG draws... too painful.
                let frame = generate_frame(footprint_clone, 64, &size_class, &mut dump_rng);
                for f in 0..frame.max_floors() {
                    let walls = wall_cells_on_floor(&frame, f);
                    eprintln!("  floor {} wall cells ({}): {:?}", f, walls.len(),
                              walls.iter().take(40).collect::<Vec<_>>());
                }
                break;
            }
        }
        panic!("{} manor builds violated invariants", failures.len());
    }

    println!("Manor repro: {} successful, 0 failures across {} seeds", total, seeds.len());
}

#[tokio::test]
async fn build_single_hall() {
    use crate::editor::World;
    use crate::http_mod::GDMCHTTPProvider;
    use crate::util::init_logger;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::buildings_v2::blueprint::{build_blueprint, render_svg};
    use crate::generator::buildings_v2::roof::RoofStyle;
    use crate::generator::buildings_v2::roof::gable::GablePitch;
    use crate::generator::buildings_v2::{BuildCtx, build_house};

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette_id: PaletteId = "medieval_spruce".into();
    let palette = data.palettes.get(&palette_id).expect("Palette not found").clone();

    let world_rect = editor.world().world_rect_2d();
    let center = world_rect.midpoint();
    let bounds = Rect2D::from_points(
        Point2D::new(center.x - 32, center.y - 32),
        Point2D::new(center.x + 31, center.y + 31),
    );

    let mut rng = RNG::new(42);
    let plot = Plot::fully_usable(bounds);
    let footprint = generate_footprint(&mut rng, &plot, &SizeClass::Hall)
        .expect("Failed to generate Hall footprint");

    use crate::generator::buildings_v2::{Culture, BuildingContext};
    let bctx = BuildingContext::new(Culture::Medieval, SizeClass::Hall, RoofStyle::Gable(GablePitch::Double));
    let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
    let house = build_house(&mut ctx, footprint, &bctx, bounds)
        .await
        .expect("build_house failed");

    let blueprint = build_blueprint(&house.frame, &house.wall_segs, &house.floor_plan, &house.room_plan, house.has_attic);
    let svg = render_svg(&blueprint);
    std::fs::create_dir_all("output").ok();
    std::fs::write("output/hall.svg", &svg).expect("Failed to write SVG");

    println!(
        "Hall: rects={}, floors={}, rooms={}, blueprint=output/hall.svg",
        house.footprint.rects().len(), house.frame.max_floors(), house.room_plan.rooms.len(),
    );

    editor.flush_buffer().await;
}

/// Build a single jettied House on a live Minecraft server. Sweeps seeds until
/// it finds a single-rect 2-floor footprint (the Phase 2 jetty eligibility),
/// then forces `bctx.jetty = true`. Requires the GDMC HTTP mod running.
#[tokio::test]
async fn build_single_jetty_house() {
    use crate::editor::World;
    use crate::http_mod::GDMCHTTPProvider;
    use crate::util::init_logger;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::buildings_v2::blueprint::{build_blueprint, render_svg};
    use crate::generator::buildings_v2::roof::RoofStyle;
    use crate::generator::buildings_v2::roof::gable::GablePitch;
    use crate::generator::buildings_v2::frame::generate_frame;
    use crate::generator::buildings_v2::{BuildCtx, BuildingContext, Culture, build_house};

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette_id: PaletteId = "medieval_spruce".into();
    let palette = data.palettes.get(&palette_id).expect("Palette not found").clone();

    let world_rect = editor.world().world_rect_2d();
    let center = world_rect.midpoint();
    let bounds = Rect2D::from_points(
        Point2D::new(center.x - 16, center.y - 16),
        Point2D::new(center.x + 15, center.y + 15),
    );

    // Search seeds for a single-rect 2-floor House footprint (jetty-eligible).
    let mut chosen: Option<(i64, Footprint)> = None;
    for seed in 0..200i64 {
        let mut rng = RNG::new(seed);
        let plot = Plot::fully_usable(bounds);
        let Some(fp) = generate_footprint(&mut rng, &plot, &SizeClass::House) else {
            continue;
        };
        if fp.rects().len() != 1 {
            continue;
        }
        let frame = generate_frame(fp.clone(), 0, &SizeClass::House, &mut rng);
        if frame.max_floors() >= 2 {
            chosen = Some((seed, fp));
            break;
        }
    }
    let (seed, footprint) = chosen.expect("no single-rect 2-floor House found in 200 seeds");

    let mut rng = RNG::new(seed);
    // Re-roll the footprint to keep RNG state aligned with the seed.
    let plot = Plot::fully_usable(bounds);
    let _ = generate_footprint(&mut rng, &plot, &SizeClass::House);

    let mut bctx = BuildingContext::new(Culture::Medieval, SizeClass::House, RoofStyle::Gable(GablePitch::Stairs));
    bctx.jetty = true;

    let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
    let house = build_house(&mut ctx, footprint, &bctx, bounds)
        .await
        .expect("build_house failed");

    let ground = house.frame.rect_at(0, 0).unwrap();
    let upper = house.frame.rect_at(0, 1).unwrap();
    let blueprint = build_blueprint(&house.frame, &house.wall_segs, &house.floor_plan, &house.room_plan, house.has_attic);
    let svg = render_svg(&blueprint);
    std::fs::create_dir_all("output").ok();
    std::fs::write("output/jetty_house.svg", &svg).expect("Failed to write SVG");

    println!(
        "Jetty House at seed={}, bounds={:?}\n  ground rect: {:?}..{:?}\n  upper rect:  {:?}..{:?}\n  blueprint=output/jetty_house.svg",
        seed, bounds, ground.min(), ground.max(), upper.min(), upper.max(),
    );

    editor.flush_buffer().await;
}

/// Live: places a small grid of Manors and Halls, each generated from a distinct
/// seed, with jetty forced on. A label sign on each marks the seed, class, rect
/// count, and whether jetty actually applied. With Phase 3 multi-rect jetty,
/// each rect overhangs on its open-air sides, so Manors/Halls should mostly read
/// "JETTY". Run with: `cargo test build_jetty_manors_halls_live -- --nocapture`
#[tokio::test]
async fn build_jetty_manors_halls_live() {
    use crate::editor::World;
    use crate::http_mod::GDMCHTTPProvider;
    use crate::util::init_logger;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::buildings_v2::roof::RoofStyle;
    use crate::generator::buildings_v2::roof::gable::GablePitch;
    use crate::generator::buildings_v2::{BuildCtx, BuildingContext, Culture, build_house};

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette_id: PaletteId = "medieval_spruce".into();
    let palette = data.palettes.get(&palette_id).expect("Palette not found").clone();

    let center = editor.world().world_rect_2d().midpoint();

    // 3x2 grid of 72x72 cells, one building per cell, distinct seed each.
    // (seed, size class) per cell — alternating Manor / Hall.
    let cells: [(i64, SizeClass); 6] = [
        (3,  SizeClass::Manor), (7,  SizeClass::Hall), (11, SizeClass::Manor),
        (19, SizeClass::Hall),  (23, SizeClass::Manor), (31, SizeClass::Hall),
    ];
    let col_off = [-84i32, 0, 84];
    let row_off = [-42i32, 42];
    const HALF: i32 = 35; // 70x70 usable cell

    // One ctx for the whole grid; reseed the rng in place per building so each
    // gets a distinct, reproducible layout without rebinding the borrowed ref.
    let mut rng = RNG::new(0);
    let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
    for (i, (seed, size_class)) in cells.iter().enumerate() {
        let cx = center.x + col_off[i % 3];
        let cz = center.y + row_off[i / 3];
        let bounds = Rect2D::from_points(
            Point2D::new(cx - HALF, cz - HALF),
            Point2D::new(cx + HALF, cz + HALF),
        );

        *ctx.rng = RNG::new(*seed);
        let plot = Plot::fully_usable(bounds);
        let Some(footprint) = generate_footprint(ctx.rng, &plot, size_class) else {
            println!("seed {} {:?}: no footprint, skipped", seed, size_class);
            continue;
        };

        let pitch = RoofStyle::Gable(GablePitch::Stairs);
        let mut bctx = BuildingContext::new(Culture::Medieval, *size_class, pitch);
        bctx.jetty = true;

        let house = build_house(&mut ctx, footprint, &bctx, bounds)
            .await
            .expect("build_house failed");

        // Jettied if any rect's top-floor extent grew past its ground extent.
        let rects = house.footprint.rects().len();
        let jettied = (0..rects).any(|r| {
            match (house.frame.rect_at(r, 0), house.frame.rect_at_top(r)) {
                (Some(g), Some(t)) => t.area() > g.area(),
                _ => false,
            }
        });
        let tag = if jettied { "JETTY" } else { "flush" };

        // Label sign at the building center, one block above the ground floor.
        let sx = (house.footprint.bounds().min().x + house.footprint.bounds().max().x) / 2;
        let sz = (house.footprint.bounds().min().y + house.footprint.bounds().max().y) / 2;
        let sy = house.frame.floor_y(0) + 1;
        let sign = sign_block(&format!("{:?} s{}", size_class, seed), tag);
        ctx.editor.place_block_forced(&sign, Point3D::new(sx, sy, sz)).await;

        println!(
            "seed {:>2} {:?}: rects={}, floors={}, {}",
            seed, size_class, rects, house.frame.max_floors(), tag,
        );
    }

    ctx.editor.flush_buffer().await;
    println!("Done — manor/hall jetty grid placed live");
}

/// Live: places a grid of Japanese houses with `bctx.engawa = true`, so each one
/// insets its walls, raises onto a wooden platform, decks the perimeter ring, and
/// gets a pent-roof skirt. A label sign marks the seed/class and whether the
/// engawa actually applied. Requires the GDMC HTTP mod running.
/// Run with: `cargo test build_engawa_houses_live -- --nocapture`
#[tokio::test]
async fn build_engawa_houses_live() {
    use crate::editor::World;
    use crate::http_mod::GDMCHTTPProvider;
    use crate::util::init_logger;
    use crate::generator::data::LoadedData;
    use crate::generator::buildings_v2::roof::RoofStyle;
    use crate::generator::buildings_v2::roof::hipped::HippedPitch;
    use crate::generator::buildings_v2::engawa::plan_engawa;
    use crate::generator::buildings_v2::{BuildCtx, BuildingContext, Culture, build_house};

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette = data.palettes.get(&Culture::Japanese.palette_id()).expect("Japanese palette not found").clone();

    let center = editor.world().world_rect_2d().midpoint();

    // 3x2 grid, one large building per cell. Each cell searches seeds for an
    // engawa-eligible footprint (one that survives the deep ground inset of 2 per
    // side) so every building shows a veranda — engawa is only for large
    // buildings, so we use Halls and Manors.
    // With the relaxed gate (only the core must afford the deep inset; wings
    // clamp to what they can fit), Halls qualify too — so the demo mixes both.
    let cells: [(SizeClass, RoofStyle); 6] = [
        (SizeClass::Manor, RoofStyle::Hipped(HippedPitch::Stairs)),
        (SizeClass::Hall,  RoofStyle::Irimoya),
        (SizeClass::Manor, RoofStyle::Hipped(HippedPitch::Slab)),
        (SizeClass::Hall,  RoofStyle::Hipped(HippedPitch::Stairs)),
        (SizeClass::Manor, RoofStyle::Irimoya),
        (SizeClass::Hall,  RoofStyle::Hipped(HippedPitch::Slab)),
    ];
    let col_off = [-90i32, 0, 90];
    let row_off = [-45i32, 45];
    const HALF: i32 = 38; // 76x76 usable cell

    let mut rng = RNG::new(0);
    let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
    for (i, (size_class, roof_style)) in cells.iter().enumerate() {
        let cx = center.x + col_off[i % 3];
        let cz = center.y + row_off[i / 3];
        let bounds = Rect2D::from_points(
            Point2D::new(cx - HALF, cz - HALF),
            Point2D::new(cx + HALF, cz + HALF),
        );

        // Each cell searches its own seed range so the buildings differ; the
        // window is generous since even Manors don't always clear the inset.
        let mut found: Option<(i64, Footprint)> = None;
        for seed in (i as i64 * 3000 + 1)..(i as i64 * 3000 + 3000) {
            let mut probe = RNG::new(seed);
            let plot = Plot::fully_usable(bounds);
            if let Some(fp) = generate_footprint(&mut probe, &plot, size_class) {
                if plan_engawa(&fp).is_some() {
                    found = Some((seed, fp));
                    break;
                }
            }
        }
        let Some((seed, footprint)) = found else {
            println!("{:?} cell {}: no engawa-eligible footprint, skipped", size_class, i);
            continue;
        };

        // Re-roll on a fresh rng seeded identically so the build matches the probe.
        *ctx.rng = RNG::new(seed);
        let plot = Plot::fully_usable(bounds);
        let _ = generate_footprint(ctx.rng, &plot, size_class);

        let mut bctx = BuildingContext::new(Culture::Japanese, *size_class, *roof_style);
        bctx.engawa = true;

        let house = build_house(&mut ctx, footprint, &bctx, bounds)
            .await
            .expect("build_house failed");

        // Engawa applied if the ground extent is inset below the nominal footprint.
        let engawad = (0..house.footprint.rects().len()).any(|r| {
            match house.frame.rect_at(r, 0) {
                Some(g) => g.area() < house.footprint.rects()[r].area(),
                None => false,
            }
        });
        let tag = if engawad { "ENGAWA" } else { "flush" };

        let sx = (house.footprint.bounds().min().x + house.footprint.bounds().max().x) / 2;
        let sz = (house.footprint.bounds().min().y + house.footprint.bounds().max().y) / 2;
        let sy = house.frame.floor_y(0) + 1;
        let sign = sign_block(&format!("{:?} s{}", size_class, seed), tag);
        ctx.editor.place_block_forced(&sign, Point3D::new(sx, sy, sz)).await;

        println!(
            "seed {:>2} {:?}: rects={}, floors={}, {}",
            seed, size_class, house.footprint.rects().len(), house.frame.max_floors(), tag,
        );
    }

    ctx.editor.flush_buffer().await;
    println!("Done — Japanese engawa house grid placed live");
}

/// Generates parcels, partitions urban area into city blocks, then fills each block
/// with buildings_v2 buildings of mixed sizes and randomized roof materials.
#[tokio::test]
async fn settlement_with_buildings_v2() {
    use crate::editor::World;
    use crate::http_mod::GDMCHTTPProvider;
    use crate::util::init_logger;
    use crate::generator::data::LoadedData;
    use crate::generator::districts::generate_parcels;
    use crate::generator::materials::PaletteId;
    use crate::generator::buildings_v2::roof::RoofStyle;
    use crate::generator::buildings_v2::roof::gable::GablePitch;
    use crate::generator::buildings_v2::{BuildCtx, BuildingContext, Culture, build_house};
    use crate::generator::buildings::get_city_blocks_and_off_limits;
    use crate::geometry::get_outer_and_inner_points;

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let mut rng = RNG::new(13);

    // Step 1: Generate parcels (creates urban/rural classification)
    generate_parcels(rng.next_i64().into(), &mut editor).await;

    let data = LoadedData::load().expect("Failed to load data");
    let base_palette_id: PaletteId = "medieval_spruce".into();
    let base_palette = data.palettes.get(&base_palette_id).expect("Base palette not found").clone();

    let roof_ids: Vec<PaletteId> = vec![
        "acacia_wood_roof".into(),
        "blackstone_roof".into(),
        "blue_wood_roof".into(),
        "brick_roof".into(),
        "oak_wood_roof".into(),
        "red_wood_roof".into(),
    ];

    // Step 2: Get city blocks from urban area
    let (city_blocks, _off_limits) = get_city_blocks_and_off_limits(&mut editor, &mut rng.derive());

    let pitches = [RoofStyle::Gable(GablePitch::Slab), RoofStyle::Gable(GablePitch::Stairs), RoofStyle::Gable(GablePitch::Double)];
    let size_classes = [
        SizeClass::Cottage, SizeClass::Cottage, SizeClass::Cottage,
        SizeClass::House, SizeClass::House,
        SizeClass::Hall,
    ];

    let mut total_buildings = 0;

    // Step 3: For each city block, create a Plot and fill with buildings_v2
    for (block_idx, block) in city_blocks.iter().enumerate() {
        let (_outer, inner) = get_outer_and_inner_points(block, 3);
        if inner.is_empty() {
            continue;
        }

        // Convert inner HashSet<Point2D> to a Plot
        let min_x = inner.iter().map(|p| p.x).min().unwrap();
        let min_z = inner.iter().map(|p| p.y).min().unwrap();
        let max_x = inner.iter().map(|p| p.x).max().unwrap();
        let max_z = inner.iter().map(|p| p.y).max().unwrap();
        let bounds = Rect2D::from_points(Point2D::new(min_x, min_z), Point2D::new(max_x, max_z));
        let w = (max_x - min_x + 1) as usize;
        let h = (max_z - min_z + 1) as usize;
        let mut usable = vec![vec![false; h]; w];
        for p in &inner {
            let lx = (p.x - min_x) as usize;
            let lz = (p.y - min_z) as usize;
            usable[lx][lz] = true;
        }
        let mut plot = Plot::new(bounds, usable);

        // Fill the plot with as many buildings as we can
        let mut block_buildings = 0;
        for attempt in 0..50 {
            let size_class = size_classes[(total_buildings + attempt) % size_classes.len()];
            let footprint = match generate_footprint(&mut rng, &plot, &size_class) {
                Some(f) => f,
                None => break,
            };

            // Mark footprint + 1-cell buffer as used
            let plot_min = plot.bounds.min();
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

            let roof_idx = rng.rand_i32_range(0, roof_ids.len() as i32) as usize;
            let roof_palette = data.palettes.get(&roof_ids[roof_idx]).expect("Roof palette not found");
            let palette = base_palette.clone().merged_with(roof_palette);

            let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
            let pitch = pitches[total_buildings % pitches.len()];
            let bctx = BuildingContext::new(Culture::Medieval, size_class, pitch);
            let _house = build_house(&mut ctx, footprint, &bctx, bounds)
                .await
                .expect("build_house failed");

            total_buildings += 1;
            block_buildings += 1;
        }

        println!(
            "Block {}: {} inner points, {} buildings placed",
            block_idx, inner.len(), block_buildings,
        );
    }

    editor.flush_buffer().await;
    println!(
        "Done — {} total buildings across {} city blocks",
        total_buildings, city_blocks.len(),
    );
}
