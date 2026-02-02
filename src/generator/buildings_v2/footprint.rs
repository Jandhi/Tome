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
    /// Width is the number of blocks along the X axis, depth along the Z axis.
    /// Vertices are ordered clockwise when viewed from above.
    pub fn rectangle(origin: Point2D, width: i32, depth: i32) -> Self {
        // Use width-1 and depth-1 so that width/depth represent block counts,
        // not coordinate offsets. A width of 6 should produce 6 block positions.
        Self {
            vertices: vec![
                origin,                                                  // SW (min corner)
                Point2D::new(origin.x + width - 1, origin.y),            // SE
                Point2D::new(origin.x + width - 1, origin.y + depth - 1),// NE
                Point2D::new(origin.x, origin.y + depth - 1),            // NW
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

    /// Check if a point is strictly inside the bounding box (not on edge).
    fn strictly_inside_bounds(&self, point: Point2D) -> bool {
        let Some((min, max)) = self.bounds() else {
            return false;
        };
        point.x > min.x && point.x < max.x && point.y > min.y && point.y < max.y
    }

    /// Calculate the minimum distance from a point to any edge of this polygon.
    /// Returns 0 if the point is on an edge, positive if outside, negative if inside.
    /// For roof calculations, we care about distance to nearest edge from inside.
    pub fn distance_to_edge(&self, point: Point2D) -> i32 {
        if self.vertices.len() < 3 {
            return i32::MAX;
        }

        let mut min_dist = i32::MAX;

        for (start, end) in self.edges() {
            let dist = point_to_segment_distance(point, start, end);
            min_dist = min_dist.min(dist);
        }

        min_dist
    }

    /// Check if a point is within `distance` blocks of the footprint.
    /// Returns true if the point is inside the footprint or within distance of any edge.
    pub fn is_within_distance(&self, point: Point2D, distance: i32) -> bool {
        // Quick bounding box check first
        let Some((min, max)) = self.bounds() else {
            return false;
        };
        if point.x < min.x - distance || point.x > max.x + distance
            || point.y < min.y - distance || point.y > max.y + distance
        {
            return false;
        }

        // If inside the polygon, definitely within distance
        if self.contains(point) {
            return true;
        }

        // Check distance to edges
        self.distance_to_edge(point) <= distance
    }

    /// Compute the outer edges of the union of two axis-aligned rectangular footprints.
    /// Returns edges as (start, end) pairs ordered clockwise around the combined shape.
    pub fn outer_edges_with(&self, other: &Footprint) -> Vec<(Point2D, Point2D)> {
        let Some((min_a, max_a)) = self.bounds() else {
            return other.edges();
        };
        let Some((min_b, max_b)) = other.bounds() else {
            return self.edges();
        };

        // Collect all candidate boundary vertices
        let mut boundary_points: Vec<Point2D> = Vec::new();

        // Add corners from self that aren't strictly inside other
        for v in &self.vertices {
            if !other.strictly_inside_bounds(*v) {
                boundary_points.push(*v);
            }
        }

        // Add corners from other that aren't strictly inside self
        for v in &other.vertices {
            if !self.strictly_inside_bounds(*v) {
                boundary_points.push(*v);
            }
        }

        // Find edge intersection points
        // For axis-aligned rectangles: horizontal edges cross vertical edges
        let h_edges_a = [(min_a, Point2D::new(max_a.x, min_a.y)),
                         (Point2D::new(min_a.x, max_a.y), max_a)];
        let v_edges_a = [(Point2D::new(max_a.x, min_a.y), max_a),
                         (min_a, Point2D::new(min_a.x, max_a.y))];
        let h_edges_b = [(min_b, Point2D::new(max_b.x, min_b.y)),
                         (Point2D::new(min_b.x, max_b.y), max_b)];
        let v_edges_b = [(Point2D::new(max_b.x, min_b.y), max_b),
                         (min_b, Point2D::new(min_b.x, max_b.y))];

        // Check A's horizontal edges against B's vertical edges
        for (h_start, h_end) in &h_edges_a {
            for (v_start, v_end) in &v_edges_b {
                if let Some(p) = edge_intersection(*h_start, *h_end, *v_start, *v_end) {
                    boundary_points.push(p);
                }
            }
        }

        // Check B's horizontal edges against A's vertical edges
        for (h_start, h_end) in &h_edges_b {
            for (v_start, v_end) in &v_edges_a {
                if let Some(p) = edge_intersection(*h_start, *h_end, *v_start, *v_end) {
                    boundary_points.push(p);
                }
            }
        }

        // Remove duplicates
        boundary_points.sort_by(|a, b| a.x.cmp(&b.x).then(a.y.cmp(&b.y)));
        boundary_points.dedup();

        if boundary_points.len() < 3 {
            return self.edges();
        }

        // Sort clockwise around centroid
        let centroid_x: i32 = boundary_points.iter().map(|p| p.x).sum::<i32>()
                              / boundary_points.len() as i32;
        let centroid_y: i32 = boundary_points.iter().map(|p| p.y).sum::<i32>()
                              / boundary_points.len() as i32;

        boundary_points.sort_by(|a, b| {
            let angle_a = ((a.y - centroid_y) as f64).atan2((a.x - centroid_x) as f64);
            let angle_b = ((b.y - centroid_y) as f64).atan2((b.x - centroid_x) as f64);
            angle_a.partial_cmp(&angle_b).unwrap()
        });

        // Generate edges between consecutive vertices
        let mut edges = Vec::with_capacity(boundary_points.len());
        for i in 0..boundary_points.len() {
            let start = boundary_points[i];
            let end = boundary_points[(i + 1) % boundary_points.len()];
            edges.push((start, end));
        }

        edges
    }
}

/// Calculate the distance from a point to a line segment.
/// For axis-aligned segments (common in our footprints), this is simplified.
fn point_to_segment_distance(point: Point2D, seg_start: Point2D, seg_end: Point2D) -> i32 {
    let dx = seg_end.x - seg_start.x;
    let dy = seg_end.y - seg_start.y;

    if dx == 0 && dy == 0 {
        // Segment is a point
        let px = (point.x - seg_start.x).abs();
        let py = (point.y - seg_start.y).abs();
        return px.max(py); // Chebyshev distance for grid
    }

    // For axis-aligned segments (which is most of our cases)
    if dx == 0 {
        // Vertical segment
        let min_y = seg_start.y.min(seg_end.y);
        let max_y = seg_start.y.max(seg_end.y);
        if point.y >= min_y && point.y <= max_y {
            // Point is alongside the segment
            return (point.x - seg_start.x).abs();
        } else {
            // Point is beyond the segment ends
            let dist_to_start = (point.x - seg_start.x).abs().max((point.y - seg_start.y).abs());
            let dist_to_end = (point.x - seg_end.x).abs().max((point.y - seg_end.y).abs());
            return dist_to_start.min(dist_to_end);
        }
    }

    if dy == 0 {
        // Horizontal segment
        let min_x = seg_start.x.min(seg_end.x);
        let max_x = seg_start.x.max(seg_end.x);
        if point.x >= min_x && point.x <= max_x {
            // Point is alongside the segment
            return (point.y - seg_start.y).abs();
        } else {
            // Point is beyond the segment ends
            let dist_to_start = (point.x - seg_start.x).abs().max((point.y - seg_start.y).abs());
            let dist_to_end = (point.x - seg_end.x).abs().max((point.y - seg_end.y).abs());
            return dist_to_start.min(dist_to_end);
        }
    }

    // For non-axis-aligned segments, use general formula
    // Project point onto line and clamp to segment
    let len_sq = (dx * dx + dy * dy) as f64;
    let t = (((point.x - seg_start.x) * dx + (point.y - seg_start.y) * dy) as f64 / len_sq)
        .clamp(0.0, 1.0);

    let proj_x = seg_start.x as f64 + t * dx as f64;
    let proj_y = seg_start.y as f64 + t * dy as f64;

    let dist_x = (point.x as f64 - proj_x).abs();
    let dist_y = (point.y as f64 - proj_y).abs();

    // Return Chebyshev distance (max of x and y) for grid-based calculations
    dist_x.max(dist_y).round() as i32
}

/// Find where a horizontal segment crosses a vertical segment.
/// Returns None if they don't cross or only touch at endpoints.
fn edge_intersection(
    h_start: Point2D,
    h_end: Point2D,
    v_start: Point2D,
    v_end: Point2D,
) -> Option<Point2D> {
    let h_y = h_start.y;
    let v_x = v_start.x;

    let h_min_x = h_start.x.min(h_end.x);
    let h_max_x = h_start.x.max(h_end.x);
    let v_min_y = v_start.y.min(v_end.y);
    let v_max_y = v_start.y.max(v_end.y);

    // Check if they cross (strictly inside, not at endpoints)
    if v_x > h_min_x && v_x < h_max_x && h_y > v_min_y && h_y < v_max_y {
        Some(Point2D::new(v_x, h_y))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rectangle_vertices() {
        // Width=10, depth=5 should create a 10x5 block building
        let fp = Footprint::rectangle(Point2D::new(0, 0), 10, 5);
        assert_eq!(fp.vertices.len(), 4);
        assert_eq!(fp.vertices[0], Point2D::new(0, 0));
        assert_eq!(fp.vertices[1], Point2D::new(9, 0));   // width-1
        assert_eq!(fp.vertices[2], Point2D::new(9, 4));   // width-1, depth-1
        assert_eq!(fp.vertices[3], Point2D::new(0, 4));   // depth-1
    }

    #[test]
    fn test_edges() {
        let fp = Footprint::rectangle(Point2D::new(0, 0), 10, 5);
        let edges = fp.edges();
        assert_eq!(edges.len(), 4);
        assert_eq!(edges[0], (Point2D::new(0, 0), Point2D::new(9, 0)));
        assert_eq!(edges[1], (Point2D::new(9, 0), Point2D::new(9, 4)));
        assert_eq!(edges[2], (Point2D::new(9, 4), Point2D::new(0, 4)));
        assert_eq!(edges[3], (Point2D::new(0, 4), Point2D::new(0, 0)));
    }

    #[test]
    fn test_rectangle_area() {
        // Note: area() calculates the polygon area using shoelace formula,
        // which is (width-1)*(depth-1) for the vertex polygon.
        // For block count, use (bounds.max - bounds.min + 1) for each axis.
        let fp = Footprint::rectangle(Point2D::new(0, 0), 10, 5);
        assert_eq!(fp.area(), 36); // 9 * 4 polygon area
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
        assert_eq!(max, Point2D::new(24, 24)); // 5+20-1=24, 10+15-1=24
    }

    #[test]
    fn test_outer_edges_l_shape() {
        // Two overlapping rectangles forming an L-shape:
        //     +----+
        //     | B  |
        // +---+--+ |
        // | A    | |
        // +------+-+
        let a = Footprint::rectangle(Point2D::new(0, 0), 8, 5);  // 0-7 x, 0-4 y
        let b = Footprint::rectangle(Point2D::new(4, 2), 5, 6);  // 4-8 x, 2-7 y

        let edges = a.outer_edges_with(&b);

        // Should have 8 vertices for an L-shape (6 corners + 2 intersection points)
        assert_eq!(edges.len(), 8);

        // Verify we have all expected boundary points
        let points: Vec<Point2D> = edges.iter().map(|(s, _)| *s).collect();
        assert!(points.contains(&Point2D::new(0, 0)));  // A corner
        assert!(points.contains(&Point2D::new(7, 0)));  // A corner
        assert!(points.contains(&Point2D::new(8, 7)));  // B corner
        assert!(points.contains(&Point2D::new(4, 7)));  // B corner
        assert!(points.contains(&Point2D::new(0, 4)));  // A corner
        // Intersection points
        assert!(points.contains(&Point2D::new(4, 4)));  // where A top meets B left
        assert!(points.contains(&Point2D::new(7, 2)));  // where A right meets B bottom
    }

    #[test]
    fn test_outer_edges_cross_shape() {
        // Two rectangles forming a + shape:
        //     +--+
        //     |  |
        // +---+--+---+
        // |          |
        // +---+--+---+
        //     |  |
        //     +--+
        let horiz = Footprint::rectangle(Point2D::new(0, 3), 10, 4);  // wide
        let vert = Footprint::rectangle(Point2D::new(3, 0), 4, 10);   // tall

        let edges = horiz.outer_edges_with(&vert);

        // Cross shape has 12 vertices (4 intersection points + 8 remaining corners)
        assert_eq!(edges.len(), 12);
    }
}
