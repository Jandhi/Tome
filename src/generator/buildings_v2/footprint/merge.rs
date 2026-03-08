use crate::geometry::{Point2D, Rect2D};
use super::Footprint;
use super::generate::Layout;

/// Direction we're facing while walking the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
    Right, // +x
    Down,  // +z
    Left,  // -x
    Up,    // -z
}

impl Dir {
    fn turn_right(self) -> Self {
        match self {
            Dir::Right => Dir::Down,
            Dir::Down => Dir::Left,
            Dir::Left => Dir::Up,
            Dir::Up => Dir::Right,
        }
    }

    fn turn_left(self) -> Self {
        match self {
            Dir::Right => Dir::Up,
            Dir::Up => Dir::Left,
            Dir::Left => Dir::Down,
            Dir::Down => Dir::Right,
        }
    }

    fn step(self) -> (i32, i32) {
        match self {
            Dir::Right => (1, 0),
            Dir::Down => (0, 1),
            Dir::Left => (-1, 0),
            Dir::Up => (0, -1),
        }
    }
}

/// Rasterizes a set of rectangles into a local boolean grid.
/// Returns (grid, offset) where offset is the min corner in world coords.
/// Grid is indexed grid[x][z].
fn rasterize(rects: &[Rect2D]) -> (Vec<Vec<bool>>, Point2D) {
    let min_x = rects.iter().map(|r| r.min().x).min().unwrap();
    let min_z = rects.iter().map(|r| r.min().y).min().unwrap();
    let max_x = rects.iter().map(|r| r.max().x).max().unwrap();
    let max_z = rects.iter().map(|r| r.max().y).max().unwrap();

    let w = (max_x - min_x + 1) as usize;
    let h = (max_z - min_z + 1) as usize;
    let mut grid = vec![vec![false; h]; w];

    for rect in rects {
        for p in rect.iter() {
            let lx = (p.x - min_x) as usize;
            let lz = (p.y - min_z) as usize;
            grid[lx][lz] = true;
        }
    }

    (grid, Point2D::new(min_x, min_z))
}

