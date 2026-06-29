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
use crate::generator::population::AnchorScene;
use crate::geometry::{Cardinal, Point2D};
use crate::noise::{Seed, RNG};

use super::additions::bowsprit::bowsprit_reach;
use super::crew::crew_scenes;
use super::hull::max_beam;
use super::keel::keel_depth;
use super::tuning::*;
use super::{build_ship, HullShape, SailState, ShipCtx, ShipSpec};

/// Largest keel length allowed for a water district, from its **dominant biome**: open
/// ocean / deep-ocean bodies get the full [`SHIP_LENGTHS`] range, while rivers / lakes /
/// anything else are capped to [`RIVER_MAX_LENGTH`]. Falls back to the full range when no
/// analysis exists for the district (e.g. a synthetic/test world).
fn size_cap_for_body(world: &World, district_id: usize) -> i32 {
    let full = SHIP_LENGTHS.iter().copied().max().unwrap_or(0);
    let Some(analysis) = world.district_analysis_data.get(&DistrictID(district_id)) else {
        return full;
    };
    // Tie-break on biome name so the dominant pick is stable across runs — `biome_count`
    // is a `HashMap`, and `max_by_key` would otherwise return an arbitrary one of equal
    // maxima depending on (randomized) iteration order.
    let dominant = analysis
        .biome_count()
        .iter()
        .max_by_key(|(biome, &count)| (count, biome.name()))
        .map(|(biome, _)| biome.name());
    match dominant {
        Some(name) if name.contains("ocean") => full,
        _ => RIVER_MAX_LENGTH,
    }
}

/// Scatter ships onto every sufficiently large Water district in the build area.
///
/// Deterministic for a given `seed`, independent of the town RNG stream. Returns the number
/// of ships placed **and** the crew [`AnchorScene`]s for the afloat ones (a captain at the
/// helm + sailors on deck) — the caller staffs them with the town roster, like any other
/// fixture. Crew looks/dialogue come from the `sailors` / `captains` fixtures in `npcs.yaml`.
pub async fn scatter_ships(
    editor: &mut Editor,
    data: &LoadedData,
    seed: Seed,
) -> (usize, Vec<AnchorScene>) {
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
        return (0, Vec::new());
    }

    // Distance-to-shore field over the whole build area (so a footprint never pokes over
    // land even where two adjacent water districts meet).
    let shore = shore_distance_field(editor.world(), size);

    // The wood palettes a ship can be built from — one is rolled per ship for variety.
    // The loader skips malformed/absent JSON *silently* (the load still succeeds), so a
    // requested id can be missing: collect only the palettes that actually loaded, and bail
    // gracefully if none did. This is the **last** pass in `generate_town`, so a panic here
    // would throw away a fully-generated settlement (and skip its final flush).
    let palettes: Vec<Palette> = SHIP_PALETTES
        .iter()
        .filter_map(|id| data.palettes.get(&PaletteId::from(*id)).cloned())
        .collect();
    if palettes.is_empty() {
        log::warn!("no ship palettes loaded ({:?}); skipping ship placement", SHIP_PALETTES);
        return (0, Vec::new());
    }

    let mut rng = RNG::new(seed).derive();
    let mut placed_cells: HashSet<Point2D> = HashSet::new();
    let mut total = 0usize;
    let mut crew: Vec<AnchorScene> = Vec::new();

    for (id, water_cells) in &bodies {
        // Candidate centres: water cells comfortably off the bank. Sorted so RNG-indexed
        // sampling is reproducible — `points_2d` is a `HashSet`, whose iteration order
        // varies per run, which would otherwise make placement non-deterministic.
        let mut centres: Vec<Point2D> = water_cells
            .iter()
            .copied()
            .filter(|c| shore_at(&shore, *c) >= MIN_CENTRE_SHORE)
            .collect();
        if centres.is_empty() {
            continue;
        }
        centres.sort_by_key(|c| (c.x, c.y));

        // Roll whether this district gets a ship at all (SHIP_CHANCE_PER_DISTRICT, currently
        // 50%). Districts with no eligible centre are already skipped above, so this only
        // decides among placeable bodies.
        if !rng.percent(SHIP_CHANCE_PER_DISTRICT) {
            continue;
        }

        // Size ceiling from the body's dominant biome: open ocean → full range, otherwise
        // (river / lake / other) capped to a modest hull.
        let max_length = size_cap_for_body(editor.world(), *id);

        // One ship per water district.
        let placed = place_one_ship(
            editor, data, &palettes, &mut rng, &centres, build_height, max_length,
            &mut placed_cells, &mut crew,
        )
        .await;
        if placed {
            total += 1;
        }
    }

    editor.flush_buffer().await;
    log::info!(
        "Scattered {} ships across {} water bodies ({} crew posts)",
        total, bodies.len(), crew.len(),
    );
    (total, crew)
}

