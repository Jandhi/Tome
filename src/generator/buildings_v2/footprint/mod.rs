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
#[derive(Debug, Clone, Copy)]
pub struct SizeClass {
    pub target_area_min: i32,
    pub target_area_max: i32,
    pub min_side: i32,
    pub min_wings: i32,
    pub max_wings: i32,
    pub min_floors: u32,
    pub max_floors: u32,
}

impl SizeClass {
    /// Small rural building on the outskirts. Simple rectangle.
    pub const COTTAGE: Self = Self { target_area_min: 45, target_area_max: 80, min_side: 5, min_wings: 0, max_wings: 1, min_floors: 1, max_floors: 1 };
    /// Standard town building. L-shapes common.
    pub const HOUSE: Self = Self { target_area_min: 80, target_area_max: 130, min_side: 5, min_wings: 1, max_wings: 2, min_floors: 1, max_floors: 2 };
    /// Larger building for craftsmen, taverns, shops. Complex shapes.
    pub const HALL: Self = Self { target_area_min: 130, target_area_max: 200, min_side: 7, min_wings: 2, max_wings: 3, min_floors: 2, max_floors: 3 };
    /// Grand building for the elite. Largest and most complex.
    pub const MANOR: Self = Self { target_area_min: 280, target_area_max: 450, min_side: 9, min_wings: 2, max_wings: 4, min_floors: 2, max_floors: 3 };

    pub fn floor_range(&self) -> std::ops::RangeInclusive<u32> {
        self.min_floors..=self.max_floors
    }
}

/// Full footprint generation pipeline: generate layouts, score/select, merge into polygon.
/// Returns `None` if no valid building fits the plot.
pub fn generate_footprint(rng: &mut RNG, plot: &Plot, size_class: &SizeClass) -> Option<Footprint> {
    let result = generate::generate_layouts(rng, plot, size_class, 5, 4)?;
    let mut select_rng = rng.derive();
    let min_area = size_class.min_side * size_class.min_side;
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