/// Walks the boundary of a rasterized grid using the right-hand rule.
/// Returns clockwise-ordered vertices in local grid coordinates.
///
/// We walk along cell *edges*, not cell centers. The boundary is traced on a
/// dual grid where each node sits at a cell corner. A cell at (x, z) has
/// corners at (x, z), (x+1, z), (x+1, z+1), (x, z+1).
///
/// Starting from the top-left corner of the top-left filled cell, facing right,
/// at each step we check: is there a filled cell to our right-hand side?
///   - If we can turn right: record a vertex, turn right, step.
///   - If we can go straight: step.
///   - Otherwise: record a vertex, turn left (don't step).
fn walk_boundary(grid: &[Vec<bool>], w: usize, h: usize) -> Vec<(i32, i32)> {
    // Find top-left filled cell (scan z then x for topmost, then leftmost)
    let mut start_x = 0;
    let mut start_z = 0;
    'outer: for z in 0..h {
        for x in 0..w {
            if grid[x][z] {
                start_x = x;
                start_z = z;
                break 'outer;
            }
        }
    }

    // We start at the top-left corner of this cell, facing right.
    // Corner coordinates are on a (w+1) x (h+1) grid.
    let mut cx = start_x as i32;
    let mut cz = start_z as i32;
    let mut dir = Dir::Right;

    let mut vertices = Vec::new();
    vertices.push((cx, cz));

    // Helper: check if a cell is filled. Cell (x, z) in the grid.
    let filled = |x: i32, z: i32| -> bool {
        x >= 0 && z >= 0 && (x as usize) < w && (z as usize) < h && grid[x as usize][z as usize]
    };

    // When walking along an edge in direction `dir`, the cell to the right
    // and the cell to the left of the edge are:
    //   Right (+x): right_cell = (cx, cz), left_cell = (cx, cz-1)
    //   Down  (+z): right_cell = (cx-1, cz), left_cell = (cx, cz)
    //   Left  (-x): right_cell = (cx-1, cz-1), left_cell = (cx-1, cz)
    //   Up    (-z): right_cell = (cx, cz-1), left_cell = (cx-1, cz-1)
    // "right" means the interior side for a clockwise walk.
    let right_cell = |x: i32, z: i32, d: Dir| -> (i32, i32) {
        match d {
            Dir::Right => (x, z),
            Dir::Down => (x - 1, z),
            Dir::Left => (x - 1, z - 1),
            Dir::Up => (x, z - 1),
        }
    };

    let left_cell = |x: i32, z: i32, d: Dir| -> (i32, i32) {
        match d {
            Dir::Right => (x, z - 1),
            Dir::Down => (x, z),
            Dir::Left => (x, z),   // wait - let me think about this more carefully
            Dir::Up => (x - 1, z - 1),
        }
    };

    // Actually, let's use a simpler formulation. After stepping in direction `dir`,
    // we arrive at a new corner. We check whether to turn right, go straight, or
    // turn left by examining the cells adjacent to our new position.
    //
    // At corner (cx, cz), the four adjacent cells are:
    //   top-left:     (cx-1, cz-1)
    //   top-right:    (cx,   cz-1)
    //   bottom-left:  (cx-1, cz)
    //   bottom-right: (cx,   cz)
    //
    // When facing `dir` at corner (cx, cz):
    //   "ahead-right" cell determines if we can turn right
    //   "ahead-left" cell determines if we should go straight
    //
    // For clockwise traversal (interior on right):
    //   Facing Right: ahead-right = (cx, cz),   ahead-left = (cx, cz-1)
    //   Facing Down:  ahead-right = (cx-1, cz),  ahead-left = (cx, cz)
    //   Facing Left:  ahead-right = (cx-1, cz-1), ahead-left = (cx-1, cz)
    //   Facing Up:    ahead-right = (cx, cz-1),  ahead-left = (cx-1, cz-1)

    let ahead_right = |x: i32, z: i32, d: Dir| -> (i32, i32) {
        match d {
            Dir::Right => (x, z),
            Dir::Down => (x - 1, z),
            Dir::Left => (x - 1, z - 1),
            Dir::Up => (x, z - 1),
        }
    };

    let ahead_left = |x: i32, z: i32, d: Dir| -> (i32, i32) {
        match d {
            Dir::Right => (x, z - 1),
            Dir::Down => (x, z),
            Dir::Left => (x, z),   // Facing left: ahead-left is below-right = wait
            Dir::Up => (x - 1, z - 1),
        }
    };

    // Let me restart with a cleaner approach based on the standard contour tracing.
    // Forget the above closures - let's just do it directly.

    loop {
        let (dx, dz) = dir.step();
        let next_cx = cx + dx;
        let next_cz = cz + dz;

        // The cell to the right of the edge we're about to walk along
        let (rx, rz) = match dir {
            Dir::Right => (cx, cz),
            Dir::Down => (cx - 1, cz),
            Dir::Left => (cx - 1, cz - 1),
            Dir::Up => (cx, cz - 1),
        };

        // The cell to the left of the edge we're about to walk along
        let (lx, lz) = match dir {
            Dir::Right => (cx, cz - 1),
            Dir::Down => (cx, cz),
            Dir::Left => (cx - 1, cz),
            Dir::Up => (cx - 1, cz - 1),
        };

        let r = filled(rx, rz);
        let l = filled(lx, lz);

        if !r {
            // No cell to our right — we've gone past the shape. Turn right (convex corner).
            vertices.push((cx, cz));
            dir = dir.turn_right();
        } else if r && !l {
            // Cell to right, none to left — straight edge. Step forward.
            cx = next_cx;
            cz = next_cz;
        } else {
            // Both sides filled — concave corner. Turn left and step forward.
            vertices.push((cx, cz));
            dir = dir.turn_left();
            let (dx, dz) = dir.step();
            cx += dx;
            cz += dz;
        }

        if cx == start_x as i32 && cz == start_z as i32 && dir == Dir::Right {
            break;
        }

        // Safety limit
        if vertices.len() > (w + h) * 4 {
            break;
        }
    }

    // Remove the duplicate start vertex if present
    if vertices.len() > 1 && vertices.first() == vertices.last() {
        vertices.pop();
    }

    // Remove collinear vertices (points that don't represent a turn)
    let mut cleaned = Vec::new();
    let n = vertices.len();
    for i in 0..n {
        let prev = vertices[(i + n - 1) % n];
        let curr = vertices[i];
        let next = vertices[(i + 1) % n];
        // Keep if there's an actual turn (not collinear)
        let same_x = prev.0 == curr.0 && curr.0 == next.0;
        let same_z = prev.1 == curr.1 && curr.1 == next.1;
        if !same_x && !same_z {
            cleaned.push(curr);
        }
    }

    cleaned
}

