use std::collections::HashSet;

use crate::editor::Editor;
use crate::generator::BuildClaim;
use crate::geometry::{Cardinal, Point2D};
use strum::IntoEnumIterator;

/// A contiguous chain of cells along one edge of a city block where the
/// neighbour just past the cell is road. Cells are ordered along the street.
#[derive(Debug, Clone)]
pub struct Frontage {
    pub cells: Vec<Point2D>,
    /// Direction from a frontage cell to the road. The road-facing wall of any
    /// house placed on this frontage should face this cardinal.
    pub outward: Cardinal,
}

/// Find every chain of block cells adjacent to a `BuildClaim::Path` or
/// `BuildClaim::PathPlanned`. The chain's `outward` is the cardinal direction
/// from the cell to the road. Accepting both variants means callers can claim
/// road cells as `PathPlanned` before placing houses (so foundations raise
/// terrain on them) and convert to `Path` when actually paving.
pub fn detect_frontages(block: &HashSet<Point2D>, editor: &Editor) -> Vec<Frontage> {
    let mut by_dir: [Vec<Point2D>; 4] = Default::default();
    for &cell in block {
        for (i, dir) in Cardinal::iter().enumerate() {
            let neighbour = cell + Point2D::from(dir);
            if block.contains(&neighbour) {
                continue;
            }
            if matches!(
                editor.world().get_claim(neighbour),
                Some(BuildClaim::Path(_) | BuildClaim::PathPlanned(_))
            ) {
                by_dir[i].push(cell);
            }
        }
    }

    // Step 2: per outward direction, group cells by fixed axis, then split
    // each group into contiguous runs along the varying axis.
    let mut out = Vec::new();
    for (i, outward) in Cardinal::iter().enumerate() {
        let cells = std::mem::take(&mut by_dir[i]);
        out.extend(split_into_chains(cells, outward));
    }
    out
}

/// Fallback frontage detection for blocks with no `BuildClaim::Path` neighbours
/// (interior blocks). Treats the block's outer perimeter as the frontage:
/// every cell that has at least one out-of-block neighbour contributes one
/// chain per such direction. Doors will face away from the block centre.
pub fn detect_perimeter_frontages(block: &HashSet<Point2D>) -> Vec<Frontage> {
    let mut by_dir: [Vec<Point2D>; 4] = Default::default();
    for &cell in block {
        for (i, dir) in Cardinal::iter().enumerate() {
            let neighbour = cell + Point2D::from(dir);
            if !block.contains(&neighbour) {
                by_dir[i].push(cell);
            }
        }
    }
    let mut out = Vec::new();
    for (i, outward) in Cardinal::iter().enumerate() {
        let cells = std::mem::take(&mut by_dir[i]);
        out.extend(split_into_chains(cells, outward));
    }
    out
}

