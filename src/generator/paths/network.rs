//! Tiered A* road network for urban areas.
//!
//! Builds **arterials** (a minimum spanning tree over urban parcel centres,
//! optionally routed through a town centre) and **collectors** (each gate routed
//! to the nearest backbone node). Every edge is an A* route, so roads follow
//! terrain height. Realise the returned paths with
//! [`build_path`](super::build_path).
//!
//! Run *after* an urban flatten so A* plans over gentled terrain.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::editor::Editor;
use crate::generator::parcels::ParcelType;
use crate::generator::materials::MaterialId;
use crate::geometry::{CARDINALS_2D, Point2D, Point3D};

use super::path::{Path, PathPriority};
use super::routing::{get_path_with, RouteContext, RouteParams};

/// Build the tiered road network. Returns routed paths (arterials first, then
/// collectors); individual edges may be absent if A* failed to find a route.
///
/// `include_town_center` adds the urban-area centroid as an extra backbone node
/// so the arterial tree converges through the middle (radial feel) rather than
/// being a pure parcel-to-parcel MST.
pub async fn build_road_network(
    editor: &Editor,
    arterial_material: MaterialId,
    collector_material: MaterialId,
    include_town_center: bool,
    anchor_nodes: &[Point3D],
    blocked: &HashSet<Point2D>,
) -> Vec<Path> {
    let urban = editor.world().get_urban_points();
    if urban.is_empty() {
        return Vec::new();
    }

    // Backbone nodes, each snapped to a real cell and lifted to the
    // (post-flatten) surface height. When `anchor_nodes` are supplied (placed
    // buildings), they ARE the backbone — the arterials connect *them*, so the
    // network has a reason for its shape. With no anchors, fall back to one
    // centre per urban super-parcel (the original centroid network).
    let mut backbone: Vec<Point3D> = Vec::new();
    if include_town_center {
        if let Some(c) = centroid_snapped(&urban) {
            backbone.push(editor.world().add_height(c));
        }
    }
    backbone.extend_from_slice(anchor_nodes);
    if anchor_nodes.is_empty() {
        for sd in editor.world().districts.values() {
            if sd.data.parcel_type != ParcelType::Urban {
                continue;
            }
            if let Some(c) = centroid_snapped(&sd.data.points_2d) {
                backbone.push(editor.world().add_height(c));
            }
        }
    }

    // If a backbone node sits on a building footprint (a centroid of a placed
    // building is *inside* it), relocate it to the nearest clear urban cell. An
    // arterial can neither start nor end on a blocked cell, so a node buried in a
    // building would silently fail to route — this is what lets callers pass raw
    // footprint centroids and have the network find each building's nearest edge.
    for node in backbone.iter_mut() {
        if blocked.contains(&node.drop_y()) {
            if let Some(c) = nearest_unblocked(node.drop_y(), &urban, blocked) {
                *node = editor.world().add_height(c);
            }
        }
    }

    // Arterials want to be as straight as possible: a heavy turn penalty plus a
    // strong diagonal surcharge push them toward axis-aligned legs (an L with one
    // corner) rather than a 45° staircase, which leaves long, tidy frontage for
    // houses instead of a stepped edge. Collectors get a milder version of both.
    let arterial_params = RouteParams { turn_weight: 6, diagonal_cost: 5, ..RouteParams::default() };
    let collector_params = RouteParams { turn_weight: 3, diagonal_cost: 4, ..RouteParams::default() };

    // Distance-to-wall field: routes pay a penalty for running close to the wall
    // (ramping up as they approach), so they keep clear of it and only cross at
    // gates instead of cutting straight through. The wall sits on the urban
    // boundary — urban cells with a non-urban cardinal neighbour.
    let wall_cells: HashSet<Point2D> = urban.iter()
        .filter(|&&c| CARDINALS_2D.iter().any(|&d| !urban.contains(&(c + d))))
        .copied()
        .collect();
    let wall_dist = wall_distance(&wall_cells, arterial_params.wall_clearance);

    let mut paths: Vec<Path> = Vec::new();

    // The network built so far. Each new route gets a steep cost discount for
    // running on these cells (so it merges instead of crossing senselessly) and
    // snaps to their height; `road_height` keeps that y. Routes are laid down
    // tier by tier (arterials first) so collectors merge onto the backbone.
    let mut road_cells: HashSet<Point2D> = HashSet::new();
    let mut road_height: HashMap<Point2D, i32> = HashMap::new();

    // Tier 1 — arterials: MST over the backbone nodes. Each routes to its exact
    // node (no `goal_cells`) but discounts earlier arterials so parallel edges
    // merge rather than doubling up.
    for (i, j) in mst_edges(&backbone) {
        let routed = {
            let ctx = RouteContext {
                region: Some(&urban),
                road_cells: Some(&road_cells),
                road_height: Some(&road_height),
                goal_cells: None,
                wall_dist: Some(&wall_dist),
                blocked: Some(blocked),
            };
            get_path_with(
                editor, backbone[i], backbone[j],
                PathPriority::High, arterial_material.clone(), arterial_params,
                ctx, async |_| {},
            ).await
        };
        match routed {
            Some(path) => { record_path(&path, &mut road_cells, &mut road_height); paths.push(path); }
            None => log::warn!("build_road_network: arterial {i}->{j} failed to route"),
        }
    }

    // Tier 2 — collectors: each gate to its nearest backbone node, but ending
    // the moment it touches the existing network (`goal_cells`), so it spurs off
    // the nearest road instead of duplicating a run all the way to a node. We
    // snap the gate to the nearest urban cell as its in-city entry point.
    for (gate_point, _dir) in editor.world().gate_locations.clone() {
        let Some(entry) = nearest_in(gate_point.drop_y(), &urban) else { continue; };
        let start = editor.world().add_height(entry);
        let Some(target) = nearest_node(start, &backbone) else { continue; };
        let routed = {
            let ctx = RouteContext {
                region: Some(&urban),
                road_cells: Some(&road_cells),
                road_height: Some(&road_height),
                goal_cells: Some(&road_cells),
                wall_dist: Some(&wall_dist),
                blocked: Some(blocked),
            };
            get_path_with(
                editor, start, target,
                PathPriority::Medium, collector_material.clone(), collector_params,
                ctx, async |_| {},
            ).await
        };
        match routed {
            Some(path) => { record_path(&path, &mut road_cells, &mut road_height); paths.push(path); }
            None => log::warn!("build_road_network: collector from gate {:?} failed to route", gate_point),
        }
    }

    paths
}

