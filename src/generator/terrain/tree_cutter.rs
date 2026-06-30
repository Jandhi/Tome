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
            let Some(base_y) = world.get_non_tree_height(p) else {
                return false;
            };
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
        let Some(top_y) = world.get_motion_blocking_height_at(p).map(|h| h - 1) else {
            continue; // out of bounds — not our cell
        };
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
        let Some(height) = editor.world().get_height_at(point) else {
            continue;
        };
        let height = height - 1; // checking ground
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
    /// How far below a column's canopy top we scan. Generous enough to always
    /// pass the whole tree down to the ground — even when a tall neighbouring
    /// canopy overhangs the column and pushes its motion-blocking top far above
    /// the actual tree. A fixed, smaller window was the bug that left logs
    /// floating below an overhang once their leaves were stripped.
    const MAX_SCAN_DEPTH: i32 = 100;
    /// Flood-fill neighbourhood for following branch logs out from the stem.
    /// Strictly 26-connected (radius 1): vanilla branches are continuous,
    /// directly-adjacent logs, so radius 1 follows them all — while refusing to
    /// jump a gap into a distinct neighbouring tree, which would cascade-fell a
    /// whole forest patch in the selective logging painter.
    const LOG_BRIDGE: i32 = 1;

    let mut columns: HashSet<Point2D> = points.clone();
    let mut visited_logs: HashSet<Point3D> = HashSet::new();
    let mut stack: Vec<Point3D> = Vec::new();

    // 1. Seed from every log in each input column (full-depth scan, no window).
    for &point in &points {
        let Some(top) = editor.world().get_motion_blocking_height_at(point).map(|h| h - 1) else {
            continue; // out of bounds — no column to scan
        };
        for y in (top - MAX_SCAN_DEPTH..=top).rev() {
            let pos = Point3D::new(point.x, y, point.y);
            // `try_get_block`, not `get_block`: the deep scan can run past the world
            // floor (low column, ravine), where `get_block` would panic. None means
            // out of bounds below — nothing deeper to find, so stop this column.
            let Some(block) = editor.try_get_block(pos) else { break; };
            if block.id.is_log() && visited_logs.insert(pos) {
                stack.push(pos);
            }
        }
    }

    // 2. Flood fill through logs to gather branch logs that jut beyond the seed
    //    footprint (big oaks, jungle trees), bridging small leaf gaps.
    while let Some(pos) = stack.pop() {
        columns.insert(Point2D::new(pos.x, pos.z));
        for dx in -LOG_BRIDGE..=LOG_BRIDGE {
            for dy in -LOG_BRIDGE..=LOG_BRIDGE {
                for dz in -LOG_BRIDGE..=LOG_BRIDGE {
                    if dx == 0 && dy == 0 && dz == 0 {
                        continue;
                    }
                    let n = Point3D::new(pos.x + dx, pos.y + dy, pos.z + dz);
                    // `try_get_block`: a neighbour can fall outside the build area
                    // (edge column) or below the world floor; None is simply not a log.
                    if editor.try_get_block(n).map_or(false, |b| b.id.is_log())
                        && visited_logs.insert(n)
                    {
                        stack.push(n);
                    }
                }
            }
        }
    }

    // 3. Clear each affected column from its canopy top down through the full
    //    scan depth, stripping every log + leaf and converting the trunk base's
    //    dirt back to grass. No fixed window, so a log left low under an overhang
    //    is reached.
    for &point in &columns {
        let Some(top) = editor.world().get_motion_blocking_height_at(point).map(|h| h - 1) else {
            continue; // out of bounds — not our column
        };
        if !editor.get_block(point.add_y(top)).id.is_tree() {
            continue; // column surface isn't a tree — leave it untouched
        }
        let mut lowest_cleared: Option<i32> = None;
        for y in (top - MAX_SCAN_DEPTH..=top).rev() {
            let pos = Point3D::new(point.x, y, point.y);
            // `try_get_block`: stop at the world floor rather than panicking below it.
            let Some(block) = editor.try_get_block(pos) else { break; };
            if block.id.is_tree() {
                editor.place_block(&"air".into(), pos).await;
                lowest_cleared = Some(y);
            }
        }
        // Restore grass only at the block directly beneath the lowest cleared
        // tree block (the trunk base) — never deeper, to avoid grassing buried dirt.
        if let Some(base) = lowest_cleared {
            let ground = Point3D::new(point.x, base - 1, point.y);
            if editor.try_get_block(ground).map_or(false, |b| b.id == "dirt".into()) {
                editor.place_block(&"grass_block".into(), ground).await;
            }
        }
    }

    // 4. Safety net: any flood-fill log whose column was skipped by the surface
    //    guard above (rare) still gets cleared.
    for &pos in &visited_logs {
        if editor.get_block(pos).id.is_log() {
            editor.place_block(&"air".into(), pos).await;
        }
    }
}

/// Fell **floating logs** in `points` — logs left unsupported after their tree was
/// cleared, e.g. a limb overhanging from a trunk just outside the cleared area that
/// `log_trees` didn't reach. Per column, scan up from the surface and drop any log
/// with only air between it and the ground; bottom-up tracking fells a whole
/// stranded limb in one pass. Ground-supported logs are left untouched. Returns the
/// number of blocks removed.
pub async fn clear_floating_logs(editor: &Editor, points: &HashSet<Point2D>) -> usize {
    /// How far above the surface to scan — covers an overhanging canopy.
    const SCAN_HEIGHT: i32 = 32;

    let mut removed = 0;
    for &point in points {
        let Some(ground) = editor.world().get_height_at(point) else {
            continue; // out of bounds
        };
        // Seed from the surface block (one under `ground`, the first-air height).
        let mut below_is_air = editor
            .try_get_block(Point3D::new(point.x, ground - 1, point.y))
            .map_or(true, |b| b.id == "air".into());
        for y in ground..=(ground + SCAN_HEIGHT) {
            let pos = Point3D::new(point.x, y, point.y);
            let Some(block) = editor.try_get_block(pos) else { break; };
            below_is_air = if block.id.is_log() && below_is_air {
                // Only air below — fell it, leaving any log resting on top floating.
                editor.place_block(&"air".into(), pos).await;
                removed += 1;
                true
            } else {
                block.id == "air".into()
            };
        }
    }
    removed
}