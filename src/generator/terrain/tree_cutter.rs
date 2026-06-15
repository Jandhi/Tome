use std::collections::HashSet;

use crate::{
    editor::Editor, geometry::{Point2D, Point3D, ALL_8}
};

/// A single tree: its canonical trunk cell (where a stump is placed) and every
/// (x,z) column the tree occupies — trunk plus canopy footprint.
pub struct TreeGroup {
    pub trunk: Point2D,
    pub cells: HashSet<Point2D>,
}

/// Groups the tree-topped cells in `cells` into individual trees.
///
/// `is_tree()` (and motion-blocking height) treat every leaf column as "tree",
/// so a single tree's wide canopy looks like many trees. This instead anchors on
/// *trunks*: a cell is a trunk where the first block above the tree-ignoring
/// ground is a log. Adjacent trunk columns are merged 8-connected so wide trunks
/// (2×2 dark oak, large jungle) count as one tree. Each remaining canopy cell is
/// then assigned to the nearest trunk within `MAX_CANOPY_RADIUS`, grouping a
/// tree's spreading leaves with its stem. Canopy cells with no trunk in range are
/// dropped (stray leaves / trees rooted outside `cells`).
pub fn group_trees(cells: &HashSet<Point2D>, editor: &Editor) -> Vec<TreeGroup> {
    /// Max horizontal reach (blocks) of a canopy from its trunk.
    const MAX_CANOPY_RADIUS: i32 = 6;

    let world = editor.world();

    // 1. Trunk base cells: first block above the ground (ignoring canopy) is a log.
    let trunk_cells: HashSet<Point2D> = cells
        .iter()
        .copied()
        .filter(|&p| {
            let base_y = world.get_non_tree_height(p);
            editor.get_block(p.add_y(base_y)).id.is_log()
        })
        .collect();

    // 2. Merge adjacent trunk columns into trunk groups (8-connected flood fill).
    let mut visited: HashSet<Point2D> = HashSet::new();
    let mut trees: Vec<TreeGroup> = Vec::new();
    for &start in &trunk_cells {
        if !visited.insert(start) {
            continue;
        }
        let mut group: HashSet<Point2D> = HashSet::new();
        let mut stack = vec![start];
        while let Some(p) = stack.pop() {
            group.insert(p);
            for d in ALL_8 {
                let n = p + d;
                if trunk_cells.contains(&n) && visited.insert(n) {
                    stack.push(n);
                }
            }
        }
        // Canonical trunk: smallest (x, then z) — a deterministic top-left cell.
        let trunk = *group.iter().min_by_key(|p| (p.x, p.y)).unwrap();
        trees.push(TreeGroup { trunk, cells: group });
    }

    if trees.is_empty() {
        return trees;
    }

    // 3. Assign each canopy cell to the nearest trunk within range.
    let max_sq = MAX_CANOPY_RADIUS * MAX_CANOPY_RADIUS;
    for &p in cells {
        if trunk_cells.contains(&p) {
            continue; // already owned by its trunk group
        }
        let top_y = world.get_motion_blocking_height_at(p) - 1;
        if !editor.get_block(p.add_y(top_y)).id.is_tree() {
            continue; // not a canopy cell
        }
        if let Some(tree) = trees
            .iter_mut()
            .filter(|t| p.distance_squared(&t.trunk) <= max_sq)
            .min_by_key(|t| p.distance_squared(&t.trunk))
        {
            tree.cells.insert(p);
        }
    }

    trees
}

pub async fn log_stems(editor: &Editor, points: HashSet<Point2D>) {
    for point in points {
        let height = editor.world().get_height_at(point) - 1; // checking ground
        let mut block_id = editor.get_block(Point3D::new(point.x, height, point.y)).id;

        if !block_id.is_tree() {
            continue;
        }
        editor.place_block(&"air".into(), Point3D::new(point.x, height, point.y)).await;

        for y in 1..40 {
            block_id = editor.get_block(Point3D::new(point.x, height - y, point.y)).id;
            if block_id.is_tree() {
                editor.place_block(&"air".into(), Point3D::new(point.x, height - y, point.y)).await;
            } else if block_id == "dirt".into() {
                editor.place_block(&"grass_block".into(), Point3D::new(point.x, height - y, point.y)).await;
            } else if block_id != "air".into() {
                continue;
            }
        }
    }
}

pub async fn log_trees(editor: &Editor, points: HashSet<Point2D>) {
    for point in points {
        let height = editor.world().get_motion_blocking_height_at(point) - 1; // checking ground
        let point3d = point.add_y(height);
        let mut block_id = editor.get_block(point3d).id;

        if !block_id.is_tree() {
            continue;
        }
        editor.place_block(&"air".into(), point3d).await;
        for y in 1..40 {
            block_id = editor.get_block(Point3D::new(point.x, height - y, point.y)).id;
            if block_id.is_tree() {
                editor.place_block(&"air".into(), Point3D::new(point.x, height - y, point.y)).await;
            } else if block_id == "dirt".into() {
                editor.place_block(&"grass_block".into(), Point3D::new(point.x, height - y, point.y)).await;
            } else if block_id != "air".into() {
                continue;
            }
        }
    }
}