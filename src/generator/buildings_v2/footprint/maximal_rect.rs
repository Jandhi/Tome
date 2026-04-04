use crate::geometry::Rect2D;

/// Finds the largest axis-aligned rectangle of `true` cells in a boolean grid.
/// Uses the histogram-based O(rows * cols) algorithm.
/// Grid is indexed as [x][z]. Returns None if no usable cell exists.
pub fn find_largest_rect(grid: &[Vec<bool>]) -> Option<Rect2D> {
    let cols = grid.len();
    if cols == 0 {
        return None;
    }
    let rows = grid[0].len();
    if rows == 0 {
        return None;
    }

    // Build height map: heights[x][z] = number of consecutive true cells
    // going upward (decreasing z) from (x, z), including (x, z) itself.
    let mut heights = vec![vec![0i32; rows]; cols];
    for x in 0..cols {
        for z in 0..rows {
            if grid[x][z] {
                heights[x][z] = if z == 0 { 1 } else { heights[x][z - 1] + 1 };
            }
        }
    }

    let mut best_area = 0;
    let mut best_rect: Option<(usize, usize, usize, usize)> = None; // (x_min, z_min, x_max, z_max)

    // For each row z, run largest-rectangle-in-histogram across all columns x.
    for z in 0..rows {
        let mut stack: Vec<usize> = Vec::new(); // stack of x indices
        let histogram: Vec<i32> = (0..cols).map(|x| heights[x][z]).collect();

        for x in 0..=cols {
            let h = if x < cols { histogram[x] } else { 0 };
            while !stack.is_empty() && histogram[*stack.last().unwrap()] > h {
                let height = histogram[stack.pop().unwrap()];
                let x_min = stack.last().map_or(0, |&s| s + 1);
                let x_max = if x == 0 { 0 } else { x - 1 };
                let width = (x_max - x_min + 1) as i32;
                let area = height * width;
                if area > best_area {
                    best_area = area;
                    let z_min = (z as i32 - height + 1) as usize;
                    best_rect = Some((x_min, z_min, x_max, z));
                }
            }
            stack.push(x);
        }
    }

    best_rect.map(|(x_min, z_min, x_max, z_max)| {
        Rect2D::from_points(
            crate::geometry::Point2D::new(x_min as i32, z_min as i32),
            crate::geometry::Point2D::new(x_max as i32, z_max as i32),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fully_usable_grid() {
        let grid = vec![vec![true; 5]; 8];
        let rect = find_largest_rect(&grid).unwrap();
        assert_eq!(rect.area(), 40);
        assert_eq!(rect.length(), 8);
        assert_eq!(rect.width(), 5);
    }

    #[test]
    fn empty_grid() {
        let grid = vec![vec![false; 5]; 5];
        assert!(find_largest_rect(&grid).is_none());
    }

    #[test]
    fn single_cell() {
        let grid = vec![vec![true]];
        let rect = find_largest_rect(&grid).unwrap();
        assert_eq!(rect.area(), 1);
    }

    #[test]
    fn obstacle_in_center() {
        // 7x7 grid with a 3x3 obstacle in the center
        let mut grid = vec![vec![true; 7]; 7];
        for x in 2..5 {
            for z in 2..5 {
                grid[x][z] = false;
            }
        }
        let rect = find_largest_rect(&grid).unwrap();
        // Largest rect should be 7x2 = 14 (top or bottom strip)
        assert_eq!(rect.area(), 14);
    }

    #[test]
    fn l_shaped_usable_area() {
        // 10x10 grid, top-right quadrant blocked
        let mut grid = vec![vec![true; 10]; 10];
        for x in 5..10 {
            for z in 0..5 {
                grid[x][z] = false;
            }
        }
        let rect = find_largest_rect(&grid).unwrap();
        // Largest should be 10x5 = 50 (bottom half) or 5x10 = 50 (left half)
        assert_eq!(rect.area(), 50);
    }
}
