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
use crate::generator::districts::ParcelType;
use crate::generator::materials::MaterialId;
use crate::geometry::{CARDINALS_2D, Point2D, Point3D};

use super::path::{Path, PathPriority};
use super::routing::{get_path_with, RouteContext, RouteParams};

/// What a road-network node represents, for labelling what each road connects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    TownCentre,
    Industry,
    District,
    Gate,
}

impl std::fmt::Display for NodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            NodeKind::TownCentre => "town-centre",
            NodeKind::Industry => "industry",
            NodeKind::District => "district",
            NodeKind::Gate => "gate",
        };
        f.write_str(s)
    }
}

/// One edge of the abstract road graph (before A* routing curves it).
#[derive(Debug, Clone, Copy)]
pub struct RoadEdge {
    /// Indices into [`RoadNetwork::nodes`].
    pub a: usize,
    pub b: usize,
    /// A loop-closing shortcut rather than an MST backbone edge.
    pub shortcut: bool,
    /// Upgraded to the large (arterial) tier.
    pub arterial: bool,
}

/// The routed roads plus the abstract graph they came from, so callers can
/// overlay the underlying node/edge structure on the realized network.
pub struct RoadNetwork {
    pub paths: Vec<Path>,
    /// Destination nodes (industry, district centres, gates, town centre),
    /// deduped and relocated off footprints — the surface points the edges join.
    pub nodes: Vec<Point3D>,
    /// MST + shortcut edges over `nodes`.
    pub edges: Vec<RoadEdge>,
    /// Per-cell road number for the realised pavement, computed geometrically
    /// from the routed centrelines (split at true junctions — including A*
    /// crossings — then merged by actual straightness). This, not `Path::road_id`,
    /// is the source of truth for road identity in the map and street signs:
    /// one number = one continuous physical road. Centreline cells only.
    pub road_labels: HashMap<Point2D, u32>,
}

/// How much longer the tree path between two nodes must be than their
/// straight-line gap before we add a loop-closing shortcut between them.
const SHORTCUT_DETOUR_RATIO: f64 = 2.5;
/// Cap on shortcut count, as a fraction of the node count — keeps a sprawling
/// graph from sprouting a web of bypasses. Biggest-detour pairs win.
const SHORTCUT_CAP_FRACTION: f64 = 1.0 / 3.0;
/// Nodes closer than this (XZ cells) are merged — an industry building sitting
/// on its district centre, or two gates in the same corner, shouldn't seed a
/// near-zero-length edge.
const NODE_MERGE_DIST: i32 = 6;

/// A settlement needs at least this many network nodes before any road is
/// upgraded to the large (arterial) tier — a hamlet stays all-medium.
const BIG_CITY_MIN_NODES: usize = 18;
/// In a big-enough city, this top fraction of edges by traffic (betweenness)
/// are promoted from medium to large.
const ARTERIAL_FRACTION: f64 = 1.0 / 3.0;

/// A pairing of two segments at a junction must be at least this straight to be
/// the same road: `straightness = -dot(dirA, dirB)` of the two outgoing
/// directions, so 1.0 is a perfectly straight through-route, 0.0 a right-angle
/// turn, negative a hairpin. We keep roads as long as possible by continuing the
/// straightest available pair through any junction up to a right angle; only a
/// turn sharper than ~90° (a near-reversal) starts a new road. Each segment
/// still pairs at most once per junction, so a road never branches.
const STROKE_MIN_STRAIGHTNESS: f64 = 0.0;

/// Minimum centreline length (cells) for a road to keep a name. Shorter
/// leftovers stay paved but go unnamed (and uncoloured) rather than cluttering
/// the labels as their own "road".
const MIN_ROAD_CELLS: usize = 20;

