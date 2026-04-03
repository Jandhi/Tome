mod maximal_rect;
pub mod generate;
pub mod merge;
#[cfg(test)]
mod test;

use crate::geometry::{Point2D, Rect2D};
use crate::noise::RNG;

/// A rectangular region with a mask indicating which cells are usable for building.
pub struct Plot {
    /// Bounding rectangle in world coordinates.
    pub bounds: Rect2D,
    /// 2D grid of usable cells, indexed as [x][z] relative to bounds.min().
    /// true = buildable, false = obstacle (water, tree, cliff, etc.)
    pub usable: Vec<Vec<bool>>,
}

impl Plot {
    pub fn new(bounds: Rect2D, usable: Vec<Vec<bool>>) -> Self {
        Self { bounds, usable }
    }

    /// Creates a fully usable plot from bounds.
    pub fn fully_usable(bounds: Rect2D) -> Self {
        let w = bounds.length() as usize;
        let h = bounds.width() as usize;
        Self {
            bounds,
            usable: vec![vec![true; h]; w],
        }
    }

    pub fn is_usable(&self, world_point: Point2D) -> bool {
        let min = self.bounds.min();
        let x = (world_point.x - min.x) as usize;
        let z = (world_point.y - min.y) as usize;
        x < self.usable.len()
            && z < self.usable[0].len()
            && self.usable[x][z]
    }
}

/// Determines the building's 2D shape and position within a plot.
pub struct Footprint {
    /// Clockwise-ordered vertices in world coordinates.
    /// Every edge is axis-aligned.
    vertices: Vec<Point2D>,
    /// The original rectangles (core + wings) that form this footprint.
    /// Core is always rects[0].
    rects: Vec<Rect2D>,
}

impl Footprint {
    pub fn new(vertices: Vec<Point2D>, rects: Vec<Rect2D>) -> Self {
        Self { vertices, rects }
    }

    pub fn bounds(&self) -> Rect2D {
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
        Rect2D::from_points(Point2D::new(min_x, min_y), Point2D::new(max_x, max_y))
    }

    pub fn edges(&self) -> impl Iterator<Item = (Point2D, Point2D)> + '_ {
        self.vertices.windows(2)
            .map(|w| (w[0], w[1]))
            .chain(std::iter::once((
                *self.vertices.last().unwrap(),
                self.vertices[0],
            )))
    }

    /// Test whether a point is inside the footprint.
    pub fn contains(&self, point: Point2D) -> bool {
        self.rects.iter().any(|r| r.contains(point))
    }

    /// All integer points inside the footprint.
    pub fn filled_points(&self) -> Vec<Point2D> {
        let mut points = Vec::new();
        for rect in &self.rects {
            for point in rect.iter() {
                points.push(point);
            }
        }
        points.sort_by_key(|p| (p.x, p.y));
        points.dedup();
        points
    }

    pub fn vertices(&self) -> &[Point2D] {
        &self.vertices
    }

    pub fn rects(&self) -> &[Rect2D] {
        &self.rects
    }
}

/// Size class for footprint generation, driven by building type and wealth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeClass {
    /// Small rural building on the outskirts. Simple rectangle.
    Cottage,
    /// Standard town building. L-shapes common.
    House,
    /// Larger building for craftsmen, taverns, shops. Complex shapes.
    Hall,
    /// Grand building for the elite. Largest and most complex.
    Manor,
}

impl SizeClass {
    pub fn target_area_min(&self) -> i32 {
        match self { Self::Cottage => 45, Self::House => 80, Self::Hall => 130, Self::Manor => 280 }
    }
    pub fn target_area_max(&self) -> i32 {
        match self { Self::Cottage => 80, Self::House => 130, Self::Hall => 200, Self::Manor => 450 }
    }
    pub fn min_side(&self) -> i32 {
        match self { Self::Cottage => 5, Self::House => 5, Self::Hall => 7, Self::Manor => 9 }
    }
    pub fn min_wings(&self) -> i32 {
        match self { Self::Cottage => 0, Self::House => 1, Self::Hall => 2, Self::Manor => 2 }
    }
    pub fn max_wings(&self) -> i32 {
        match self { Self::Cottage => 1, Self::House => 2, Self::Hall => 3, Self::Manor => 4 }
    }
    pub fn min_floors(&self) -> u32 {
        match self { Self::Cottage => 1, Self::House => 1, Self::Hall => 2, Self::Manor => 2 }
    }
    pub fn max_floors(&self) -> u32 {
        match self { Self::Cottage => 1, Self::House => 2, Self::Hall => 3, Self::Manor => 3 }
    }
    pub fn floor_range(&self) -> std::ops::RangeInclusive<u32> {
        self.min_floors()..=self.max_floors()
    }
}

