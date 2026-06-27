//! Ship-placement pass — scatter free-floating ships onto **Water districts**.
//!
//! Body detection is reused from the districting system: districts classified
//! [`ParcelType::Water`] (`districts/classification.rs`) are the bodies. The pass seats
//! **one ship per water district** via [`place_one_ship`].
//!
//! Each placement guarantees the whole footprint is open water **deep enough that the keel
//! never touches the seabed** (`keel_depth(length) + KEEL_CLEARANCE`), oriented along the
//! body's longer axis, clear of the shore, other ships, and the build-height ceiling.
//!
//! Geometry reused unchanged: [`build_ship`] does all the hull/rig/interior work; this
//! module only chooses *where*, *which way*, and *how big*.

use std::collections::{HashSet, VecDeque};

use crate::editor::{Editor, World};
use crate::generator::BuildClaim;
use crate::generator::data::LoadedData;
use crate::generator::districts::{DistrictID, ParcelType};
use crate::generator::materials::{Palette, PaletteId};
use crate::geometry::{Cardinal, Point2D};
use crate::noise::{Seed, RNG};

use super::keel::keel_depth;
use super::tuning::*;
use super::{build_ship, HullShape, SailState, ShipCtx, ShipSpec};

/// Max hull beam for a keel length — mirrors `hull::build_hull_model`
/// (`max_beam = round(length / beam_ratio), min 3`) without building the hull.
fn max_beam(length: i32) -> i32 {
    ((length as f32) / DEFAULT_BEAM_RATIO).round().max(3.0) as i32
}

/// Largest keel length allowed for a water district, from its **dominant biome**: open
/// ocean / deep-ocean bodies get the full [`SHIP_LENGTHS`] range, while rivers / lakes /
/// anything else are capped to [`RIVER_MAX_LENGTH`]. Falls back to the full range when no
/// analysis exists for the district (e.g. a synthetic/test world).
fn size_cap_for_body(world: &World, district_id: usize) -> i32 {
    let full = SHIP_LENGTHS.iter().copied().max().unwrap_or(0);
    let Some(analysis) = world.district_analysis_data.get(&DistrictID(district_id)) else {
        return full;
    };
    let dominant = analysis
        .biome_count()
        .iter()
        .max_by_key(|(_, &count)| count)
        .map(|(biome, _)| biome.name());
    match dominant {
        Some(name) if name.contains("ocean") => full,
        _ => RIVER_MAX_LENGTH,
    }
}

/// Scatter ships onto every sufficiently large Water district in the build area.
///
/// Deterministic for a given `seed`, independent of the town RNG stream. Returns the
/// number of ships placed.
pub async fn scatter_ships(editor: &mut Editor, data: &LoadedData, seed: Seed) -> usize {
    // ── Plan inputs (owned, so no World borrow is held across build_ship) ──────
    let (size, build_height, bodies) = {
        let world = editor.world();
        let size = world.world_rect_2d().size; // (x, y = z extent), local coords
        let build_height = world.build_area.size.y; // local ceiling for masts/flags

        // Each Water district → its actual water cells (the district footprint can
        // include a fringe of shoreline; ships only seat on real water).
        let mut bodies: Vec<(usize, Vec<Point2D>)> = world
            .districts
            .iter()
            .filter(|(_, d)| d.data.parcel_type == ParcelType::Water)
            .map(|(id, d)| {
                let cells: Vec<Point2D> =
                    d.data.points_2d.iter().copied().filter(|p| world.is_water(*p)).collect();
                (id.0, cells)
            })
            .filter(|(_, cells)| cells.len() >= MIN_WATER_CELLS)
            .collect();
        bodies.sort_by_key(|(id, _)| *id); // stable order so a seed reproduces the fleet
        (size, build_height, bodies)
    };

    if bodies.is_empty() {
        return 0;
    }

    // Distance-to-shore field over the whole build area (so a footprint never pokes over
    // land even where two adjacent water districts meet).
    let shore = shore_distance_field(editor.world(), size);

    let palette = data
        .palettes
        .get(&PaletteId::from("ship_oak"))
        .expect("ship_oak palette (data/palettes/ships/)")
        .clone();

    let mut rng = RNG::new(seed).derive();
    let mut placed_cells: HashSet<Point2D> = HashSet::new();
    let mut total = 0usize;

    for (id, water_cells) in &bodies {
        // Candidate centres: water cells comfortably off the bank.
        let centres: Vec<Point2D> = water_cells
            .iter()
            .copied()
            .filter(|c| shore_at(&shore, *c) >= MIN_CENTRE_SHORE)
            .collect();
        if centres.is_empty() {
            continue;
        }

        // Size ceiling from the body's dominant biome: open ocean → full range, otherwise
        // (river / lake / other) capped to a modest hull.
        let max_length = size_cap_for_body(editor.world(), *id);

        // One ship per water district.
        let placed = place_one_ship(
            editor, data, &palette, &mut rng, &centres, build_height, max_length, &mut placed_cells,
        )
        .await;
        if placed {
            total += 1;
        }
    }

    editor.flush_buffer().await;
    log::info!("Scattered {} ships across {} water bodies", total, bodies.len());
    total
}

