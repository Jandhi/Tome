use crate::editor::World;
use crate::generator::buildings_v2::floors::{place_floors, clear_attic_stair_headroom};
use crate::generator::buildings_v2::footprint::{Footprint, Plot, SizeClass, generate_footprint};
use crate::generator::buildings_v2::footprint::merge::outline_from_rects;
use crate::generator::buildings_v2::foundation::place_foundation;
use crate::generator::buildings_v2::frame::{Frame, generate_frame};
use crate::generator::buildings_v2::walls::{
    boundary_cell_set, build_segments, place_doors, place_frame, place_openings, place_wall_infill,
};
use crate::generator::data::LoadedData;
use crate::generator::materials::PaletteId;
use crate::geometry::{Point2D, Rect2D};
use crate::http_mod::GDMCHTTPProvider;
use crate::noise::RNG;
use crate::util::init_logger;
use super::gable::{GablePitch, RidgeAxis, gable_heightmap, pick_ridge_axis};
use super::heightmap::RoofHeightmap;
use super::place_roof;

fn make_frame(rects: Vec<Rect2D>, floor_counts: Vec<u32>) -> Frame {
    let vertices = outline_from_rects(&rects);
    let footprint = Footprint::new(vertices, rects);
    Frame::new(footprint, 64, floor_counts, 3)
}

/// Render a cross-section perpendicular to the ridge at a given ridge-axis position.
/// Shows S=stair, T=top slab (ridge/half), #=fill, .=air
fn render_cross_section(
    hm: &RoofHeightmap,
    ridge_axis: RidgeAxis,
    ridge_pos: i32,
    rect_short_min: i32,
    rect_short_max: i32,
) -> String {
    render_cross_section_with_pitch(hm, ridge_axis, ridge_pos, rect_short_min, rect_short_max, GablePitch::Stairs)
}

fn render_cross_section_with_pitch(
    hm: &RoofHeightmap,
    ridge_axis: RidgeAxis,
    ridge_pos: i32,
    rect_short_min: i32,
    rect_short_max: i32,
    pitch: GablePitch,
) -> String {
    let (short_min, short_max) = match ridge_axis {
        RidgeAxis::X => (hm.min_z(), hm.max_z()),
        RidgeAxis::Z => (hm.min_x(), hm.max_x()),
    };

    let heights: Vec<f32> = (short_min..=short_max)
        .map(|s| match ridge_axis {
            RidgeAxis::X => hm.get(ridge_pos, s),
            RidgeAxis::Z => hm.get(s, ridge_pos),
        })
        .collect();

    let max_h = heights.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let min_h = heights
        .iter()
        .cloned()
        .filter(|h| *h > f32::NEG_INFINITY)
        .fold(f32::INFINITY, f32::min);

    let y_max = max_h.ceil() as i32;
    let y_min = min_h.floor() as i32;

    let mut out = String::new();

    for y in (y_min..=y_max).rev() {
        out.push_str(&format!("  y{:>3} |", y));
        for (i, &h) in heights.iter().enumerate() {
            if h == f32::NEG_INFINITY {
                out.push('.');
                continue;
            }

            let floor_h = h.floor() as i32;
            let frac = h - h.floor();
            let short_pos = short_min + i as i32;
            let is_inside = short_pos >= rect_short_min && short_pos <= rect_short_max;

            let is_local_max = {
                let prev = if i > 0 { heights[i - 1] } else { f32::NEG_INFINITY };
                let next = if i + 1 < heights.len() {
                    heights[i + 1]
                } else {
                    f32::NEG_INFINITY
                };
                prev <= h && next <= h
            };

            if y == floor_h {
                // Surface level — determine block type
                if is_local_max {
                    out.push('T'); // ridge cap (top slab)
                } else if frac >= 0.5 - f32::EPSILON {
                    out.push('T'); // half-step (top slab)
                } else {
                    out.push('S'); // stair
                }
            } else if matches!(pitch, GablePitch::Double) && y == floor_h + 1 && !is_local_max && frac < 0.5 - f32::EPSILON {
                out.push('B'); // extra block above stair for Double pitch
            } else if y < floor_h && y >= 0 && is_inside {
                out.push('#'); // fill
            } else {
                out.push('.');
            }
        }
        out.push_str("|\n");
    }

    // Footer: position numbers
    out.push_str("        ");
    for s in short_min..=short_max {
        if s >= rect_short_min && s <= rect_short_max {
            out.push_str(&format!("{}", (s % 10).abs()));
        } else {
            out.push('^');
        }
    }
    out.push('\n');

    // Legend
    out.push_str("        ^ = overhang\n");

    out
}