/// Computes the clockwise outline polygon for a set of rectangles.
/// Returns vertices on the dual/corner grid in world coordinates.
/// These trace the outer boundary: for a 5-wide rect at x=0,
/// vertices span x=0 to x=5 (one past the last cell).
pub fn outline_from_rects(rects: &[Rect2D]) -> Vec<Point2D> {
    let (grid, offset) = rasterize(rects);
    let w = grid.len();
    let h = grid[0].len();
    let local_verts = walk_boundary(&grid, w, h);
    local_verts.iter()
        .map(|&(cx, cz)| Point2D::new(offset.x + cx, offset.y + cz))
        .collect()
}

/// Walks the edges of a dual-grid outline, yielding the cell positions along
/// each edge. For a clockwise polygon, cells are on the interior (right) side.
///
/// Each edge yields cell positions from start (inclusive) to end (exclusive).
/// The cell at each step is offset from the dual-grid position based on the
/// walk direction:
///   Walking +x: cell = (cx, cz)      (cell below-right)
///   Walking +z: cell = (cx-1, cz)    (cell below-left)
///   Walking -x: cell = (cx-1, cz-1)  (cell above-left)
///   Walking -z: cell = (cx, cz-1)    (cell above-right)
pub fn walk_edge_cells(start: Point2D, end: Point2D) -> Vec<Point2D> {
    let dx = (end.x - start.x).signum();
    let dz = (end.y - start.y).signum();
    let len = (end.x - start.x).abs() + (end.y - start.y).abs();

    // Offset from dual-grid corner to interior cell
    let (ox, oz) = match (dx, dz) {
        (1, 0) => (0, 0),
        (0, 1) => (-1, 0),
        (-1, 0) => (-1, -1),
        (0, -1) => (0, -1),
        _ => unreachable!(),
    };

    let mut cells = Vec::with_capacity(len as usize);
    let mut pos = start;
    for _ in 0..len {
        cells.push(Point2D::new(pos.x + ox, pos.y + oz));
        pos = Point2D::new(pos.x + dx, pos.y + dz);
    }
    cells
}

/// Returns the cell positions at concave (inner) corners of a clockwise outline.
/// At concave corners, `walk_edge_cells` leaves a gap because both adjacent edges
/// offset away from the corner. This function finds those gap cells.
pub fn concave_corner_cells(outline: &[Point2D]) -> Vec<Point2D> {
    let n = outline.len();
    if n < 3 {
        return Vec::new();
    }

    let mut cells = Vec::new();

    for i in 0..n {
        let prev = outline[(i + n - 1) % n];
        let curr = outline[i];
        let next = outline[(i + 1) % n];

        // Edge directions
        let dx1 = (curr.x - prev.x).signum();
        let dz1 = (curr.y - prev.y).signum();
        let dx2 = (next.x - curr.x).signum();
        let dz2 = (next.y - curr.y).signum();

        // Cross product: negative means concave (left turn) in a CW polygon
        let cross = dx1 * dz2 - dz1 * dx2;
        if cross >= 0 {
            continue;
        }

        // Find the gap cell. The incoming edge's walk_edge_cells uses one offset,
        // the outgoing edge uses another. The gap cell is at the vertex position
        // offset by the incoming edge's cell offset.
        let (ox, oz) = match (dx1, dz1) {
            (1, 0) => (0, 0),
            (0, 1) => (-1, 0),
            (-1, 0) => (-1, -1),
            (0, -1) => (0, -1),
            _ => continue,
        };
        cells.push(Point2D::new(curr.x + ox, curr.y + oz));
    }

    cells
}