/// Try to seat a **single** ship somewhere in `centres`. Rejection-samples up to
/// [`PLACE_ATTEMPTS`] centres; the first that admits a fitting ship is built via
/// [`build_ship`] and its footprint claimed. Returns `true` on success, `false` if no
/// attempt found room (the body is effectively full).
async fn place_one_ship(
    editor: &mut Editor,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
    centres: &[Point2D],
    build_height: i32,
    max_length: i32,
    placed: &mut HashSet<Point2D>,
) -> bool {
    for _ in 0..PLACE_ATTEMPTS {
        let centre = centres[rng.rand_i32_range(0, centres.len() as i32) as usize];

        // Plan against the world immutably; the borrow ends before build_ship.
        let fit = {
            let world = editor.world();
            try_fit(world, centre, build_height, max_length, placed)
        };
        let Some((heading, length, footprint)) = fit else { continue };

        // Anchor = stern keel point: the hull runs +length along the heading from there,
        // so back off half a length to centre the hull on `centre`.
        let dir: Point2D = heading.into();
        let anchor = Point2D::new(centre.x - dir.x * (length / 2), centre.y - dir.y * (length / 2));

        let hull_shape = if rng.percent(50) { HullShape::Teardrop } else { HullShape::Oval };
        let sail_state = if rng.percent(FURLED_CHANCE) { SailState::Furled } else { SailState::Full };
        let spec = ShipSpec::new(heading, length)
            .with_hull_shape(hull_shape)
            .with_sail_state(sail_state);

        let mut ship_rng = rng.derive();
        let mut ctx = ShipCtx::new(editor, data, palette, &mut ship_rng);
        build_ship(&mut ctx, &spec, anchor).await;

        for cell in &footprint {
            placed.insert(*cell);
            editor.world_mut().claim(*cell, BuildClaim::Ship);
        }
        return true;
    }
    false
}

/// Plan a ship centred on `centre`: choose the orientation along the body's longer open
/// axis, then the largest length whose full footprint is open, deep enough for the keel to
/// clear the seabed, unclaimed, unoccupied, and under the build ceiling. Returns the
/// `(heading, length, footprint cells)` or `None` if nothing fits here.
fn try_fit(
    world: &World,
    centre: Point2D,
    build_height: i32,
    max_length: i32,
    placed: &HashSet<Point2D>,
) -> Option<(Cardinal, i32, Vec<Point2D>)> {
    const EXTENT_CAP: i32 = 30;
    let ext = |dir: Cardinal| open_extent(world, centre, dir, EXTENT_CAP);
    let (n, s, e, w) = (
        ext(Cardinal::North),
        ext(Cardinal::South),
        ext(Cardinal::East),
        ext(Cardinal::West),
    );

    // Bow points toward the more open side on each axis; try the longer axis first.
    let ns_head = if n >= s { Cardinal::North } else { Cardinal::South };
    let ew_head = if e >= w { Cardinal::East } else { Cardinal::West };
    let headings = if (n + s) >= (e + w) { [ns_head, ew_head] } else { [ew_head, ns_head] };

    // Conservative vertical-clearance bound: surface + length-scaled masts + headroom must
    // fit under the world top. (Mast top ≈ surface + ~0.82·length; using `length` is a safe
    // over-estimate.)
    let surface = world.get_motion_blocking_height_at(centre);
    let length_ceiling = (build_height - surface - VERTICAL_HEADROOM).min(max_length);

    for heading in headings {
        for &length in SHIP_LENGTHS {
            if length > length_ceiling {
                continue;
            }
            let min_depth = keel_depth(length) + KEEL_CLEARANCE;
            if let Some(footprint) = footprint_cells(world, centre, heading, length, min_depth, placed) {
                return Some((heading, length, footprint));
            }
        }
    }
    None
}