/// Render top-down height values as a grid.
fn render_topdown(hm: &RoofHeightmap) -> String {
    let mut out = String::new();

    // Header
    out.push_str("     x: ");
    for x in hm.min_x()..=hm.max_x() {
        out.push_str(&format!("{:>4}", x));
    }
    out.push('\n');

    for z in hm.min_z()..=hm.max_z() {
        out.push_str(&format!("  z{:>2}: ", z));
        for x in hm.min_x()..=hm.max_x() {
            let h = hm.get(x, z);
            if h == f32::NEG_INFINITY {
                out.push_str("   .");
            } else {
                out.push_str(&format!("{:>4}", format!("{:.0}", h)));
            }
        }
        out.push('\n');
    }

    out
}

#[test]
fn gable_heightmap_values_pitch_1() {
    // 8-wide rect (x:0-7), 12-long (z:0-11), ridge along Z
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(7, 11));
    let hm = gable_heightmap(&rect, GablePitch::Stairs, RidgeAxis::Z);

    // Heights along X at any Z inside rect (all Z give same profile)
    // x: -1  0  1  2  3  4  5  6  7  8
    // h: -1  0  1  2  3  3  2  1  0 -1
    assert_eq!(hm.get(-1, 5), -1.0);
    assert_eq!(hm.get(0, 5), 0.0);
    assert_eq!(hm.get(1, 5), 1.0);
    assert_eq!(hm.get(2, 5), 2.0);
    assert_eq!(hm.get(3, 5), 3.0);
    assert_eq!(hm.get(4, 5), 3.0);
    assert_eq!(hm.get(5, 5), 2.0);
    assert_eq!(hm.get(6, 5), 1.0);
    assert_eq!(hm.get(7, 5), 0.0);
    assert_eq!(hm.get(8, 5), -1.0);

    println!("\n=== 8x12 rect, pitch 1.0, ridge along Z ===");
    println!("Cross-section at z=5:");
    print!("{}", render_cross_section(&hm, RidgeAxis::Z, 5, 0, 7));
}

#[test]
fn gable_heightmap_values_pitch_half() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(7, 11));
    let hm = gable_heightmap(&rect, GablePitch::Slab, RidgeAxis::Z);

    // x: -1   0    1    2    3    4    5    6    7    8
    // h: -0.5 0.0  0.5  1.0  1.5  1.5  1.0  0.5  0.0 -0.5
    assert_eq!(hm.get(-1, 5), -0.5);
    assert_eq!(hm.get(0, 5), 0.0);
    assert_eq!(hm.get(1, 5), 0.5);
    assert_eq!(hm.get(2, 5), 1.0);
    assert_eq!(hm.get(3, 5), 1.5);

    println!("\n=== 8x12 rect, pitch 0.5, ridge along Z ===");
    println!("Cross-section at z=5:");
    print!("{}", render_cross_section(&hm, RidgeAxis::Z, 5, 0, 7));
}

#[test]
fn gable_heightmap_values_pitch_double() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(7, 11));
    let hm = gable_heightmap(&rect, GablePitch::Double, RidgeAxis::Z);

    // x: -1  0  1  2  3  4  5  6  7  8
    // h: -2  0  2  4  6  6  4  2  0 -2
    assert_eq!(hm.get(-1, 5), -2.0);
    assert_eq!(hm.get(0, 5), 0.0);
    assert_eq!(hm.get(1, 5), 2.0);
    assert_eq!(hm.get(3, 5), 6.0);

    println!("\n=== 8x12 rect, pitch 2.0, ridge along Z ===");
    println!("Cross-section at z=5:");
    print!("{}", render_cross_section_with_pitch(&hm, RidgeAxis::Z, 5, 0, 7, GablePitch::Double));
}

#[test]
fn gable_ridge_along_x() {
    // 6-wide (x:0-5), 10-long (z:0-9). Width > length so ridge along Z.
    // But if we force ridge along X, slopes fall in Z.
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(9, 5));
    let hm = gable_heightmap(&rect, GablePitch::Stairs, RidgeAxis::X);

    // Heights along Z at x=5 (any x inside gives same profile since ridge along X)
    // z: -1  0  1  2  3  4  5  6
    // h: -1  0  1  2  2  1  0 -1
    assert_eq!(hm.get(5, -1), -1.0);
    assert_eq!(hm.get(5, 0), 0.0);
    assert_eq!(hm.get(5, 2), 2.0);
    assert_eq!(hm.get(5, 3), 2.0);
    assert_eq!(hm.get(5, 5), 0.0);
    assert_eq!(hm.get(5, 6), -1.0);

    println!("\n=== 10x6 rect, pitch 1.0, ridge along X ===");
    println!("Cross-section at x=5:");
    print!("{}", render_cross_section(&hm, RidgeAxis::X, 5, 0, 5));
}

