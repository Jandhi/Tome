use std::collections::{BinaryHeap, HashMap, HashSet};

use lerp::num_traits::clamp;

use crate::{editor::Editor, generator::{materials::MaterialId, paths::path::{Path, PathPriority}}, geometry::{Point2D, Point3D, ALL_8}};

fn mod4_point(point : Point3D, editor : &Editor) -> Point3D {
    let point = Point2D{
        x : point.x - point.x.rem_euclid(4),
        y : point.z - point.z.rem_euclid(4),
    };

    editor.world().add_height(point)
}

fn get_best_mod4_point(point : Point3D, editor : &Editor) -> Point3D {
    vec![(0, 0), (0, 4), (4, 0), (4, 4)]
        .into_iter()
        .map(|(dx, dz)| Point3D {
            x: point.x + dx,
            y: point.y,
            z: point.z + dz,
        })
        .filter(|p| editor.world().is_in_bounds_2d(p.drop_y()))
        .map(|p| mod4_point(p, editor))
        .min_by_key(|p| {
            p.y.abs_diff(point.y)
        })
        .unwrap_or(mod4_point(point, editor))
}

/// Tunable knobs for A* road routing. `step` is the horizontal lattice spacing
/// between search nodes — 4 = sparse (good for long, mostly-empty runs), 1 =
/// per-tile (precise, claim-aware, but far more expensive with the current
/// path-as-state A*). The weights shape the cost function: a higher
/// `turn_weight` yields straighter roads.
#[derive(Debug, Clone, Copy)]
pub struct RouteParams {
    /// Horizontal lattice step between A* nodes.
    pub step: i32,
    /// Max |Δy| the placed road may rise/fall per hop (terrain is clamped to this).
    pub max_grade: i32,
    /// Weight on direction-change (turn) cost. Higher → straighter.
    pub turn_weight: u64,
    /// Weight on cutting into terrain (burrowing below the surface).
    pub burrow_weight: u64,
    /// Weight on remaining height difference to the goal.
    pub goal_height_weight: u64,
    /// Flat extra cost for routing a node through water.
    pub water_cost: u64,
    /// Flat cost to step onto a cell that is already paved (see
    /// [`RouteContext::road_cells`]). Far below the normal per-step cost so a
    /// route prefers merging onto and running along an existing road rather
    /// than carving a parallel one. Only takes effect when `road_cells` is set.
    pub road_cost: u64,
    /// Flat extra cost for a diagonal step (both x and z change). A small value
    /// biases routes toward axis-aligned runs, which leave tidier frontage for
    /// buildings than staircased diagonals.
    pub diagonal_cost: u64,
    /// How many cells out from the wall the clearance penalty applies. A cell
    /// `d` cells from the wall (`d < wall_clearance`) is charged
    /// `(wall_clearance - d) * wall_weight`, so routes are pushed off the wall
    /// and only cross it where they must (gates). Needs
    /// [`RouteContext::wall_dist`] to take effect.
    pub wall_clearance: i32,
    /// Per-cell weight of the wall-clearance penalty (see `wall_clearance`).
    pub wall_weight: u64,
}

impl Default for RouteParams {
    fn default() -> Self {
        Self {
            step: 4,
            max_grade: 2,
            turn_weight: 1,
            burrow_weight: 10,
            goal_height_weight: 3,
            water_cost: 30,
            road_cost: 1,
            diagonal_cost: 2,
            wall_clearance: 8,
            wall_weight: 8,
        }
    }
}