/// Converts a Layout (core + wing rects) into a Footprint with a merged polygon outline.
pub fn merge_layout(layout: &Layout) -> Footprint {
    let rects = layout.rects();
    let vertices = outline_from_rects(&rects);
    Footprint::new(vertices, rects)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_grid(rects: &[Rect2D]) -> (Vec<Vec<bool>>, Point2D, usize, usize) {
        let (grid, offset) = rasterize(rects);
        let w = grid.len();
        let h = grid[0].len();
        (grid, offset, w, h)
    }

    /// Renders the rasterized grid with the polygon outline overlaid.
    /// '#' = filled cell, '.' = empty, '+' = vertex on corner grid,
    /// '-'/'|' = edge segment on corner grid.
    ///
    /// The output interleaves cell rows with corner rows:
    /// corner row: + - + - +
    /// cell row:   | # | . |
    /// corner row: + - + - +
    fn render_overlay(grid: &[Vec<bool>], w: usize, h: usize, verts: &[(i32, i32)]) -> String {
        // Build set of edges between consecutive vertices
        let mut corner_marks = vec![vec![' '; w + 1]; h + 1];
        let mut h_edges = vec![vec![false; w]; h + 1]; // horizontal edge at corner row z, from x to x+1
        let mut v_edges = vec![vec![false; w + 1]; h]; // vertical edge at corner col x, from z to z+1

        let n = verts.len();
        for i in 0..n {
            let (x0, z0) = verts[i];
            let (x1, z1) = verts[(i + 1) % n];
            corner_marks[z0 as usize][x0 as usize] = '+';

            if z0 == z1 {
                // Horizontal edge
                let (a, b) = if x0 < x1 { (x0, x1) } else { (x1, x0) };
                for x in a..b {
                    h_edges[z0 as usize][x as usize] = true;
                }
            } else {
                // Vertical edge
                let (a, b) = if z0 < z1 { (z0, z1) } else { (z1, z0) };
                for z in a..b {
                    v_edges[z as usize][x0 as usize] = true;
                }
            }
        }
        // Mark last vertex too
        if let Some(&(x, z)) = verts.last() {
            corner_marks[z as usize][x as usize] = '+';
        }

        let mut lines = Vec::new();

        for z in 0..=h {
            // Corner row
            let mut row = String::new();
            for x in 0..=w {
                if corner_marks[z][x] == '+' {
                    row.push('+');
                } else {
                    row.push(' ');
                }
                if x < w {
                    if h_edges[z][x] {
                        row.push('-');
                    } else {
                        row.push(' ');
                    }
                }
            }
            lines.push(row);

            // Cell row (between corner row z and z+1)
            if z < h {
                let mut row = String::new();
                for x in 0..=w {
                    if v_edges[z][x] {
                        row.push('|');
                    } else {
                        row.push(' ');
                    }
                    if x < w {
                        if grid[x][z] {
                            row.push('#');
                        } else {
                            row.push('.');
                        }
                    }
                }
                lines.push(row);
            }
        }

        lines.join("\n")
    }

    #[test]
    fn merge_single_rect() {
        let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(4, 3));
        let layout = Layout { core, wings: vec![] };
        let footprint = merge_layout(&layout);

        let (grid, _, w, h) = make_grid(&[core]);
        let local_verts = walk_boundary(&grid, w, h);

        println!("Single 5x4 rect:");
        println!("{}", render_overlay(&grid, w, h, &local_verts));
        println!("Vertices: {:?}", footprint.vertices());

        assert_eq!(footprint.vertices().len(), 4);
        assert_eq!(footprint.filled_points().len(), core.area() as usize);
    }

    #[test]
    fn merge_l_shape() {
        let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(6, 4));
        let wing = Rect2D::from_points(Point2D::new(7, 2), Point2D::new(9, 4));
        let layout = Layout { core, wings: vec![wing] };
        let footprint = merge_layout(&layout);

        let rects = layout.rects();
        let (grid, _, w, h) = make_grid(&rects);
        let local_verts = walk_boundary(&grid, w, h);

        println!("L-shape (7x5 core + 3x3 wing on right):");
        println!("{}", render_overlay(&grid, w, h, &local_verts));
        println!("Vertices: {:?}", footprint.vertices());

        assert_eq!(footprint.vertices().len(), 6);
        assert_eq!(footprint.filled_points().len(), (core.area() + wing.area()) as usize);
    }

    #[test]
    fn merge_t_shape() {
        let core = Rect2D::from_points(Point2D::new(0, 2), Point2D::new(8, 5));
        let wing = Rect2D::from_points(Point2D::new(2, 0), Point2D::new(6, 1));
        let layout = Layout { core, wings: vec![wing] };
        let footprint = merge_layout(&layout);

        let rects = layout.rects();
        let (grid, _, w, h) = make_grid(&rects);
        let local_verts = walk_boundary(&grid, w, h);

        println!("T-shape (9x4 core + 5x2 wing on top center):");
        println!("{}", render_overlay(&grid, w, h, &local_verts));
        println!("Vertices: {:?}", footprint.vertices());

        assert_eq!(footprint.vertices().len(), 8);
    }

    #[test]
    fn merge_u_shape() {
        let core = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(8, 4));
        let wing_l = Rect2D::from_points(Point2D::new(0, 5), Point2D::new(2, 7));
        let wing_r = Rect2D::from_points(Point2D::new(6, 5), Point2D::new(8, 7));
        let layout = Layout { core, wings: vec![wing_l, wing_r] };
        let footprint = merge_layout(&layout);

        let rects = layout.rects();
        let (grid, _, w, h) = make_grid(&rects);
        let local_verts = walk_boundary(&grid, w, h);

        println!("U-shape (9x5 core + two 3x3 wings below):");
        println!("{}", render_overlay(&grid, w, h, &local_verts));
        println!("Vertices: {:?}", footprint.vertices());

        assert_eq!(footprint.vertices().len(), 8);
    }

    #[test]
    fn merge_gallery() {
        use crate::noise::RNG;
        use super::super::{Plot, SizeClass};
        use super::super::generate::{generate_layouts, select_layout, score_layout};

        let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(19, 19));
        let plot = Plot::fully_usable(bounds);

        for (name, class) in [
            ("HOUSE", SizeClass::HOUSE),
            ("HALL", SizeClass::HALL),
            ("MANOR", SizeClass::MANOR),
        ] {
            println!("\n========== {} ==========", name);
            for seed in [1, 42, 77, 123, 256, 512] {
                let mut rng = RNG::new(seed);
                if let Some(result) = generate_layouts(&mut rng, &plot, &class, 4, 4) {
                    let mut select_rng = rng.derive();
                    if let Some(winner) = select_layout(&mut select_rng, &result.layouts, result.target_area, &result.candidate, class.min_side * class.min_side) {
                        let score = score_layout(&winner, result.target_area, &result.candidate);
                        let footprint = merge_layout(&winner);
                        let rects = winner.rects();
                        let (grid, _, w, h) = make_grid(&rects);
                        let local_verts = walk_boundary(&grid, w, h);

                        println!("\nseed={} {}x{} +{}w a={} s={:.2} verts={}",
                            seed, winner.core.length(), winner.core.width(),
                            winner.wings.len(), winner.total_area(), score,
                            footprint.vertices().len());
                        println!("{}", render_overlay(&grid, w, h, &local_verts));
                    }
                }
            }
        }
    }
}