#[test]
fn merge_l_shape_heightmaps() {
    // Core: 10x8 at (0,0)-(9,7), ridge along X (longer)
    let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(9, 7));
    // Wing: 6x4 at (10,0)-(15,3), ridge along X (longer)
    let wing = Rect2D::from_points(Point2D::new(10, 0), Point2D::new(15, 3));

    let hm_core = gable_heightmap(&core, GablePitch::Stairs, RidgeAxis::X);
    let hm_wing = gable_heightmap(&wing, GablePitch::Stairs, RidgeAxis::X);

    // Merge into combined heightmap
    let combined_min_x = hm_core.min_x().min(hm_wing.min_x());
    let combined_min_z = hm_core.min_z().min(hm_wing.min_z());
    let combined_max_x = hm_core.max_x().max(hm_wing.max_x());
    let combined_max_z = hm_core.max_z().max(hm_wing.max_z());
    let width = (combined_max_x - combined_min_x + 1) as usize;
    let depth = (combined_max_z - combined_min_z + 1) as usize;

    let mut combined = RoofHeightmap::new(combined_min_x, combined_min_z, width, depth);
    combined.merge_max(&hm_core);
    combined.merge_max(&hm_wing);

    println!("\n=== L-shape: core 10x8 + wing 6x4, both ridge along X ===");
    println!("Top-down height values:");
    print!("{}", render_topdown(&combined));

    // At the overlap zone (x=10, z=0-3), the max of core and wing should apply
    // Core at x=10: overhang, h depends on z distance from core's eaves
    // Wing at x=10: inside wing, h depends on z distance from wing's eaves
    // Wing's z range is 0-3, half-width is 1. So wing height at z=1 = 1.
    // Core's overhang at x=10 gives same heights as x=9 (gable overhang).
    let core_h_at_overlap = hm_core.get(10, 2);
    let wing_h_at_overlap = hm_wing.get(10, 2);
    let merged_h = combined.get(10, 2);
    println!(
        "\nAt (10,2): core={}, wing={}, merged={}",
        core_h_at_overlap, wing_h_at_overlap, merged_h
    );
    assert_eq!(merged_h, core_h_at_overlap.max(wing_h_at_overlap));
}

#[test]
fn odd_width_single_peak() {
    // 7-wide rect: should have a single peak at center
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 9));
    let hm = gable_heightmap(&rect, GablePitch::Stairs, RidgeAxis::Z);

    // x: -1  0  1  2  3  4  5  6  7
    // h: -1  0  1  2  3  2  1  0 -1
    assert_eq!(hm.get(3, 5), 3.0); // single peak
    assert_eq!(hm.get(2, 5), 2.0);
    assert_eq!(hm.get(4, 5), 2.0);

    println!("\n=== 7x10 rect (odd width), pitch 1.0, ridge along Z ===");
    println!("Cross-section at z=5:");
    print!("{}", render_cross_section(&hm, RidgeAxis::Z, 5, 0, 6));
}

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