/// Spatial context for a route beyond bare terrain. `region` is a hard
/// constraint (the route may never leave it). The rest couple a new route to
/// the roads already on the ground so the network grows coherently instead of
/// every route being solved in isolation:
/// - `road_cells` — cells already paved; stepping onto one costs only
///   [`RouteParams::road_cost`], so routes merge and share trunks.
/// - `road_height` — the y of each paved cell, so a merge snaps flush.
/// - `goal_cells` — reaching any of these ends the search (besides the explicit
///   end), so e.g. a collector stops the moment it touches the network instead
///   of duplicating a run all the way to a node.
#[derive(Clone, Copy, Default)]
pub struct RouteContext<'a> {
    pub region: Option<&'a HashSet<Point2D>>,
    pub road_cells: Option<&'a HashSet<Point2D>>,
    pub road_height: Option<&'a HashMap<Point2D, i32>>,
    pub goal_cells: Option<&'a HashSet<Point2D>>,
    /// Per-cell distance to the nearest wall cell, for cells within the
    /// clearance band (farther cells omitted = no penalty). Drives the
    /// wall-clearance penalty in [`RouteParams`].
    pub wall_dist: Option<&'a HashMap<Point2D, i32>>,
    /// Cells the route may never enter — placed building footprints (expanded by
    /// a small margin). Unlike `region` (which the route must stay *inside*),
    /// this is subtractive: a hard barrier so roads route *around* buildings
    /// instead of carving through them. Sufficient at the mod-4 lattice because
    /// any straight crossing of a building wider than `step` must land a waypoint
    /// inside the footprint, which this rejects.
    pub blocked: Option<&'a HashSet<Point2D>>,
}

pub async fn get_path(
    editor: &Editor,
    start: Point3D,
    end: Point3D,
    priority : PathPriority,
    material : MaterialId,
    explore_callback: impl AsyncFnMut(&Vec<Point3D>)
) -> Option<Path> {
    get_path_with(editor, start, end, priority, material, RouteParams::default(), RouteContext::default(), explore_callback).await
}

/// Like [`get_path`] but with explicit [`RouteParams`] and a [`RouteContext`]
/// (region constraint + the existing road network to merge onto).
pub async fn get_path_with(
    editor: &Editor,
    start: Point3D,
    end: Point3D,
    priority : PathPriority,
    material : MaterialId,
    params : RouteParams,
    ctx : RouteContext<'_>,
    explore_callback: impl AsyncFnMut(&Vec<Point3D>)
) -> Option<Path> {
    let new_start = get_best_mod4_point(start, editor);
    let new_end = get_best_mod4_point(end, editor);

    let width = match priority {
        PathPriority::Low => 1,
        PathPriority::Medium => 2,
        PathPriority::High => 3,
    };

    let mut path = route_path_with(editor, new_start, new_end, &params, ctx, explore_callback).await?;

    if !path.is_empty() {
        path = fill_out_path(path, priority != PathPriority::Low);
    }

    Some(Path::new(
        path,
        width,
        material,
        priority,
    ))
}

pub async fn route_path(
    editor: &Editor,
    start : Point3D,
    end: Point3D,
    explore_callback: impl AsyncFnMut(&Vec<Point3D>)
) -> Option<Vec<Point3D>> {
    route_path_with(editor, start, end, &RouteParams::default(), RouteContext::default(), explore_callback).await
}

