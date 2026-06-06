use crate::geometry::{Point2D, Rect2D};
use crate::noise::RNG;
use super::super::footprint::{Footprint, SizeClass};
use super::super::footprint::merge::outline_from_rects;
use super::{apply_jetty, generate_frame, Frame};

fn simple_footprint(rects: Vec<Rect2D>) -> Footprint {
    let vertices = outline_from_rects(&rects);
    Footprint::new(vertices, rects)
}

#[test]
fn single_rect_floor_y() {
    let rect = Rect2D::new(Point2D::new(0, 0), Point2D::new(10, 10));
    let footprint = simple_footprint(vec![rect]);
    let frame = Frame::new(footprint, 64, vec![2], 3);

    assert_eq!(frame.floor_y(0), 64);
    assert_eq!(frame.floor_y(1), 68); // 64 + 1 * 4
    assert_eq!(frame.ceiling_y(0), 67); // 64 + 3
    assert_eq!(frame.ceiling_y(1), 71); // 68 + 3
    assert_eq!(frame.roof_y(0), 73); // 64 + 2*4 + 1 (one above top wall)
    assert_eq!(frame.max_floors(), 2);
    assert_eq!(frame.rect_height(0), 8);
}

#[test]
fn single_rect_single_floor() {
    let rect = Rect2D::new(Point2D::new(0, 0), Point2D::new(5, 5));
    let footprint = simple_footprint(vec![rect]);
    let frame = Frame::new(footprint, 100, vec![1], 3);

    assert_eq!(frame.floor_y(0), 100);
    assert_eq!(frame.ceiling_y(0), 103);
    assert_eq!(frame.roof_y(0), 105); // 100 + 1*4 + 1
    assert_eq!(frame.max_floors(), 1);
    assert_eq!(frame.floors().collect::<Vec<_>>(), vec![0]);
}

#[test]
fn multi_rect_different_heights() {
    let core = Rect2D::new(Point2D::new(0, 0), Point2D::new(10, 10));
    let wing = Rect2D::new(Point2D::new(10, 0), Point2D::new(5, 8));
    let footprint = simple_footprint(vec![core, wing]);
    let frame = Frame::new(footprint, 64, vec![3, 2], 3);

    assert_eq!(frame.max_floors(), 3);
    assert_eq!(frame.roof_y(0), 77); // 64 + 3*4 + 1
    assert_eq!(frame.roof_y(1), 73); // 64 + 2*4 + 1

    // Floor 0: both rects active
    assert_eq!(frame.active_rects(0), &[0, 1]);
    // Floor 1: both rects active
    assert_eq!(frame.active_rects(1), &[0, 1]);
    // Floor 2: only core active
    assert_eq!(frame.active_rects(2), &[0]);
}

#[test]
fn filled_points_shrinks_on_upper_floors() {
    let core = Rect2D::new(Point2D::new(0, 0), Point2D::new(3, 3));
    let wing = Rect2D::new(Point2D::new(3, 0), Point2D::new(2, 2));
    let footprint = simple_footprint(vec![core, wing]);
    let frame = Frame::new(footprint, 0, vec![2, 1], 3);

    let floor0_points = frame.filled_points_at_floor(0);
    let floor1_points = frame.filled_points_at_floor(1);

    // Floor 0 has both rects, floor 1 only core
    assert!(floor0_points.len() > floor1_points.len());
    // Floor 1 should have exactly 3*3 = 9 points (the core)
    assert_eq!(floor1_points.len(), 9);
}

#[test]
fn generate_frame_cottage_always_one_floor() {
    let rect = Rect2D::new(Point2D::new(0, 0), Point2D::new(7, 7));
    for seed in 0..20 {
        let footprint = simple_footprint(vec![rect]);
        let mut rng = RNG::new(seed as i64);
        let frame = generate_frame(footprint, 64, &SizeClass::Cottage, &mut rng);
        assert_eq!(frame.max_floors(), 1, "Cottage should always be 1 floor (seed {seed})");
    }
}

#[test]
fn generate_frame_wing_floors_bounded() {
    let core = Rect2D::new(Point2D::new(0, 0), Point2D::new(10, 10));
    let wing = Rect2D::new(Point2D::new(10, 0), Point2D::new(5, 8));
    for seed in 0..20 {
        let footprint = simple_footprint(vec![core, wing]);
        let mut rng = RNG::new(seed as i64);
        let frame = generate_frame(footprint, 64, &SizeClass::Hall, &mut rng);
        let core_floors = frame.floor_counts()[0];
        let wing_floors = frame.floor_counts()[1];
        assert!(core_floors >= 2 && core_floors <= 3);
        assert!(wing_floors >= core_floors - 1 && wing_floors <= core_floors);
    }
}

