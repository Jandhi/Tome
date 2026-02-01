use crate::geometry::Point2D;

/// A closed polygon defining the building's base shape in the XZ plane.
/// Vertices are ordered clockwise when viewed from above (positive Y looking down).
#[derive(Debug, Clone)]
pub struct Footprint {
    /// Ordered vertices forming a closed polygon.
    /// The last vertex implicitly connects back to the first.
    pub vertices: Vec<Point2D>,
}

impl Footprint {
    /// Create a new footprint from a list of vertices.
    pub fn new(vertices: Vec<Point2D>) -> Self {
        Self { vertices }
    }

    /// Create a rectangular footprint.
    /// Origin is the minimum corner (smallest x and y/z values).
    /// Width extends along the X axis, depth along the Z axis (Point2D.y).
    /// Vertices are ordered clockwise when viewed from above.
    pub fn rectangle(origin: Point2D, width: i32, depth: i32) -> Self {
        Self {
            vertices: vec![
                origin,                                              // SW (min corner)
                Point2D::new(origin.x + width, origin.y),            // SE
                Point2D::new(origin.x + width, origin.y + depth),    // NE
                Point2D::new(origin.x, origin.y + depth),            // NW
            ],
        }
    }

    /// Get the edges of this footprint as pairs of (start, end) vertices.
    /// The last edge connects the final vertex back to the first.
    pub fn edges(&self) -> Vec<(Point2D, Point2D)> {
        if self.vertices.is_empty() {
            return vec![];
        }

        let mut edges = Vec::with_capacity(self.vertices.len());
        for i in 0..self.vertices.len() {
            let start = self.vertices[i];
            let end = self.vertices[(i + 1) % self.vertices.len()];
            edges.push((start, end));
        }
        edges
    }

    /// Check if a point lies inside this polygon using the ray casting algorithm.
    pub fn contains(&self, point: Point2D) -> bool {
        if self.vertices.len() < 3 {
            return false;
        }

        let mut inside = false;
        let n = self.vertices.len();

        for i in 0..n {
            let j = (i + 1) % n;
            let vi = self.vertices[i];
            let vj = self.vertices[j];

            // Ray casting: count intersections with edges
            if ((vi.y > point.y) != (vj.y > point.y))
                && (point.x < (vj.x - vi.x) * (point.y - vi.y) / (vj.y - vi.y) + vi.x)
            {
                inside = !inside;
            }
        }

        inside
    }

    /// Calculate the area of the polygon using the shoelace formula.
    /// Returns the absolute area (always positive).
    pub fn area(&self) -> i32 {
        if self.vertices.len() < 3 {
            return 0;
        }

        let mut sum = 0i64;
        let n = self.vertices.len();

        for i in 0..n {
            let j = (i + 1) % n;
            let vi = self.vertices[i];
            let vj = self.vertices[j];
            sum += (vi.x as i64) * (vj.y as i64);
            sum -= (vj.x as i64) * (vi.y as i64);
        }

        (sum.abs() / 2) as i32
    }

    /// Get the axis-aligned bounding box as (min, max) corners.
    pub fn bounds(&self) -> Option<(Point2D, Point2D)> {
        if self.vertices.is_empty() {
            return None;
        }

        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;

        for v in &self.vertices {
            min_x = min_x.min(v.x);
            min_y = min_y.min(v.y);
            max_x = max_x.max(v.x);
            max_y = max_y.max(v.y);
        }

        Some((Point2D::new(min_x, min_y), Point2D::new(max_x, max_y)))
    }

    /// Get the number of vertices (and edges) in this footprint.
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rectangle_vertices() {
        let fp = Footprint::rectangle(Point2D::new(0, 0), 10, 5);
        assert_eq!(fp.vertices.len(), 4);
        assert_eq!(fp.vertices[0], Point2D::new(0, 0));
        assert_eq!(fp.vertices[1], Point2D::new(10, 0));
        assert_eq!(fp.vertices[2], Point2D::new(10, 5));
        assert_eq!(fp.vertices[3], Point2D::new(0, 5));
    }

    #[test]
    fn test_edges() {
        let fp = Footprint::rectangle(Point2D::new(0, 0), 10, 5);
        let edges = fp.edges();
        assert_eq!(edges.len(), 4);
        assert_eq!(edges[0], (Point2D::new(0, 0), Point2D::new(10, 0)));
        assert_eq!(edges[1], (Point2D::new(10, 0), Point2D::new(10, 5)));
        assert_eq!(edges[2], (Point2D::new(10, 5), Point2D::new(0, 5)));
        assert_eq!(edges[3], (Point2D::new(0, 5), Point2D::new(0, 0)));
    }

    #[test]
    fn test_rectangle_area() {
        let fp = Footprint::rectangle(Point2D::new(0, 0), 10, 5);
        assert_eq!(fp.area(), 50);
    }

    #[test]
    fn test_contains() {
        let fp = Footprint::rectangle(Point2D::new(0, 0), 10, 10);
        assert!(fp.contains(Point2D::new(5, 5)));
        assert!(!fp.contains(Point2D::new(15, 5)));
        assert!(!fp.contains(Point2D::new(-1, 5)));
    }

    #[test]
    fn test_bounds() {
        let fp = Footprint::rectangle(Point2D::new(5, 10), 20, 15);
        let (min, max) = fp.bounds().unwrap();
        assert_eq!(min, Point2D::new(5, 10));
        assert_eq!(max, Point2D::new(25, 25));
    }
}