/// Build the urban road network. Returns routed paths; individual edges may be
/// absent if A* failed to find a route.
///
/// All destinations — placed industry buildings (`anchor_nodes`), urban
/// district centres, town gates, and (optionally) the town centre — are
/// first-class nodes. The network is their minimum spanning tree plus a handful
/// of loop-closing shortcuts (node pairs that are close in space but far apart
/// along the tree). In a big-enough city the busiest edges (by graph
/// betweenness) are upgraded from medium to large; small towns stay all-medium.
pub async fn build_road_network(
    editor: &Editor,
    arterial_material: MaterialId,
    collector_material: MaterialId,
    include_town_center: bool,
    anchor_nodes: &[Point3D],
    blocked: &HashSet<Point2D>,
    // A* lattice step: 4 = sparse mod-4 lattice (fast, straight legs, but
    // endpoints snap to the grid and gaps narrower than the step are invisible
    // — edges can fail to route); 1 = exact per-cell search (no snapping,
    // threads any gap a road fits through, ~equal wall-clock in practice).
    route_step: i32,
) -> RoadNetwork {
    let urban = editor.world().get_urban_points();
    if urban.is_empty() {
        return RoadNetwork { paths: Vec::new(), nodes: Vec::new(), edges: Vec::new(), road_labels: HashMap::new() };
    }

    // Destination nodes (town centre, industry, district centres, gates),
    // deduped and relocated off building footprints.
    let (nodes, kinds) = assemble_nodes(editor, include_town_center, anchor_nodes, blocked, &urban);
    if nodes.len() < 2 {
        return RoadNetwork { paths: Vec::new(), nodes, edges: Vec::new(), road_labels: HashMap::new() };
    }

    // --- Edges: MST backbone + capped loop-closing shortcuts. ---
    let mst = mst_edges(&nodes);
    let shortcuts = shortcut_edges(&nodes, &mst);
    let gate_count = kinds.iter().filter(|k| **k == NodeKind::Gate).count();
    log::info!(
        "road network: {} nodes ({} gates), {} MST edges, {} shortcuts",
        nodes.len(), gate_count, mst.len(), shortcuts.len(),
    );

    // Tier each edge: the busiest form a connected arterial spine (large), the
    // rest stay medium. Small towns skip the upgrade (all-medium).
    let all_edges: Vec<(usize, usize)> = mst.iter().chain(shortcuts.iter()).copied().collect();
    let is_arterial = select_arterials(&nodes, &all_edges);

    // Arterials want straight, axis-aligned legs; collectors get a milder bias.
    let arterial_params = RouteParams { step: route_step, turn_weight: 6, diagonal_cost: 5, ..RouteParams::default() };
    let collector_params = RouteParams { step: route_step, turn_weight: 3, diagonal_cost: 4, ..RouteParams::default() };

    // Distance-to-wall field: routes pay a ramping penalty near the wall so they
    // keep clear of it and only meet it at gates.
    let wall_dist = wall_distance_field(&urban, arterial_params.wall_clearance);

    let mut paths: Vec<Path> = Vec::new();
    // The abstract edge each successfully-routed path came from, parallel to
    // `paths` — fed to the road-grouping (stroke) pass below.
    let mut path_edges: Vec<(usize, usize)> = Vec::new();
    // The network built so far: later edges get a steep discount for running on
    // these cells (so they merge), and snap to their height.
    let mut road_cells: HashSet<Point2D> = HashSet::new();
    let mut road_height: HashMap<Point2D, i32> = HashMap::new();

    // Route order: all arterials first (so the medium roads merge onto the large
    // trunk), then the rest. Within each tier, MST edges before shortcuts. For an
    // MST edge `(i, j)` we route from `j` toward the network and stop on first
    // touch (`goal_cells`); shortcuts route the full chord (both ends already
    // connected) so they actually lay the bypass.
    let mst_len = mst.len();
    let mut route_order: Vec<usize> = (0..all_edges.len()).collect();
    // Stable sort: arterials (true) before collectors; ties keep MST-before-shortcut.
    route_order.sort_by_key(|&ei| !is_arterial[ei]);

    for ei in route_order {
        let (i, j) = all_edges[ei];
        let is_shortcut = ei >= mst_len;
        let arterial = is_arterial[ei];
        let (start, end) = if is_shortcut { (nodes[i], nodes[j]) } else { (nodes[j], nodes[i]) };
        let (priority, material, params) = if arterial {
            (PathPriority::High, arterial_material.clone(), arterial_params)
        } else {
            (PathPriority::Medium, collector_material.clone(), collector_params)
        };
        let routed = {
            let ctx = RouteContext {
                region: Some(&urban),
                road_cells: Some(&road_cells),
                road_height: Some(&road_height),
                goal_cells: if is_shortcut { None } else { Some(&road_cells) },
                wall_dist: Some(&wall_dist),
                blocked: Some(blocked),
            };
            get_path_with(editor, start, end, priority, material, params, ctx, async |_| {}).await
        };
        match routed {
            Some(path) => {
                record_path(&path, &mut road_cells, &mut road_height);
                paths.push(path);
                path_edges.push((i, j));
            }
            None => log::warn!(
                "build_road_network: {} {} edge {i}->{j} failed to route",
                if arterial { "arterial" } else { "collector" },
                if is_shortcut { "shortcut" } else { "MST" },
            ),
        }
    }

    // Legacy topological grouping: still stamps a per-path road_id and logs what
    // each road connects. Kept for the abstract-graph view; road identity for the
    // map and signs now comes from the geometric pass below.
    name_roads(&mut paths, &path_edges, &nodes, &kinds);

    // Geometric labelling: split the realised centrelines at true junctions
    // (degree ≥ 3, catching A* crossings and merged corridors the node graph
    // can't see) and merge through them by actual straightness, so one number is
    // one continuous physical road.
    let road_labels = crate::generator::paths::label_roads_geometric(&paths);
    log::info!(
        "geometric road labelling: {} centreline cells -> {} roads",
        road_labels.len(),
        road_labels.values().copied().collect::<std::collections::HashSet<_>>().len(),
    );

    let edges = all_edges.iter().enumerate().map(|(ei, &(a, b))| RoadEdge {
        a, b, shortcut: ei >= mst_len, arterial: is_arterial[ei],
    }).collect();

    RoadNetwork { paths, nodes, edges, road_labels }
}