/// The footprint cells of a hull `length` long centred on `centre`, heading `heading`, or
/// `None` if any cell fails: out of bounds, not water, too shallow (`depth < min_depth` —
/// keel would touch the seabed), already claimed, or overlapping a previously placed ship.
/// The rect covers the hull (length × max-beam) plus [`HULL_MARGIN`] of clear water on
/// every side.
fn footprint_cells(
    world: &World,
    centre: Point2D,
    heading: Cardinal,
    length: i32,
    min_depth: i32,
    placed: &HashSet<Point2D>,
) -> Option<Vec<Point2D>> {
    let dir: Point2D = heading.into();
    let perp: Point2D = heading.rotate_right().into();
    let half_len = length / 2 + HULL_MARGIN;
    let half_w = max_beam(length) / 2 + HULL_MARGIN;

    let mut cells = Vec::with_capacity(((2 * half_len + 1) * (2 * half_w + 1)) as usize);
    for a in -half_len..=half_len {
        for c in -half_w..=half_w {
            let cell = Point2D::new(centre.x + dir.x * a + perp.x * c, centre.y + dir.y * a + perp.y * c);
            if !world.is_in_bounds_2d(cell) || !world.is_water(cell) || placed.contains(&cell) {
                return None;
            }
            let depth = world.get_motion_blocking_height_at(cell) - world.get_ocean_floor_height_at(cell);
            if depth < min_depth {
                return None;
            }
            match world.get_claim(cell) {
                Some(BuildClaim::None) | Some(BuildClaim::Nature) | None => {}
                _ => return None,
            }
            cells.push(cell);
        }
    }
    Some(cells)
}

/// Consecutive water cells stepping out from `centre` along `dir`, up to `cap` — a cheap
/// measure of open water used to choose the ship's orientation.
fn open_extent(world: &World, centre: Point2D, dir: Cardinal, cap: i32) -> i32 {
    let d: Point2D = dir.into();
    let mut n = 0;
    for k in 1..=cap {
        let c = Point2D::new(centre.x + d.x * k, centre.y + d.y * k);
        if world.is_in_bounds_2d(c) && world.is_water(c) {
            n += 1;
        } else {
            break;
        }
    }
    n
}

/// Multi-source BFS distance (in cells) from every water cell to the nearest land cell.
/// Land (non-water in-bounds) seeds at 0; the field is `-1` for water unreachable from any
/// land (e.g. open ocean running off the map edge), which [`shore_at`] reads as
/// "infinitely open".
fn shore_distance_field(world: &World, size: Point2D) -> Vec<Vec<i32>> {
    let (sx, sz) = (size.x as usize, size.y as usize);
    let mut dist = vec![vec![-1i32; sz]; sx];
    let mut queue: VecDeque<Point2D> = VecDeque::new();

    for x in 0..size.x {
        for z in 0..size.y {
            let cell = Point2D::new(x, z);
            if !world.is_water(cell) {
                dist[x as usize][z as usize] = 0;
                queue.push_back(cell);
            }
        }
    }

    while let Some(cell) = queue.pop_front() {
        let d = dist[cell.x as usize][cell.y as usize];
        for n in crate::geometry::CARDINALS_2D.iter().map(|c| *c + cell) {
            if n.x < 0 || n.y < 0 || n.x >= size.x || n.y >= size.y {
                continue;
            }
            if dist[n.x as usize][n.y as usize] == -1 && world.is_water(n) {
                dist[n.x as usize][n.y as usize] = d + 1;
                queue.push_back(n);
            }
        }
    }
    dist
}

/// Shore distance at `cell`; `-1` (unreachable from land) reads as `i32::MAX` (open sea).
fn shore_at(field: &[Vec<i32>], cell: Point2D) -> i32 {
    match field[cell.x as usize][cell.y as usize] {
        -1 => i32::MAX,
        d => d,
    }
}