#[tokio::test]
async fn build_full_buildings_with_roofs() {
    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette_id: PaletteId = "medieval_spruce".into();
    let palette = data.palettes.get(&palette_id).expect("Palette not found").clone();

    let world_rect = editor.world().world_rect_2d();
    let center = world_rect.midpoint();

    let plot_min = Point2D::new(center.x - 32, center.y - 32);
    let plot_max = Point2D::new(center.x + 31, center.y + 31);
    let bounds = Rect2D::from_points(plot_min, plot_max);
    let mut plot = Plot::fully_usable(bounds);

    let mut rng = RNG::new(123);
    let footprints = fill_plot(&mut rng, &mut plot, &SizeClass::Hall, 50);
    println!("Placed {} house footprints", footprints.len());

    let pitches = [GablePitch::Slab, GablePitch::Stairs, GablePitch::Double];

    for (i, footprint) in footprints.iter().enumerate() {
        let base_y = place_foundation(&mut editor, footprint, &data, &palette, &mut rng).await;

        let frame_footprint = Footprint::new(
            outline_from_rects(footprint.rects()),
            footprint.rects().to_vec(),
        );
        let frame = generate_frame(frame_footprint, base_y, &SizeClass::Hall, &mut rng);

        // Build segments and plan openings
        let mut wall_segs = build_segments(&frame);
        let footprint_area = footprint.filled_points().len() as i32;
        let bc = boundary_cell_set(footprint.rects());
        place_doors(&mut wall_segs, &bounds, footprint_area, &bc, &mut rng);


        // Roof — cycle through pitches
        let pitch = pitches[i % pitches.len()];
        let has_attic = matches!(pitch, GablePitch::Double);

        // Upper floor slabs + stairs
        let floor_plan = place_floors(&editor, &frame, &wall_segs, has_attic, &data, &palette, &mut rng).await;

        // Wall infill
        place_wall_infill(&editor, &wall_segs, &data, &palette, &mut rng).await;

        // Timber frame
        place_frame(&editor, &frame, &data, &palette, &mut rng).await;

        // Openings
        place_openings(&editor, &wall_segs, &data, &palette, &mut rng).await;

        // Roof
        place_roof(&editor, &frame, pitch, &data, &palette, &mut rng).await;

        // Re-clear headroom above attic stairs (roof blocks may have overwritten air)
        if has_attic {
            clear_attic_stair_headroom(&editor, &frame, &floor_plan).await;
        }

        println!(
            "  Building {}: base_y={}, floors={}, rects={}, pitch={:?}",
            i, base_y, frame.max_floors(), footprint.rects().len(), pitch,
        );
    }

    editor.flush_buffer().await;
    println!("Done — {} buildings with roofs", footprints.len());
}

#[tokio::test]
async fn compare_three_pitches() {
    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette_id: PaletteId = "medieval_spruce".into();
    let palette = data.palettes.get(&palette_id).expect("Palette not found").clone();

    let world_rect = editor.world().world_rect_2d();
    let center = world_rect.midpoint();

    // Generate one footprint
    let plot_min = Point2D::new(center.x - 16, center.y - 16);
    let plot_max = Point2D::new(center.x + 15, center.y + 15);
    let bounds = Rect2D::from_points(plot_min, plot_max);
    let mut plot = Plot::fully_usable(bounds);
    let mut rng = RNG::new(42);
    let footprints = fill_plot(&mut rng, &mut plot, &SizeClass::Hall, 1);
    let footprint = &footprints[0];

    let pitches = [GablePitch::Slab, GablePitch::Stairs, GablePitch::Double];
    let offsets = [0, 20, 40]; // space them apart along X

    for (pitch, x_offset) in pitches.iter().zip(offsets.iter()) {
        let mut rng = RNG::new(42); // same RNG for consistent results

        // Shift footprint rects by x_offset
        let shifted_rects: Vec<Rect2D> = footprint.rects().iter().map(|r| {
            Rect2D::from_points(
                Point2D::new(r.min().x + x_offset, r.min().y),
                Point2D::new(r.max().x + x_offset, r.max().y),
            )
        }).collect();

        let shifted_footprint = Footprint::new(
            outline_from_rects(&shifted_rects),
            shifted_rects.clone(),
        );

        let base_y = place_foundation(&mut editor, &shifted_footprint, &data, &palette, &mut rng).await;

        let frame_footprint = Footprint::new(
            outline_from_rects(&shifted_rects),
            shifted_rects.clone(),
        );
        let frame = generate_frame(frame_footprint, base_y, &SizeClass::Hall, &mut rng);

        let mut wall_segs = build_segments(&frame);
        let footprint_area = shifted_footprint.filled_points().len() as i32;
        let bc = boundary_cell_set(&shifted_rects);
        place_doors(&mut wall_segs, &bounds, footprint_area, &bc, &mut rng);


        let has_attic = matches!(pitch, GablePitch::Double);
        let floor_plan = place_floors(&editor, &frame, &wall_segs, has_attic, &data, &palette, &mut rng).await;
        place_wall_infill(&editor, &wall_segs, &data, &palette, &mut rng).await;
        place_frame(&editor, &frame, &data, &palette, &mut rng).await;
        place_openings(&editor, &wall_segs, &data, &palette, &mut rng).await;
        place_roof(&editor, &frame, *pitch, &data, &palette, &mut rng).await;
        if has_attic {
            clear_attic_stair_headroom(&editor, &frame, &floor_plan).await;
        }

        println!("  {:?} pitch at x_offset={}", pitch, x_offset);
    }

    editor.flush_buffer().await;
    println!("Done — 3 buildings with different pitches");
}