/// Like [`route_path`] but with explicit [`RouteParams`] and a [`RouteContext`].
///
/// Grid A* over a `(cell, y, incoming-direction)` state with a `came_from`
/// map, so each state is settled at most once and nodes are cheap to hash.
/// (A path-as-state search would re-expand cells once per prefix and clone the
/// whole path per node — fine for short hops, but it blows up over the long,
/// flat district-spacing runs the road network needs.)
pub async fn route_path_with(
    editor: &Editor,
    start : Point3D,
    end: Point3D,
    params : &RouteParams,
    ctx : RouteContext<'_>,
    mut explore_callback: impl AsyncFnMut(&Vec<Point3D>)
) -> Option<Vec<Point3D>> {
    let new_start = get_best_mod4_point(start, editor);
    let new_end = get_best_mod4_point(end, editor);

    const HEURISTIC_WEIGHT: u64 = 10;

    // Primary hop = `step` cells; if that hop is too steep (terrain slope worse
    // than ~1:1 over the hop) fall back to a half-length hop so the route can
    // pick its way up rough ground instead of stepping over it blind.
    let neighbour_factors = [params.step, (params.step / 2).max(1)];

    // A node's y is clamped to ±max_grade of its predecessor, so the same cell
    // reached two ways can sit at different heights — y is part of the key.
    // `dir` (the incoming step delta) is what the turn penalty scores against;
    // it's `None` only at the start.
    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    struct State {
        cell: Point2D,
        y: i32,
        dir: Option<Point3D>,
    }
    impl State {
        fn pos(&self) -> Point3D {
            Point3D { x: self.cell.x, y: self.y, z: self.cell.y }
        }
    }

    // Min-heap by f = g + h (lazy deletion: stale pops filtered via `g_score`).
    struct HeapEntry {
        f: u64,
        g: u64,
        state: State,
    }
    impl PartialEq for HeapEntry {
        fn eq(&self, other: &Self) -> bool { self.f == other.f }
    }
    impl Eq for HeapEntry {}
    impl Ord for HeapEntry {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.f.cmp(&other.f).reverse()
        }
    }
    impl PartialOrd for HeapEntry {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    let start_cell = new_start.drop_y();
    let heuristic = |pos: Point3D| pos.distance(new_end) as u64 * HEURISTIC_WEIGHT;
    let is_end = |cell: Point2D, y: i32| {
        if cell == new_end.drop_y() && y.abs_diff(new_end.y) <= 4 {
            return true;
        }
        // Reaching the existing network ends the search too — but not at the
        // start cell, or a route that begins on a road would terminate instantly.
        cell != start_cell && ctx.goal_cells.is_some_and(|g| g.contains(&cell))
    };

    let start_state = State { cell: start_cell, y: new_start.y, dir: None };

    let mut open: BinaryHeap<HeapEntry> = BinaryHeap::new();
    let mut g_score: HashMap<State, u64> = HashMap::new();
    let mut came_from: HashMap<State, State> = HashMap::new();
    let mut closed: HashSet<State> = HashSet::new();

    g_score.insert(start_state, 0);
    open.push(HeapEntry { f: heuristic(new_start), g: 0, state: start_state });

    while let Some(HeapEntry { g, state, .. }) = open.pop() {
        // Stale heap entry: a cheaper route to this state was already settled.
        if g_score.get(&state).is_some_and(|&best| g > best) {
            continue;
        }
        if !closed.insert(state) {
            continue;
        }

        let current = state.pos();
        explore_callback(&vec![current]).await;

        if is_end(state.cell, state.y) {
            // Walk the parent chain back to the start, then reverse.
            let mut path = vec![current];
            let mut node = state;
            while let Some(&parent) = came_from.get(&node) {
                path.push(parent.pos());
                node = parent;
            }
            path.reverse();
            return Some(path);
        }

        for direction in ALL_8 {
            // Longest hop in this direction whose raw terrain slope is walkable
            // (≤1:1); fall back to the half-hop on steep ground.
            let mut chosen: Option<Point3D> = None;
            for &factor in &neighbour_factors {
                let neighbour_2d = state.cell + direction * factor;

                if !editor.world().is_in_bounds_2d(neighbour_2d) {
                    continue;
                }

                // Hard region constraint: never route outside the allowed cells
                // (e.g. urban area) so roads can't clip through the wall/edge.
                if let Some(region) = ctx.region {
                    if !region.contains(&neighbour_2d) {
                        continue;
                    }
                }

                // Hard barrier: never route through a placed building footprint.
                if let Some(blocked) = ctx.blocked {
                    if blocked.contains(&neighbour_2d) {
                        continue;
                    }
                }

                let raw = editor.world().add_height(neighbour_2d);
                if (raw.y.abs_diff(state.y) as i32) <= factor {
                    let y = clamp(raw.y, state.y - params.max_grade, state.y + params.max_grade);
                    chosen = Some(Point3D { x: neighbour_2d.x, y, z: neighbour_2d.y });
                    break;
                }
            }
            let Some(mut neighbour) = chosen else { continue; };

            // If this cell is already paved, merge onto it: snap to the road's
            // height (flush, not stepped) and treat the step as nearly free so
            // the route runs along the existing road instead of beside it.
            let on_road = ctx.road_cells.is_some_and(|rc| rc.contains(&neighbour.drop_y()));
            if on_road {
                if let Some(&ry) = ctx.road_height.and_then(|rh| rh.get(&neighbour.drop_y())) {
                    neighbour.y = ry;
                }
            }

            let delta = neighbour - current;
            let next = State { cell: neighbour.drop_y(), y: neighbour.y, dir: Some(delta) };
            if closed.contains(&next) {
                continue;
            }

            // Edge cost: a flat discount on paved cells (so routes merge),
            // otherwise the terrain cost — travel distance, a turn penalty
            // against the incoming direction, a burrow penalty for cutting
            // below the surface, a pull toward the goal height, and water.
            let step_cost = if on_road {
                params.road_cost
            } else {
                let mut c = neighbour.distance(current) as u64;
                if delta.x != 0 && delta.z != 0 {
                    c += params.diagonal_cost;
                }
                if let Some(in_dir) = state.dir {
                    c += delta.distance(in_dir) as u64 * params.turn_weight;
                }
                let burrow = editor.world().get_height_at(neighbour.drop_y()).abs_diff(neighbour.y) as u64;
                c += burrow * params.burrow_weight;
                c += neighbour.y.abs_diff(new_end.y) as u64 * params.goal_height_weight;
                if editor.world().is_water(neighbour.drop_y()) {
                    c += params.water_cost;
                }
                // Push the route off the wall: cells within `wall_clearance` of a
                // wall cell pay a penalty that ramps up as they approach, so the
                // road only hugs/crosses the wall where it has no choice (gates).
                if let Some(&d) = ctx.wall_dist.and_then(|wd| wd.get(&neighbour.drop_y())) {
                    if d < params.wall_clearance {
                        c += (params.wall_clearance - d) as u64 * params.wall_weight;
                    }
                }
                c
            };

            let tentative = g + step_cost;
            if g_score.get(&next).map_or(true, |&best| tentative < best) {
                g_score.insert(next, tentative);
                came_from.insert(next, state);
                let f = tentative + heuristic(neighbour);
                open.push(HeapEntry { f, g: tentative, state: next });
            }
        }
    }

    None
}


