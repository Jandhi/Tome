//! Rural road network: connects placed rural resource buildings to the urban
//! road network **via the town gates**, over un-flattened countryside.
//!
//! Each building is routed to its nearest gate with a terrain-aware A* (the same
//! router the urban network uses), attaching at the building's **door** when it
//! has one and otherwise at the nearest footprint-perimeter cell. Routes are
//! cost-coupled: later buildings merge onto roads already laid, so buildings
//! sharing a direction share a spine instead of each carving a parallel track.
//!
//! Where a production area will paint a `rural_road` **border ring**, the network
//! predicts that ring (it is deterministic from the district's edge geometry —
//! see [`crate::generator::resource_chain::paint_production_area`]) and seeds it
//! into the router's on-road discount field, so routes hug the ring rather than
//! running beside it. The ring itself is paved later by the painter.
//!
//! Run **after** all rural buildings are placed and **before** the production
//! painters. Realise the returned paths with
//! [`build_paths_merged`](super::build_paths_merged).

use std::collections::{HashMap, HashSet, VecDeque};

use crate::editor::Editor;
use crate::generator::BuildClaim;
use crate::generator::districts::{District, DistrictID};
use crate::generator::materials::MaterialId;
use crate::generator::nbts::StructureID;
use crate::generator::resource_chain::border_ring_cells;
use crate::geometry::{Point2D, Point3D, CARDINALS_2D};

use super::network::wall_distance_field;
use super::path::{Path, PathPriority};
use super::routing::{get_path_with, RouteContext, RouteParams};

/// A placed rural resource building the road network must connect to a gate.
pub struct RuralBuilding {
    pub district: DistrictID,
    pub structure: StructureID,
    /// Whether this building's production painter paints a `rural_road` border
    /// ring — if so, the network predicts and reuses that ring.
    pub has_border_ring: bool,
}

/// How far up a footprint column the door scan looks for a door block.
const DOOR_SCAN_HEIGHT: i32 = 6;