/// Assemble the network's destination nodes — town centre (optional), industry
/// buildings, urban district centres, gates — lifted to the post-flatten
/// surface, relocated off building footprints, and deduped. Returns the kept
/// node positions and their kinds, in sync.
fn assemble_nodes(
    editor: &Editor,
    include_town_center: bool,
    anchor_nodes: &[Point3D],
    blocked: &HashSet<Point2D>,
    urban: &HashSet<Point2D>,
) -> (Vec<Point3D>, Vec<NodeKind>) {
    let mut nodes: Vec<Point3D> = Vec::new();
    let mut kinds: Vec<NodeKind> = Vec::new();

    if include_town_center {
        if let Some(c) = centroid_snapped(urban) {
            nodes.push(editor.world().add_height(c));
            kinds.push(NodeKind::TownCentre);
        }
    }
    nodes.extend_from_slice(anchor_nodes);
    kinds.extend(std::iter::repeat(NodeKind::Industry).take(anchor_nodes.len()));
    for sd in editor.world().districts.values() {
        if sd.data.parcel_type != ParcelType::Urban {
            continue;
        }
        if let Some(c) = centroid_snapped(&sd.data.points_2d) {
            nodes.push(editor.world().add_height(c));
            kinds.push(NodeKind::District);
        }
    }
    // Gates use their exact centre so the road meets the threshold; paving lays
    // road surface across the gate/wall tiles without cutting into the wall.
    for (gate_point, _dir) in editor.world().gate_locations.clone() {
        nodes.push(editor.world().add_height(gate_point.drop_y()));
        kinds.push(NodeKind::Gate);
    }

    // A route can neither start nor end on a blocked cell, so pull any node
    // sitting on a footprint out to the nearest clear urban cell.
    for node in nodes.iter_mut() {
        if blocked.contains(&node.drop_y()) {
            if let Some(c) = nearest_unblocked(node.drop_y(), urban, blocked) {
                *node = editor.world().add_height(c);
            }
        }
    }

    // Merge near-coincident nodes so the MST has no degenerate edges (positions
    // and kinds kept in sync via the surviving indices).
    let keep = dedup_node_indices(&nodes, NODE_MERGE_DIST);
    let kept_nodes = keep.iter().map(|&i| nodes[i]).collect();
    let kept_kinds = keep.iter().map(|&i| kinds[i]).collect();
    (kept_nodes, kept_kinds)
}