#[test]
fn apply_jetty_grows_upper_floor_single_rect_two_floors() {
    let rect = Rect2D::from_points(Point2D::new(5, 5), Point2D::new(10, 12));
    let footprint = simple_footprint(vec![rect]);
    let frame = Frame::new(footprint, 64, vec![2], 3);
    // Plot bounds with room on every side
    let plot = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(20, 20));

    let frame = apply_jetty(frame, &plot);

    let ground = frame.rect_at(0, 0).unwrap();
    let upper = frame.rect_at(0, 1).unwrap();
    assert_eq!(ground.min(), rect.min());
    assert_eq!(ground.max(), rect.max());
    assert_eq!(upper.min(), Point2D::new(4, 4));
    assert_eq!(upper.max(), Point2D::new(11, 13));

    // Filled points expand on the jettied floor.
    let f0 = frame.filled_points_at_floor(0);
    let f1 = frame.filled_points_at_floor(1);
    assert_eq!(f0.len(), rect.area() as usize);
    assert_eq!(f1.len(), upper.area() as usize);
    assert!(f1.len() > f0.len());

    // Outline at floor 1 is the grown rectangle (4 vertices, fully enclosing the ground).
    let outline1 = frame.outline_at_floor(1);
    assert_eq!(outline1.len(), 4);
}

#[test]
fn apply_jetty_noop_on_single_floor() {
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 6));
    let footprint = simple_footprint(vec![rect]);
    let frame = Frame::new(footprint, 64, vec![1], 3);
    let plot = Rect2D::from_points(Point2D::new(-10, -10), Point2D::new(20, 20));

    let frame = apply_jetty(frame, &plot);

    // Ground rect unchanged; no upper floor.
    let g = frame.rect_at(0, 0).unwrap();
    assert_eq!(g.min(), rect.min());
    assert_eq!(g.max(), rect.max());
    assert_eq!(frame.max_floors(), 1);
}

#[test]
fn apply_jetty_multi_rect_grows_only_open_sides() {
    // L-shape: core (0,0)..(6,6), wing (7,1)..(9,5) abutting the core's east
    // edge. Phase 3 grows each rect's open-air sides by 1 and keeps the shared
    // seam (core east / wing west) flush.
    let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 6));
    let wing = Rect2D::from_points(Point2D::new(7, 1), Point2D::new(9, 5));
    let footprint = simple_footprint(vec![core, wing]);
    let frame = Frame::new(footprint, 64, vec![2, 2], 3);
    let plot = Rect2D::from_points(Point2D::new(-10, -10), Point2D::new(20, 20));

    let frame = apply_jetty(frame, &plot);

    // Ground floor unchanged for both rects.
    assert_eq!(frame.rect_at(0, 0).unwrap().min(), core.min());
    assert_eq!(frame.rect_at(0, 0).unwrap().max(), core.max());
    assert_eq!(frame.rect_at(1, 0).unwrap().min(), wing.min());
    assert_eq!(frame.rect_at(1, 0).unwrap().max(), wing.max());

    // Core upper floor: west/north/south grow, east (seam) flush at x=6.
    let c1 = frame.rect_at(0, 1).unwrap();
    assert_eq!(c1.min(), Point2D::new(-1, -1));
    assert_eq!(c1.max(), Point2D::new(6, 7));

    // Wing upper floor: east/north/south grow, west (seam) flush at x=7.
    let w1 = frame.rect_at(1, 1).unwrap();
    assert_eq!(w1.min(), Point2D::new(7, 0));
    assert_eq!(w1.max(), Point2D::new(10, 6));

    // Seam stays aligned on the jettied floor: no gap, no overlap.
    assert_eq!(c1.max().x + 1, w1.min().x);
}