pub fn fill_out_path(mut points: Vec<Point3D>, allow_diagonals: bool) -> Vec<Point3D> {
    if points.is_empty() {
        return vec![];
    }
    let mut curr_point = points.remove(0);
    let mut full_points = vec![curr_point];
    if points.is_empty() {
        return full_points;
    }
    let mut next_point = points.remove(0);

    let mut x_axis_first = true;
    let mut can_update_y = true;

    while !points.is_empty() || curr_point != next_point {
        if can_update_y {
            if curr_point.y < next_point.y {
                curr_point.y += 1;
                can_update_y = !can_update_y;
            } else if curr_point.y > next_point.y {
                curr_point.y -= 1;
                can_update_y = !can_update_y;
            }
        } else {
            can_update_y = true;
        }

        if allow_diagonals {
            if curr_point.x > next_point.x && curr_point.z > next_point.z {
                curr_point.x -= 1;
                curr_point.z -= 1;
                full_points.push(curr_point);
                continue;
            }
            if curr_point.x < next_point.x && curr_point.z < next_point.z {
                curr_point.x += 1;
                curr_point.z += 1;
                full_points.push(curr_point);
                continue;
            }
            if curr_point.x > next_point.x && curr_point.z < next_point.z {
                curr_point.x -= 1;
                curr_point.z += 1;
                full_points.push(curr_point);
                continue;
            }
            if curr_point.x < next_point.x && curr_point.z > next_point.z {
                curr_point.x += 1;
                curr_point.z -= 1;
                full_points.push(curr_point);
                continue;
            }
        }

        if x_axis_first {
            if curr_point.x < next_point.x {
                curr_point.x += 1;
                full_points.push(curr_point);
                x_axis_first = !x_axis_first;
                continue;
            }
            if curr_point.x > next_point.x {
                curr_point.x -= 1;
                full_points.push(curr_point);
                x_axis_first = !x_axis_first;
                continue;
            }
        }
        if curr_point.z < next_point.z {
            curr_point.z += 1;
            full_points.push(curr_point);
            x_axis_first = !x_axis_first;
            continue;
        }
        if curr_point.z > next_point.z {
            curr_point.z -= 1;
            full_points.push(curr_point);
            x_axis_first = !x_axis_first;
            continue;
        }
        if curr_point.x < next_point.x {
            curr_point.x += 1;
            full_points.push(curr_point);
            x_axis_first = !x_axis_first;
            continue;
        }
        if curr_point.x > next_point.x {
            curr_point.x -= 1;
            full_points.push(curr_point);
            x_axis_first = !x_axis_first;
            continue;
        }

        // curr_point must be equal to next_point
        full_points.push(curr_point);
        if !points.is_empty() {
            next_point = points.remove(0);
        } else {
            break;
        }
    }

    full_points
}