/// Try to seat a **single** ship somewhere in `centres`. Rejection-samples up to
/// [`PLACE_ATTEMPTS`] centres; the first that admits a fitting ship is built via
/// [`build_ship`], its footprint claimed, and (for an afloat ship) its crew scenes pushed
/// onto `crew`. Returns `true` on success, `false` if no attempt found room (the body is
/// effectively full).
#[allow(clippy::too_many_arguments)]
async fn place_one_ship(
    editor: &mut Editor,
    data: &LoadedData,
    palettes: &[Palette],
    rng: &mut RNG,
    centres: &[Point2D],
    build_height: i32,
    max_length: i32,
    placed: &mut HashSet<Point2D>,
    crew: &mut Vec<AnchorScene>,
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

        // Roll this ship's wood (only on a successful fit, so rejected attempts don't churn
        // the stream). One palette per vessel.
        let palette = &palettes[rng.rand_i32_range(0, palettes.len() as i32) as usize];
        let hull_shape = if rng.percent(50) { HullShape::Teardrop } else { HullShape::Oval };
        let sail_state = if rng.percent(FURLED_CHANCE) { SailState::Furled } else { SailState::Full };
        let spec = ShipSpec::new(heading, length)
            .with_hull_shape(hull_shape)
            .with_sail_state(sail_state);

        let mut ship_rng = rng.derive();
        let mut ctx = ShipCtx::new(editor, data, palette, &mut ship_rng);
        let out = build_ship(&mut ctx, &spec, anchor).await;

        // Crew NPC scenes (afloat ships only); the settlement layer staffs them. Looks and
        // dialogue come from the ship-crew fixtures in `npcs.yaml`.
        let mut crew_rng = rng.derive();
        crew.extend(crew_scenes(&out, &data.npc_data, &mut crew_rng));

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
    let surface = world.get_motion_blocking_height_at(centre)?;
    let length_ceiling = (build_height - surface - VERTICAL_HEADROOM).min(max_length);

    for heading in headings {
        for &length in SHIP_LENGTHS {
            if length > length_ceiling {
                continue;
            }
            let min_depth = keel_depth(length) + KEEL_CLEARANCE;
            let bow_reach = bowsprit_reach(length);
            if let Some(footprint) =
                footprint_cells(world, centre, heading, length, min_depth, bow_reach, placed)
            {
                return Some((heading, length, footprint));
            }
        }
    }
    None
}

/// The footprint cells of a hull `length` long centred on `centre`, heading `heading`, or
/// `None` if any cell fails. The rect covers the hull (length × max-beam) plus
/// [`HULL_MARGIN`] of clear water on every side, **and** the bowsprit's forward overhang
/// (`bow_reach` cells past the bow). Two predicates apply:
/// - **hull cells** must be in-bounds, water, unclaimed, unoccupied, *and* deep enough that
///   the keel clears the seabed (`depth ≥ min_depth`);
/// - **bow-overhang cells** (the spar/jib hang at/above the surface) only need to be open
///   water — in-bounds, water, unclaimed, unoccupied — with no depth requirement, so a
///   bowsprit never embeds in the shore or pokes through another ship.
fn footprint_cells(
    world: &World,
    centre: Point2D,
    heading: Cardinal,
    length: i32,
    min_depth: i32,
    bow_reach: i32,
    placed: &HashSet<Point2D>,
) -> Option<Vec<Point2D>> {
    let dir: Point2D = heading.into();
    let perp: Point2D = heading.rotate_right().into();
    let half_len = length / 2 + HULL_MARGIN;
    let half_w = max_beam(length, DEFAULT_BEAM_RATIO) / 2 + HULL_MARGIN;

    // The hull spans `a ∈ [-half_len, half_len]` (bow toward `+dir`); the bowsprit overhangs
    // the bow by `bow_reach` more cells.
    let bow_max = half_len + bow_reach;
    let mut cells = Vec::with_capacity(((half_len + bow_max + 1) * (2 * half_w + 1)) as usize);
    for a in -half_len..=bow_max {
        let hull_cell = a <= half_len;
        for c in -half_w..=half_w {
            let cell = Point2D::new(centre.x + dir.x * a + perp.x * c, centre.y + dir.y * a + perp.y * c);
            if !world.is_in_bounds_2d(cell) || !world.is_water(cell) || placed.contains(&cell) {
                return None;
            }
            if hull_cell {
                let (Some(surface), Some(seabed)) = (
                    world.get_motion_blocking_height_at(cell),
                    world.get_ocean_floor_height_at(cell),
                ) else {
                    return None;
                };
                if surface - seabed < min_depth {
                    return None;
                }
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