fn split_into_chains(mut cells: Vec<Point2D>, outward: Cardinal) -> Vec<Frontage> {
    if cells.is_empty() {
        return Vec::new();
    }

    // For N/S outward, chain runs along x at fixed z (Point2D::y).
    // For E/W outward, chain runs along z at fixed x.
    let perp_axis_north_south = matches!(outward, Cardinal::North | Cardinal::South);
    let key = |p: &Point2D| if perp_axis_north_south { p.y } else { p.x };
    let along = |p: &Point2D| if perp_axis_north_south { p.x } else { p.y };

    cells.sort_by_key(|p| (key(p), along(p)));

    let mut chains = Vec::new();
    let mut current: Vec<Point2D> = Vec::new();
    for cell in cells {
        let start_new = match current.last() {
            None => true,
            Some(prev) => key(prev) != key(&cell) || along(&cell) - along(prev) != 1,
        };
        if start_new && !current.is_empty() {
            chains.push(Frontage { cells: std::mem::take(&mut current), outward });
        }
        current.push(cell);
    }
    if !current.is_empty() {
        chains.push(Frontage { cells: current, outward });
    }
    chains
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::paths::PathType;

    #[test]
    fn split_into_chains_groups_contiguous_runs() {
        // North-facing cells along x with one gap.
        let cells = vec![
            Point2D::new(0, 5), Point2D::new(1, 5), Point2D::new(2, 5),
            Point2D::new(4, 5), Point2D::new(5, 5),
        ];
        let chains = split_into_chains(cells, Cardinal::North);
        assert_eq!(chains.len(), 2);
        assert_eq!(chains[0].cells.len(), 3);
        assert_eq!(chains[1].cells.len(), 2);
    }

    #[test]
    fn split_into_chains_separates_by_perpendicular_axis() {
        // Two parallel rows, no gap within each row, but they're on different z.
        let cells = vec![
            Point2D::new(0, 5), Point2D::new(1, 5),
            Point2D::new(0, 7), Point2D::new(1, 7),
        ];
        let chains = split_into_chains(cells, Cardinal::North);
        assert_eq!(chains.len(), 2);
    }

    #[test]
    fn split_into_chains_east_west_uses_z_axis() {
        // East-facing column along z.
        let cells = vec![
            Point2D::new(8, 0), Point2D::new(8, 1), Point2D::new(8, 2),
        ];
        let chains = split_into_chains(cells, Cardinal::East);
        assert_eq!(chains.len(), 1);
        assert_eq!(chains[0].cells.len(), 3);
    }

    /// Builds a tiny synthetic world, claims a strip of cells as Path, then
    /// asserts the frontage detector finds the expected chain.
    #[tokio::test]
    async fn frontage_detection_picks_path_adjacent_cells() {
        use crate::editor::World;
        use crate::geometry::Rect3D;
        use crate::geometry::Point3D;

        let build_area = Rect3D::from_points(
            Point3D::new(0, 0, 0),
            Point3D::new(63, 63, 63),
        );
        let world = World::synthetic(build_area, 64);
        let mut editor = world.get_offline_editor();

        // Block: 4x4 square at (10..=13, 10..=13).
        let block: HashSet<Point2D> = (10..=13)
            .flat_map(|x| (10..=13).map(move |z| Point2D::new(x, z)))
            .collect();

        // Road just to the north of the block (z = 9).
        for x in 10..=13 {
            editor.world_mut().claim(Point2D::new(x, 9), BuildClaim::Path(PathType::Pavement));
        }

        let chains = detect_frontages(&block, &editor);

        // We should get one north-facing chain along z=10 with x=10..=13.
        let north: Vec<&Frontage> = chains.iter().filter(|f| f.outward == Cardinal::North).collect();
        assert_eq!(north.len(), 1);
        assert_eq!(north[0].cells.len(), 4);
        assert!(north[0].cells.iter().all(|p| p.y == 10));
    }

    #[test]
    fn perimeter_fallback_finds_chains_on_all_four_sides() {
        // 5×5 block with no path claims around it.
        let block: HashSet<Point2D> = (10..=14)
            .flat_map(|x| (10..=14).map(move |z| Point2D::new(x, z)))
            .collect();

        let chains = detect_perimeter_frontages(&block);

        // Expect at least one chain per cardinal direction.
        for outward in [Cardinal::North, Cardinal::East, Cardinal::South, Cardinal::West] {
            let n = chains.iter().filter(|f| f.outward == outward).count();
            assert!(n >= 1, "Expected ≥1 chain facing {:?}, got 0", outward);
        }

        // Each side should have 5 cells.
        for outward in [Cardinal::North, Cardinal::South] {
            let total: usize = chains.iter()
                .filter(|f| f.outward == outward)
                .map(|f| f.cells.len())
                .sum();
            assert_eq!(total, 5, "{:?} chains should cover 5 cells", outward);
        }
    }

    #[tokio::test]
    async fn frontage_chains_split_at_gaps() {
        use crate::editor::World;
        use crate::geometry::Rect3D;
        use crate::geometry::Point3D;

        let build_area = Rect3D::from_points(
            Point3D::new(0, 0, 0),
            Point3D::new(63, 63, 63),
        );
        let world = World::synthetic(build_area, 64);
        let mut editor = world.get_offline_editor();

        let block: HashSet<Point2D> = (10..=20)
            .flat_map(|x| (10..=13).map(move |z| Point2D::new(x, z)))
            .collect();

        // Road north of the block in two segments (x = 10..=12 and x = 15..=20)
        // with a gap at x = 13, 14 (no path claim there).
        for x in (10..=12).chain(15..=20) {
            editor.world_mut().claim(Point2D::new(x, 9), BuildClaim::Path(PathType::Pavement));
        }

        let chains = detect_frontages(&block, &editor);
        let north: Vec<&Frontage> = chains.iter().filter(|f| f.outward == Cardinal::North).collect();
        assert_eq!(north.len(), 2, "Expected two chains split by the path gap");
    }
}