#[test]
fn apply_jetty_u_shape_grows_three_open_sides_each() {
    // Core (0,0)..(8,6) with a wing on its west and a wing on its east, both
    // abutting the core. Each side wing keeps its core-facing seam flush; the
    // core keeps both east and west flush and grows only north/south.
    let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 6));
    let west_wing = Rect2D::from_points(Point2D::new(-3, 1), Point2D::new(-1, 5));
    let east_wing = Rect2D::from_points(Point2D::new(9, 1), Point2D::new(11, 5));
    let footprint = simple_footprint(vec![core, west_wing, east_wing]);
    let frame = Frame::new(footprint, 64, vec![2, 2, 2], 3);
    let plot = Rect2D::from_points(Point2D::new(-10, -10), Point2D::new(20, 20));

    let frame = apply_jetty(frame, &plot);

    // Core: both x-sides flush (seams), y-sides grow.
    let c1 = frame.rect_at(0, 1).unwrap();
    assert_eq!(c1.min(), Point2D::new(0, -1));
    assert_eq!(c1.max(), Point2D::new(8, 7));

    // West wing: east (seam) flush at x=-1; west/north/south grow.
    let w1 = frame.rect_at(1, 1).unwrap();
    assert_eq!(w1.min(), Point2D::new(-4, 0));
    assert_eq!(w1.max(), Point2D::new(-1, 6));

    // East wing: west (seam) flush at x=9; east/north/south grow.
    let e1 = frame.rect_at(2, 1).unwrap();
    assert_eq!(e1.min(), Point2D::new(9, 0));
    assert_eq!(e1.max(), Point2D::new(12, 6));
}

#[test]
fn apply_jetty_one_floor_wing_stays_flush() {
    // Core has 2 floors, wing has 1. The wing has no upper floor (stays flush),
    // and the core's east side — shared with the wing on the ground — stays
    // flush on the upper floor too (no overhang over the wing's roof).
    let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 6));
    let wing = Rect2D::from_points(Point2D::new(7, 1), Point2D::new(9, 5));
    let footprint = simple_footprint(vec![core, wing]);
    let frame = Frame::new(footprint, 64, vec![2, 1], 3);
    let plot = Rect2D::from_points(Point2D::new(-10, -10), Point2D::new(20, 20));

    let frame = apply_jetty(frame, &plot);

    // Wing has no presence above the ground floor.
    assert!(frame.rect_at(1, 1).is_none());

    // Core upper floor grows on open sides but keeps the wing-facing east flush.
    let c1 = frame.rect_at(0, 1).unwrap();
    assert_eq!(c1.min(), Point2D::new(-1, -1));
    assert_eq!(c1.max(), Point2D::new(6, 7));
}

#[test]
fn apply_jetty_noop_when_multi_rect_grown_exceeds_plot() {
    // Core fits, but its grown extent would poke past the plot edge → the whole
    // building falls back to flush.
    let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 6));
    let wing = Rect2D::from_points(Point2D::new(7, 1), Point2D::new(9, 5));
    let footprint = simple_footprint(vec![core, wing]);
    let frame = Frame::new(footprint, 64, vec![2, 2], 3);
    // Plot hugs the core's north/west edge so growing core north/west overflows.
    let plot = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(20, 20));

    let frame = apply_jetty(frame, &plot);

    assert_eq!(frame.rect_at(0, 1).unwrap().min(), core.min());
    assert_eq!(frame.rect_at(0, 1).unwrap().max(), core.max());
    assert_eq!(frame.rect_at(1, 1).unwrap().min(), wing.min());
    assert_eq!(frame.rect_at(1, 1).unwrap().max(), wing.max());
}

#[test]
fn apply_jetty_noop_when_grown_exceeds_plot() {
    // Rect already touches the plot edge; growing by 1 would push outside.
    let rect = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 6));
    let footprint = simple_footprint(vec![rect]);
    let frame = Frame::new(footprint, 64, vec![2], 3);
    let plot = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 6));

    let frame = apply_jetty(frame, &plot);

    // No jetty applied — upper floor equals ground.
    let u = frame.rect_at(0, 1).unwrap();
    assert_eq!(u.min(), rect.min());
    assert_eq!(u.max(), rect.max());
}

#[test]
fn outline_at_floor_shrinks() {
    // Core 7x7 at (0,0), wing 3x5 adjacent on the east side
    let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 6));
    let wing = Rect2D::from_points(Point2D::new(7, 1), Point2D::new(9, 5));
    let footprint = simple_footprint(vec![core, wing]);
    let frame = Frame::new(footprint, 64, vec![2, 1], 3);

    let outline0 = frame.outline_at_floor(0);
    let outline1 = frame.outline_at_floor(1);

    // Floor 0 has both rects (more vertices), floor 1 is just core (4 vertices)
    assert!(outline0.len() > 4, "Floor 0 should have more than 4 vertices");
    assert_eq!(outline1.len(), 4, "Floor 1 core-only should have 4 vertices");
}
