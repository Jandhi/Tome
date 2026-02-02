use crate::geometry::{Point2D, Point3D};
use super::footprint::Footprint;
use super::wall::WallSegment;

/// The structural skeleton of a building.
/// Combines a 2D footprint with vertical extent information.
#[derive(Debug, Clone)]
pub struct Frame {
    /// The 2D polygon defining the building's base shape.
    pub footprint: Footprint,
    /// Y coordinate of the ground level (bottom of first floor).
    pub base_y: i32,
    /// Height of each floor's walls in blocks.
    pub wall_height: i32,
    /// Number of floors in the building.
    pub floors: u32,
    /// Wall segments with their openings (doors, windows).
    pub walls: Vec<WallSegment>,
}

impl Frame {
    /// Create a new frame from a footprint and vertical parameters.
    pub fn new(footprint: Footprint, base_y: i32, wall_height: i32, floors: u32) -> Self {
        let walls = footprint
            .edges()
            .into_iter()
            .map(|(start, end)| WallSegment::new(start, end))
            .collect();
        Self {
            footprint,
            base_y,
            wall_height,
            floors,
            walls,
        }
    }

    /// Create a simple rectangular frame.
    pub fn rectangle(
        origin: Point3D,
        width: i32,
        depth: i32,
        wall_height: i32,
        floors: u32,
    ) -> Self {
        let footprint = Footprint::rectangle(origin.drop_y(), width, depth);
        let walls = footprint
            .edges()
            .into_iter()
            .map(|(start, end)| WallSegment::new(start, end))
            .collect();
        Self {
            footprint,
            base_y: origin.y,
            wall_height,
            floors,
            walls,
        }
    }

    /// Get the Y coordinate for the floor surface of a given floor (0-indexed).
    pub fn floor_y(&self, floor: u32) -> i32 {
        self.base_y + (floor as i32 * self.wall_height)
    }

    /// Get the Y coordinate for the ceiling of a given floor (0-indexed).
    pub fn ceiling_y(&self, floor: u32) -> i32 {
        self.floor_y(floor) + self.wall_height
    }

    /// Get the total height of the building (all floors).
    pub fn total_height(&self) -> i32 {
        self.floors as i32 * self.wall_height
    }

    /// Get all corner positions as 3D points at a specific Y level.
    pub fn corners_at_y(&self, y: i32) -> Vec<Point3D> {
        self.footprint
            .vertices
            .iter()
            .map(|v| v.add_y(y))
            .collect()
    }

    /// Get corner positions at the base of a given floor.
    pub fn corners_at_floor(&self, floor: u32) -> Vec<Point3D> {
        self.corners_at_y(self.floor_y(floor))
    }

    /// Get corner positions at the top of the building (roof attachment point).
    pub fn corners_at_top(&self) -> Vec<Point3D> {
        self.corners_at_y(self.base_y + self.total_height())
    }

    /// Get the wall segments (with their openings).
    pub fn wall_segments(&self) -> &[WallSegment] {
        &self.walls
    }

    /// Get mutable access to wall segments for adding openings.
    pub fn wall_segments_mut(&mut self) -> &mut [WallSegment] {
        &mut self.walls
    }

    /// Get the 3D bounding box as (min, max) corners.
    /// Returns None if the footprint is empty.
    pub fn bounds(&self) -> Option<(Point3D, Point3D)> {
        let (min_2d, max_2d) = self.footprint.bounds()?;
        let min_3d = min_2d.add_y(self.base_y);
        let max_3d = max_2d.add_y(self.base_y + self.total_height());
        Some((min_3d, max_3d))
    }

    /// Check if a 2D point is inside the footprint.
    pub fn contains(&self, point: Point2D) -> bool {
        self.footprint.contains(point)
    }

    /// Get the footprint area in blocks.
    pub fn area(&self) -> i32 {
        self.footprint.area()
    }

    /// Get the Y coordinate where the roof should start (top of the highest floor's walls).
    pub fn roof_base_y(&self) -> i32 {
        self.base_y + (self.wall_height * self.floors as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rectangle_frame() {
        let frame = Frame::rectangle(Point3D::new(0, 64, 0), 10, 8, 4, 2);

        assert_eq!(frame.base_y, 64);
        assert_eq!(frame.wall_height, 4);
        assert_eq!(frame.floors, 2);
        assert_eq!(frame.total_height(), 8);
    }

    #[test]
    fn test_floor_y() {
        let frame = Frame::rectangle(Point3D::new(0, 64, 0), 10, 8, 5, 3);

        assert_eq!(frame.floor_y(0), 64);
        assert_eq!(frame.floor_y(1), 69);
        assert_eq!(frame.floor_y(2), 74);
    }

    #[test]
    fn test_corners_at_floor() {
        let frame = Frame::rectangle(Point3D::new(0, 64, 0), 10, 5, 4, 1);
        let corners = frame.corners_at_floor(0);

        assert_eq!(corners.len(), 4);
        assert_eq!(corners[0], Point3D::new(0, 64, 0));
        assert_eq!(corners[1], Point3D::new(9, 64, 0));  // width-1
        assert_eq!(corners[2], Point3D::new(9, 64, 4));  // width-1, depth-1
        assert_eq!(corners[3], Point3D::new(0, 64, 4));  // depth-1
    }

    #[test]
    fn test_wall_segments() {
        let frame = Frame::rectangle(Point3D::new(0, 64, 0), 10, 5, 4, 1);
        let segments = frame.wall_segments();

        assert_eq!(segments.len(), 4);
        assert_eq!(segments[0].length(), 9);  // width-1 (distance between corners)
        assert_eq!(segments[1].length(), 4);  // depth-1
    }

    #[test]
    fn test_bounds() {
        let frame = Frame::rectangle(Point3D::new(5, 64, 10), 20, 15, 4, 2);
        let (min, max) = frame.bounds().unwrap();

        assert_eq!(min, Point3D::new(5, 64, 10));
        assert_eq!(max, Point3D::new(24, 72, 24)); // 5+20-1=24, 10+15-1=24
    }
}