/// Choose which edges are arterials (large tier). Ranks edges by graph
/// betweenness, then — in a big-enough city — grows a *connected* arterial spine
/// from the busiest edge outward (always adding the busiest edge that touches
/// the spine), up to `ARTERIAL_FRACTION` of all edges. Small towns get none.
fn select_arterials(nodes: &[Point3D], edges: &[(usize, usize)]) -> Vec<bool> {
    let betweenness = edge_betweenness(nodes, edges);
    let arterial_count = if nodes.len() >= BIG_CITY_MIN_NODES {
        ((edges.len() as f64) * ARTERIAL_FRACTION).round() as usize
    } else {
        0
    };
    let mut by_traffic: Vec<usize> = (0..edges.len()).collect();
    by_traffic.sort_by(|&a, &b| betweenness[b].partial_cmp(&betweenness[a]).unwrap_or(std::cmp::Ordering::Equal));

    let mut is_arterial = vec![false; edges.len()];
    if arterial_count > 0 {
        let seed = by_traffic[0];
        is_arterial[seed] = true;
        let mut spine_nodes: HashSet<usize> = HashSet::new();
        spine_nodes.insert(edges[seed].0);
        spine_nodes.insert(edges[seed].1);
        let mut count = 1;
        while count < arterial_count {
            // Busiest edge (in sorted order) that touches the spine so far.
            let next = by_traffic.iter().copied().find(|&ei| {
                !is_arterial[ei] && {
                    let (a, b) = edges[ei];
                    spine_nodes.contains(&a) || spine_nodes.contains(&b)
                }
            });
            match next {
                Some(ei) => {
                    is_arterial[ei] = true;
                    spine_nodes.insert(edges[ei].0);
                    spine_nodes.insert(edges[ei].1);
                    count += 1;
                }
                None => break, // spine can't grow further (graph exhausted)
            }
        }
    }
    log::info!(
        "road tiers: {} arterial / {} edges (city {} >= {} nodes: {})",
        arterial_count, edges.len(), nodes.len(), BIG_CITY_MIN_NODES,
        nodes.len() >= BIG_CITY_MIN_NODES,
    );
    is_arterial
}

/// Group segments into named roads (strokes), discard stubs shorter than
/// `MIN_ROAD_CELLS`, and stamp the surviving road id onto each path (short ones
/// stay paved but unnamed). `path_edges` is the node pair each path spans,
/// parallel to `paths`. Logs the count and what each road connects.
fn name_roads(paths: &mut [Path], path_edges: &[(usize, usize)], nodes: &[Point3D], kinds: &[NodeKind]) {
    // Strokes: at each junction the straightest pair continues as one road and
    // everything else dead-ends, so each road is a single maximal path.
    let stroke_of_path = group_into_roads(nodes, path_edges);
    let stroke_count = stroke_of_path.iter().copied().max().map_or(0, |m| m + 1) as usize;

    // Centreline length per stroke; roads shorter than the threshold lose their
    // name. Survivors get re-densified ids; short ones map to None.
    let mut centreline: Vec<usize> = vec![0; stroke_count];
    for (p, &rid) in paths.iter().zip(stroke_of_path.iter()) {
        centreline[rid as usize] += p.points().len();
    }
    let mut remap: Vec<Option<u32>> = vec![None; stroke_count];
    let mut next = 0u32;
    for (id, &len) in centreline.iter().enumerate() {
        if len >= MIN_ROAD_CELLS {
            remap[id] = Some(next);
            next += 1;
        }
    }
    for (path, &rid) in paths.iter_mut().zip(stroke_of_path.iter()) {
        if let Some(nid) = remap[rid as usize] {
            path.set_road_id(nid);
        }
    }
    log::info!(
        "road grouping: {} segments -> {} strokes -> {} named roads (short ones discarded)",
        paths.len(), stroke_count, next,
    );

    // What each named road connects: the kinds of nodes its segments touch.
    let mut road_nodes: std::collections::BTreeMap<u32, std::collections::BTreeSet<usize>> =
        std::collections::BTreeMap::new();
    for ((a, b), &rid) in path_edges.iter().zip(stroke_of_path.iter()) {
        if let Some(nid) = remap[rid as usize] {
            let set = road_nodes.entry(nid).or_default();
            set.insert(*a);
            set.insert(*b);
        }
    }
    for (nid, ns) in &road_nodes {
        let desc: Vec<String> = ns.iter().map(|&i| format!("{}#{}", kinds[i], i)).collect();
        log::info!("  road {} connects: {}", nid, desc.join(", "));
    }
}