/// Build the rural road network. Returns one routed [`Path`] per building that
/// reached a gate (buildings that fail to route are logged and skipped — partial
/// connectivity is acceptable). `route_step` is the A* lattice step (1 = exact
/// per-cell, 4 = sparse/faster over long open runs).
pub async fn build_rural_road_network(
    editor: &Editor,
    buildings: &[RuralBuilding],
    material: MaterialId,
    route_step: i32,
) -> Vec<Path> {
    let gates = gate_nodes(editor);
    // No gates → nothing to connect. (Buildings may be empty: gates still get
    // their edge-spurs below, so every gate has a road regardless.)
    if gates.is_empty() {
        return Vec::new();
    }

    // Deterministic order so the cost-coupled merge result is reproducible.
    let mut order: Vec<usize> = (0..buildings.len()).collect();
    order.sort_by_key(|&i| buildings[i].district.0);

    // Per building: its attach anchor (door approach or nearest perimeter cell)
    // and the gate it heads for. Footprints accumulate into the blocked set, and
    // predicted border rings into the router's on-road discount field.
    let mut jobs: Vec<(Point2D, Point3D)> = Vec::new();
    let mut footprints: HashSet<Point2D> = HashSet::new();
    let mut ring_cells: HashSet<Point2D> = HashSet::new();

    for &i in &order {
        let b = &buildings[i];
        let Some(district) = editor.world().districts.get(&b.district) else { continue };
        let fc = footprint_cells(editor, district, &b.structure);
        if fc.is_empty() {
            continue;
        }
        footprints.extend(&fc);

        let centroid = fc.iter().fold(Point2D::ZERO, |a, p| a + *p) / fc.len() as i32;
        let gate = *gates
            .iter()
            .min_by_key(|g| g.drop_y().distance_squared(&centroid))
            .expect("gates is non-empty");
        let anchor = building_anchor(editor, &fc, gate.drop_y());
        jobs.push((anchor, gate));

        if b.has_border_ring {
            // Predict the painter's border ring (it hasn't run yet) from the same
            // shared computation, so routes can hug the ring the painter will lay.
            ring_cells.extend(border_ring_cells(district, editor));
        }
    }

    // Confine routing to the countryside: the urban interior is blocked so a
    // rural road can't cut through the city (it meets the urban network at the
    // gate), and every footprint is blocked so roads route *around* buildings.
    // Gate nodes (and the anchors) are freed so routes can actually start/end on
    // them even if they fall on an otherwise-blocked cell.
    let urban = editor.world().get_urban_points();
    let mut blocked: HashSet<Point2D> = urban.clone();
    blocked.extend(&footprints);
    for g in &gates {
        blocked.remove(&g.drop_y());
    }
    for (anchor, _) in &jobs {
        blocked.remove(anchor);
    }

    // Push rural roads off the town wall harder than the urban net does: a wider
    // clearance band so a route swings well clear of the wall and only touches it
    // at the gate, instead of grazing the wall on its way around.
    let params = RouteParams {
        step: route_step,
        wall_clearance: 14,
        wall_weight: 12,
        ..RouteParams::default()
    };
    let wall_dist = wall_distance_field(&urban, params.wall_clearance);

    let mut paths: Vec<Path> = Vec::new();
    // Cells of roads laid so far (a route may end on these to merge), plus their
    // heights so the merge snaps flush.
    let mut network_cells: HashSet<Point2D> = HashSet::new();
    let mut road_height: HashMap<Point2D, i32> = HashMap::new();

    for (anchor, gate) in jobs {
        let start = editor.world().add_height(anchor);

        // Discount field = laid roads ∪ predicted rings, so routes prefer running
        // along an existing road or a ring rather than carving a parallel one.
        let mut discount = network_cells.clone();
        discount.extend(&ring_cells);

        let routed = {
            let ctx = RouteContext {
                region: None,
                road_cells: Some(&discount),
                road_height: Some(&road_height),
                // Stop early only on the already-laid network (NOT on rings — a
                // ring isn't itself connected to a gate, so terminating on one
                // would strand the building). Empty on the first route.
                goal_cells: if network_cells.is_empty() { None } else { Some(&network_cells) },
                wall_dist: Some(&wall_dist),
                blocked: Some(&blocked),
            };
            get_path_with(editor, start, gate, PathPriority::Medium, material.clone(), params, ctx, async |_| {}).await
        };

        match routed {
            Some(path) => {
                for p in path.points() {
                    network_cells.insert(p.drop_y());
                    road_height.insert(p.drop_y(), p.y);
                }
                paths.push(path);
            }
            None => log::warn!(
                "rural road: building anchor {:?} failed to route to gate {:?}",
                anchor, gate.drop_y(),
            ),
        }
    }
    let building_segments = paths.len();

    // Ensure every gate has a road. A gate no building routed to would otherwise
    // open onto blank countryside, so give it a spur out to the nearest reachable
    // non-water edge — BFS finds where dry land first touches the boundary, then
    // the terrain-aware router lays the road there (falling back to the BFS land
    // path if A* can't reach it). A gate fenced in by water gets none.
    for gate in &gates {
        let gnode = gate.drop_y();
        if network_cells.contains(&gnode) {
            continue; // a building route already reached this gate
        }
        let Some(land_path) = bfs_to_dry_edge(editor, gnode, &blocked) else {
            log::warn!("rural road: gate {:?} found no dry edge to spur to", gnode);
            continue;
        };
        let target = editor.world().add_height(*land_path.last().expect("path has an edge cell"));

        let mut discount = network_cells.clone();
        discount.extend(&ring_cells);
        let routed = {
            let ctx = RouteContext {
                region: None,
                road_cells: Some(&discount),
                road_height: Some(&road_height),
                goal_cells: None, // run all the way out to the edge
                wall_dist: Some(&wall_dist),
                blocked: Some(&blocked),
            };
            get_path_with(editor, *gate, target, PathPriority::Medium, material.clone(), params, ctx, async |_| {}).await
        };

        // Prefer the terrain-aware A* road; if it can't reach the target (too
        // long/steep for its search), fall back to the BFS dry-land path so the
        // gate still gets a road. Width 2 matches the Medium collector tier.
        let path = routed.unwrap_or_else(|| {
            let pts: Vec<Point3D> = land_path.iter().map(|&c| editor.world().add_height(c)).collect();
            Path::new(pts, 2, material.clone(), PathPriority::Medium)
        });
        for p in path.points() {
            network_cells.insert(p.drop_y());
            road_height.insert(p.drop_y(), p.y);
        }
        paths.push(path);
    }

    log::info!(
        "rural road network: {} buildings, {} gates, {} segments routed ({} building, {} gate-spur), {} predicted ring cells",
        buildings.len(), gates.len(), paths.len(), building_segments, paths.len() - building_segments, ring_cells.len(),
    );
    paths
}

/// Gate destination nodes, each stepped one cell to the **rural** (non-urban)
/// side of the gate. The route centreline thus ends just outside the wall (never
/// on it), while the road's widened paved band still reaches back onto the gate
/// tile — meeting the urban collector that paves through the gate from the inside.
fn gate_nodes(editor: &Editor) -> Vec<Point3D> {
    let world = editor.world();
    world
        .gate_locations
        .iter()
        .map(|(gate, dir)| {
            let cell = gate.drop_y();
            let d = Point2D::from(*dir);
            // Step toward whichever side is not urban (the countryside).
            let outward = if world.is_urban(cell + d) { Point2D::new(-d.x, -d.y) } else { d };
            let out = cell + outward;
            let node = if world.is_in_bounds_2d(out) { out } else { cell };
            world.add_height(node)
        })
        .collect()
}

/// How far short of the build-area boundary a spur stops. The router and the
/// road-widening sample `is_water` / the heightmap on cells *beyond* the road
/// centreline, and those lookups aren't bounds-checked — running a road right
/// onto the boundary panics. A few cells of slack keeps every sample in bounds
/// while the spur still clearly reaches the edge.
const EDGE_MARGIN: i32 = 5;