/// A boundary between two adjacent rects where an interior wall goes.
pub struct RectBoundary {
    pub rect_a: usize,
    pub rect_b: usize,
    /// Cell positions where wall blocks are placed.
    pub wall_cells: Vec<Point2D>,
}

/// Find pairs of adjacent rects and compute the cells for each shared boundary wall.
/// The wall is placed on the inside edge of the core rect (index 0) so that
/// wings keep their full interior space. For wing-to-wing boundaries, the wall
/// goes on the lower-indexed rect's edge.
pub fn find_boundaries(rects: &[Rect2D]) -> Vec<RectBoundary> {
    let mut boundaries = Vec::new();

    for i in 0..rects.len() {
        for j in (i + 1)..rects.len() {
            let a = &rects[i];
            let b = &rects[j];

            // East: A's east side adjacent to B's west side
            if a.max().x + 1 == b.min().x {
                let z_start = a.min().y.max(b.min().y);
                let z_end = a.max().y.min(b.max().y);
                if z_start <= z_end {
                    // Wall on A's inside edge (last column of A)
                    let cells = (z_start..=z_end)
                        .map(|z| Point2D::new(a.max().x, z))
                        .collect();
                    boundaries.push(RectBoundary { rect_a: i, rect_b: j, wall_cells: cells });
                }
            }
            // West: B's east side adjacent to A's west side
            else if b.max().x + 1 == a.min().x {
                let z_start = a.min().y.max(b.min().y);
                let z_end = a.max().y.min(b.max().y);
                if z_start <= z_end {
                    // Wall on A's inside edge (first column of A)
                    let cells = (z_start..=z_end)
                        .map(|z| Point2D::new(a.min().x, z))
                        .collect();
                    boundaries.push(RectBoundary { rect_a: i, rect_b: j, wall_cells: cells });
                }
            }
            // South: A's south side adjacent to B's north side
            else if a.max().y + 1 == b.min().y {
                let x_start = a.min().x.max(b.min().x);
                let x_end = a.max().x.min(b.max().x);
                if x_start <= x_end {
                    // Wall on A's inside edge (last row of A)
                    let cells = (x_start..=x_end)
                        .map(|x| Point2D::new(x, a.max().y))
                        .collect();
                    boundaries.push(RectBoundary { rect_a: i, rect_b: j, wall_cells: cells });
                }
            }
            // North: B's south side adjacent to A's north side
            else if b.max().y + 1 == a.min().y {
                let x_start = a.min().x.max(b.min().x);
                let x_end = a.max().x.min(b.max().x);
                if x_start <= x_end {
                    // Wall on A's inside edge (first row of A)
                    let cells = (x_start..=x_end)
                        .map(|x| Point2D::new(x, a.min().y))
                        .collect();
                    boundaries.push(RectBoundary { rect_a: i, rect_b: j, wall_cells: cells });
                }
            }
        }
    }

    boundaries
}

/// Full footprint generation pipeline: generate layouts, score/select, merge into polygon.
/// Returns `None` if no valid building fits the plot.
pub fn generate_footprint(rng: &mut RNG, plot: &Plot, size_class: &SizeClass) -> Option<Footprint> {
    let result = generate::generate_layouts(rng, plot, size_class, 5, 4)?;
    let mut select_rng = rng.derive();
    let min_area = size_class.min_side() * size_class.min_side();
    let winner = generate::select_layout(
        &mut select_rng, &result.layouts, result.target_area, &result.candidate, min_area,
    )?;
    let footprint = merge::merge_layout(&winner);

    debug_assert!(
        footprint.filled_points().iter().all(|p| plot.is_usable(*p)),
        "Footprint contains unusable cells"
    );

    Some(footprint)
}