/// Flood-fill the cells of `region` that are **not** in `barriers` into
/// connected components (4-connectivity), each one a "block" — an area walled
/// off by roads and the town wall. Use the road cells (at full width) plus the
/// wall cells as `barriers` so they act as the block outlines. Components
/// smaller than `min_size` are dropped as slivers.
///
/// 4-connectivity is deliberate: a 1-wide diagonal road still seals a block
/// (its corners aren't cardinally passable), so the fill can't leak across it.
pub fn find_blocks(
    region: &HashSet<Point2D>,
    barriers: &HashSet<Point2D>,
    min_size: usize,
) -> Vec<HashSet<Point2D>> {
    let open: HashSet<Point2D> = region.difference(barriers).copied().collect();
    let mut visited: HashSet<Point2D> = HashSet::new();
    let mut blocks: Vec<HashSet<Point2D>> = Vec::new();

    for &start in &open {
        if !visited.insert(start) {
            continue;
        }
        let mut block: HashSet<Point2D> = HashSet::new();
        let mut queue: VecDeque<Point2D> = VecDeque::new();
        queue.push_back(start);
        while let Some(cell) = queue.pop_front() {
            block.insert(cell);
            for dir in CARDINALS_2D {
                let n = cell + dir;
                if open.contains(&n) && visited.insert(n) {
                    queue.push_back(n);
                }
            }
        }
        if block.len() >= min_size {
            blocks.push(block);
        }
    }

    blocks
}