/// Whether `c` sits at least `m` cells from every build-area boundary (so the
/// unchecked `is_water` / heightmap lookups around it stay in bounds).
fn well_inside(editor: &Editor, c: Point2D, m: i32) -> bool {
    let world = editor.world();
    world.is_in_bounds_2d(c + Point2D::new(m, 0))
        && world.is_in_bounds_2d(c + Point2D::new(-m, 0))
        && world.is_in_bounds_2d(c + Point2D::new(0, m))
        && world.is_in_bounds_2d(c + Point2D::new(0, -m))
}

/// BFS out from `start` over dry, unblocked countryside until dry land first
/// touches the safe near-edge ring (exactly [`EDGE_MARGIN`] from the build
/// boundary), returning the **whole land path** `start..=edge`. Crossing water
/// and the urban/footprint `blocked` set is disallowed, so the route is
/// guaranteed land-to-land. `None` if the gate is fenced in by water before any
/// edge is touched (a gate facing open sea gets no spur).
fn bfs_to_dry_edge(editor: &Editor, start: Point2D, blocked: &HashSet<Point2D>) -> Option<Vec<Point2D>> {
    let mut visited: HashSet<Point2D> = HashSet::from([start]);
    let mut parent: HashMap<Point2D, Point2D> = HashMap::new();
    let mut queue: VecDeque<Point2D> = VecDeque::from([start]);
    while let Some(c) = queue.pop_front() {
        // On the margin ring (≥ EDGE_MARGIN from all sides, but not one deeper)
        // → as close to this edge as we can safely lay road. Walk parents back.
        if c != start && well_inside(editor, c, EDGE_MARGIN) && !well_inside(editor, c, EDGE_MARGIN + 1) {
            let mut path = vec![c];
            let mut cur = c;
            while let Some(&p) = parent.get(&cur) {
                path.push(p);
                cur = p;
            }
            path.reverse(); // start → edge
            return Some(path);
        }
        for d in CARDINALS_2D {
            let n = c + d;
            if !visited.insert(n) {
                continue;
            }
            // Stay safely in bounds (so is_water never indexes OOB), off water,
            // and clear of the urban/footprint blockers.
            if !well_inside(editor, n, EDGE_MARGIN) || editor.world().is_water(n) || blocked.contains(&n) {
                continue;
            }
            parent.insert(n, c);
            queue.push_back(n);
        }
    }
    None
}

/// Footprint cells of `structure` within `district` — the cells claimed
/// `BuildClaim::Structure` with this building's unique instance id.
fn footprint_cells(editor: &Editor, district: &District, structure: &StructureID) -> HashSet<Point2D> {
    district
        .data
        .points_2d
        .iter()
        .copied()
        .filter(|&p| matches!(
            editor.world().get_claim(p),
            Some(BuildClaim::Structure(id)) if id.id == structure.id
        ))
        .collect()
}

/// The cell a road should attach to: the approach cell just outside a door if the
/// building has one, otherwise the footprint-perimeter cell nearest `target`.
fn building_anchor(editor: &Editor, footprint: &HashSet<Point2D>, target: Point2D) -> Point2D {
    if let Some(approach) = door_approach(editor, footprint, target) {
        return approach;
    }
    perimeter_outside(footprint)
        .into_iter()
        .min_by_key(|p| p.distance_squared(&target))
        // A non-empty footprint always has at least one outside neighbour.
        .unwrap_or_else(|| *footprint.iter().next().expect("footprint is non-empty"))
}

/// Scans the footprint columns for a door block; on a hit returns the outside
/// approach cell (cardinal neighbour not in the footprint) nearest `target`.
/// `None` if no door is found (mines, open pastures/apiaries).
fn door_approach(editor: &Editor, footprint: &HashSet<Point2D>, target: Point2D) -> Option<Point2D> {
    let mut approaches: Vec<Point2D> = Vec::new();
    for &cell in footprint {
        let ground = editor.world().get_non_tree_height(cell);
        for dy in 0..DOOR_SCAN_HEIGHT {
            // Read from the placement cache by local coord: the door was placed
            // this run, and `try_get_block` would subtract the build-area origin
            // and return world terrain instead (no door) on a live server.
            let Some(block) = editor.get_cached_block(Point3D::new(cell.x, ground + dy, cell.y)) else {
                continue;
            };
            let id = block.id.as_str();
            // `door` matches all door variants; exclude trapdoors (also contain "door").
            if id.contains("door") && !id.contains("trapdoor") {
                for d in CARDINALS_2D {
                    let n = cell + d;
                    if !footprint.contains(&n) {
                        approaches.push(n);
                    }
                }
                break;
            }
        }
    }
    approaches.into_iter().min_by_key(|p| p.distance_squared(&target))
}

/// Cells just outside the footprint (cardinal neighbours not in it).
fn perimeter_outside(footprint: &HashSet<Point2D>) -> Vec<Point2D> {
    let mut out: HashSet<Point2D> = HashSet::new();
    for &c in footprint {
        for d in CARDINALS_2D {
            let n = c + d;
            if !footprint.contains(&n) {
                out.insert(n);
            }
        }
    }
    out.into_iter().collect()
}