/// Group routed segments into named roads ("strokes"). At each junction the
/// straightest pair of segments is joined into the same road (greedily, each
/// segment paired once), so a long avenue keeps one identity while side streets
/// that meet it at an angle become their own roads. Returns a road id per
/// segment (parallel to `edges`).
fn group_into_roads(nodes: &[Point3D], edges: &[(usize, usize)]) -> Vec<u32> {
    let m = edges.len();
    let mut parent: Vec<usize> = (0..m).collect();
    fn find(parent: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while parent[r] != r { r = parent[r]; }
        let mut cur = x;
        while parent[cur] != r { let next = parent[cur]; parent[cur] = r; cur = next; }
        r
    }

    // Outgoing direction of edge `ei` away from node `node` (unit, XZ).
    let dir_away = |ei: usize, node: usize| -> (f64, f64) {
        let (a, b) = edges[ei];
        let other = if node == a { b } else { a };
        let d = nodes[other].drop_y() - nodes[node].drop_y();
        let len = ((d.x * d.x + d.y * d.y) as f64).sqrt().max(1e-9);
        (d.x as f64 / len, d.y as f64 / len)
    };

    // Incident segments per node.
    let mut incident: HashMap<usize, Vec<usize>> = HashMap::new();
    for (ei, &(a, b)) in edges.iter().enumerate() {
        incident.entry(a).or_default().push(ei);
        incident.entry(b).or_default().push(ei);
    }

    for (&node, inc) in &incident {
        // Candidate pairings, straightest first.
        let mut cands: Vec<(f64, usize, usize)> = Vec::new();
        for x in 0..inc.len() {
            for y in (x + 1)..inc.len() {
                let (e1, e2) = (inc[x], inc[y]);
                let d1 = dir_away(e1, node);
                let d2 = dir_away(e2, node);
                let straightness = -(d1.0 * d2.0 + d1.1 * d2.1);
                cands.push((straightness, e1, e2));
            }
        }
        cands.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut used: HashSet<usize> = HashSet::new();
        for (s, e1, e2) in cands {
            if s < STROKE_MIN_STRAIGHTNESS {
                break;
            }
            if used.contains(&e1) || used.contains(&e2) {
                continue;
            }
            let (r1, r2) = (find(&mut parent, e1), find(&mut parent, e2));
            if r1 != r2 {
                parent[r1] = r2;
            }
            used.insert(e1);
            used.insert(e2);
        }
    }

    // Compact roots into dense 0..road_count ids.
    let mut root_to_id: HashMap<usize, u32> = HashMap::new();
    let mut next = 0u32;
    let mut out = vec![0u32; m];
    for ei in 0..m {
        let r = find(&mut parent, ei);
        let id = *root_to_id.entry(r).or_insert_with(|| { let v = next; next += 1; v });
        out[ei] = id;
    }
    out
}

