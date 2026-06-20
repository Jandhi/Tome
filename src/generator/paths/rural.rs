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

use std::collections::{HashMap, HashSet};

use crate::editor::Editor;
use crate::generator::BuildClaim;
use crate::generator::districts::{District, DistrictID};
use crate::generator::materials::MaterialId;
use crate::generator::nbts::StructureID;
use crate::generator::resource_chain::border_ring_cells;
use crate::geometry::{Point2D, Point3D, CARDINALS_2D};

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
    if gates.is_empty() || buildings.is_empty() {
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

    if jobs.is_empty() {
        return Vec::new();
    }

    // Confine routing to the countryside: the urban interior is blocked so a
    // rural road can't cut through the city (it meets the urban network at the
    // gate), and every footprint is blocked so roads route *around* buildings.
    // Gate nodes (and the anchors) are freed so routes can actually start/end on
    // them even if they fall on an otherwise-blocked cell.
    let mut blocked: HashSet<Point2D> = editor.world().get_urban_points();
    blocked.extend(&footprints);
    for g in &gates {
        blocked.remove(&g.drop_y());
    }
    for (anchor, _) in &jobs {
        blocked.remove(anchor);
    }

    let params = RouteParams { step: route_step, ..RouteParams::default() };

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
                wall_dist: None,
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

    log::info!(
        "rural road network: {} buildings, {} gates, {} segments routed, {} predicted ring cells",
        buildings.len(), gates.len(), paths.len(), ring_cells.len(),
    );
    paths
}

/// Gate destination cells, each stepped one cell to the **rural** (non-urban)
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
