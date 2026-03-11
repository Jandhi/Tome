use crate::geometry::{Point2D, Rect2D};
use super::{find_boundaries, assign_roles, RoomRole};

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