/// Edge betweenness on the abstract node graph: for every ordered node pair, the
/// (weighted) shortest path is found and each edge it crosses is tallied. Edges
/// carrying the most pair-paths are the trunks. Weights are XZ edge lengths.
/// O(n³) dense Dijkstra — the node count is tiny, so no heap is needed.
fn edge_betweenness(nodes: &[Point3D], edges: &[(usize, usize)]) -> Vec<f64> {
    let n = nodes.len();
    let mut adj: Vec<Vec<(usize, usize, f64)>> = vec![Vec::new(); n];
    for (ei, &(a, b)) in edges.iter().enumerate() {
        let w = node_dist(nodes[a], nodes[b]);
        adj[a].push((b, ei, w));
        adj[b].push((a, ei, w));
    }

    let mut bet = vec![0.0; edges.len()];
    for s in 0..n {
        let mut dist = vec![f64::INFINITY; n];
        let mut visited = vec![false; n];
        let mut pred_edge = vec![usize::MAX; n];
        let mut pred_node = vec![usize::MAX; n];
        dist[s] = 0.0;
        for _ in 0..n {
            let mut u = usize::MAX;
            let mut best = f64::INFINITY;
            for v in 0..n {
                if !visited[v] && dist[v] < best {
                    best = dist[v];
                    u = v;
                }
            }
            if u == usize::MAX {
                break;
            }
            visited[u] = true;
            for &(nb, ei, w) in &adj[u] {
                if dist[u] + w < dist[nb] {
                    dist[nb] = dist[u] + w;
                    pred_edge[nb] = ei;
                    pred_node[nb] = u;
                }
            }
        }
        // Tally every edge on each shortest path s -> t.
        for t in 0..n {
            if t == s {
                continue;
            }
            let mut cur = t;
            while pred_node[cur] != usize::MAX {
                bet[pred_edge[cur]] += 1.0;
                cur = pred_node[cur];
            }
        }
    }
    bet
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

/// Distance-to-wall field for the urban perimeter: derives the wall cells (urban
/// cells with at least one non-urban cardinal neighbour) and runs a multi-source
/// BFS out to `max_dist`. Wall cells are distance 0; cells farther than `max_dist`
/// are omitted (callers treat "absent" as "far, no penalty"). Shared by the urban
/// and rural routers so both push their roads off the town wall.
pub(super) fn wall_distance_field(urban: &HashSet<Point2D>, max_dist: i32) -> HashMap<Point2D, i32> {
    let wall_cells: HashSet<Point2D> = urban.iter()
        .filter(|&&c| CARDINALS_2D.iter().any(|&d| !urban.contains(&(c + d))))
        .copied()
        .collect();

    let mut dist: HashMap<Point2D, i32> = HashMap::new();
    let mut queue: VecDeque<Point2D> = VecDeque::new();
    for &c in &wall_cells {
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

/// Indices of nodes to keep after dropping any that sit within `min_dist` (XZ)
/// of an already-kept node, so the MST never builds a degenerate near-zero-length
/// edge (e.g. an industry building whose centroid coincides with its district
/// centre). Returns kept indices so a parallel array (kinds) can be filtered too.
fn dedup_node_indices(nodes: &[Point3D], min_dist: i32) -> Vec<usize> {
    let min_sq = min_dist * min_dist;
    let mut kept: Vec<usize> = Vec::new();
    for (i, &n) in nodes.iter().enumerate() {
        if kept.iter().all(|&k| nodes[k].drop_y().distance_squared(&n.drop_y()) > min_sq) {
            kept.push(i);
        }
    }
    kept
}

/// XZ Euclidean distance between two nodes.
fn node_dist(a: Point3D, b: Point3D) -> f64 {
    (a.drop_y().distance_squared(&b.drop_y()) as f64).sqrt()
}

/// Length of the unique tree path between nodes `i` and `j`, summed as XZ
/// Euclidean edge lengths. BFS over the tree adjacency; the node count is small.
fn tree_path_length(adj: &[Vec<usize>], nodes: &[Point3D], i: usize, j: usize) -> f64 {
    let mut parent: HashMap<usize, usize> = HashMap::new();
    let mut queue: VecDeque<usize> = VecDeque::new();
    let mut seen: HashSet<usize> = HashSet::new();
    queue.push_back(i);
    seen.insert(i);
    while let Some(c) = queue.pop_front() {
        if c == j {
            break;
        }
        for &nb in &adj[c] {
            if seen.insert(nb) {
                parent.insert(nb, c);
                queue.push_back(nb);
            }
        }
    }
    // Walk j back to i, accumulating edge lengths.
    let mut total = 0.0;
    let mut cur = j;
    while let Some(&p) = parent.get(&cur) {
        total += node_dist(nodes[cur], nodes[p]);
        cur = p;
    }
    total
}

/// Loop-closing shortcuts: node pairs whose tree path is at least
/// `SHORTCUT_DETOUR_RATIO`× their straight-line gap. Capped to a fraction of the
/// node count, biggest-detour pairs first, so a sprawling graph doesn't grow a
/// web of bypasses.
fn shortcut_edges(nodes: &[Point3D], mst: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let n = nodes.len();
    if n < 4 {
        return Vec::new();
    }
    let mut adj = vec![Vec::new(); n];
    for &(i, j) in mst {
        adj[i].push(j);
        adj[j].push(i);
    }

    // (saving ratio, i, j) for every non-adjacent pair past the threshold.
    let mut candidates: Vec<(f64, usize, usize)> = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            if adj[i].contains(&j) {
                continue; // already a tree edge
            }
            let straight = node_dist(nodes[i], nodes[j]);
            if straight < 1.0 {
                continue;
            }
            let tree = tree_path_length(&adj, nodes, i, j);
            let ratio = tree / straight;
            if ratio >= SHORTCUT_DETOUR_RATIO {
                candidates.push((ratio, i, j));
            }
        }
    }

    // Highest detour ratio first, capped.
    candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let cap = ((n as f64) * SHORTCUT_CAP_FRACTION).floor() as usize;
    candidates.into_iter().take(cap.max(1)).map(|(_, i, j)| (i, j)).collect()
}

/// Prim's MST over `nodes`, edges weighted by XZ squared-distance. Returns the
/// `(i, j)` index pairs of the tree edges.
pub(crate) fn mst_edges(nodes: &[Point3D]) -> Vec<(usize, usize)> {
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