/// Multi-source BFS distance (in cardinal steps) from the wall cells, out to
/// `max_dist`. Wall cells are distance 0; cells farther than `max_dist` are
/// omitted (callers treat "absent" as "far, no penalty").
fn wall_distance(wall_cells: &HashSet<Point2D>, max_dist: i32) -> HashMap<Point2D, i32> {
    let mut dist: HashMap<Point2D, i32> = HashMap::new();
    let mut queue: VecDeque<Point2D> = VecDeque::new();
    for &c in wall_cells {
        dist.insert(c, 0);
        queue.push_back(c);
    }
    while let Some(c) = queue.pop_front() {
        let d = dist[&c];
        if d >= max_dist {
            continue;
        }
        for dir in CARDINALS_2D {
            let n = c + dir;
            if !dist.contains_key(&n) {
                dist.insert(n, d + 1);
                queue.push_back(n);
            }
        }
    }
    dist
}

/// Record a routed path's cells (and their height) into the running network so
/// later routes can merge onto it.
fn record_path(path: &Path, cells: &mut HashSet<Point2D>, heights: &mut HashMap<Point2D, i32>) {
    for p in path.points() {
        cells.insert(p.drop_y());
        heights.insert(p.drop_y(), p.y);
    }
}

/// Centroid of `cells`, snapped to the nearest member cell (concave-safe).
fn centroid_snapped(cells: &HashSet<Point2D>) -> Option<Point2D> {
    if cells.is_empty() {
        return None;
    }
    let avg = cells.iter().fold(Point2D::ZERO, |a, p| a + *p) / cells.len() as i32;
    if cells.contains(&avg) {
        return Some(avg);
    }
    cells.iter().min_by_key(|p| p.distance_manhattan(&avg)).copied()
}

/// Nearest cell in `cells` to `target` (squared-distance).
fn nearest_in(target: Point2D, cells: &HashSet<Point2D>) -> Option<Point2D> {
    cells.iter().min_by_key(|p| p.distance_squared(&target)).copied()
}

/// Nearest cell in `cells` to `target` that is not in `blocked` — used to keep
/// snapped nodes (e.g. the town centre) off a building footprint so they stay
/// routable.
fn nearest_unblocked(
    target: Point2D,
    cells: &HashSet<Point2D>,
    blocked: &HashSet<Point2D>,
) -> Option<Point2D> {
    cells
        .iter()
        .filter(|p| !blocked.contains(*p))
        .min_by_key(|p| p.distance_squared(&target))
        .copied()
}

/// Nearest node to `from` (compared in the XZ plane).
fn nearest_node(from: Point3D, nodes: &[Point3D]) -> Option<Point3D> {
    nodes.iter().min_by_key(|n| n.drop_y().distance_squared(&from.drop_y())).copied()
}

/// Prim's MST over `nodes`, edges weighted by XZ squared-distance. Returns the
/// `(i, j)` index pairs of the tree edges.
fn mst_edges(nodes: &[Point3D]) -> Vec<(usize, usize)> {
    let n = nodes.len();
    if n < 2 {
        return Vec::new();
    }
    let mut in_tree = vec![false; n];
    in_tree[0] = true;
    let mut edges = Vec::new();

    for _ in 1..n {
        let mut best: Option<(usize, usize, i32)> = None;
        for i in 0..n {
            if !in_tree[i] {
                continue;
            }
            for j in 0..n {
                if in_tree[j] {
                    continue;
                }
                let d = nodes[i].drop_y().distance_squared(&nodes[j].drop_y());
                if best.map_or(true, |(_, _, bd)| d < bd) {
                    best = Some((i, j, d));
                }
            }
        }
        match best {
            Some((i, j, _)) => {
                in_tree[j] = true;
                edges.push((i, j));
            }
            None => break,
        }
    }

    edges
}
