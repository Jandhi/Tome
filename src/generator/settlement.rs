use std::collections::{HashMap, HashSet};

use crate::data::Loadable;
use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::districts::{build_wall, generate_parcels, ParcelType, WallType};
use crate::generator::materials::{Material, MaterialId, Placer};
use crate::generator::nbts::Structure;
use crate::generator::paths::{build_paths_merged, build_road_network, build_rural_road_network, find_blocks, Path, PathPriority, RuralBuilding};
use crate::generator::placement::{resolve_rural_production, try_place_rural, PlacedRural};
use crate::generator::resource_chain::paint_production_area_for;
use crate::generator::terrain::{flatten_urban_area, force_height, log_trees};
use crate::geometry::{Point2D, Point3D};
use crate::minecraft::Block;
use crate::noise::{Seed, RNG};

/// Full town-generation pipeline: feathered urban flatten + tiered A* road
/// network, then hierarchical house placement.
///
/// parcels -> wall+gates -> flatten -> industrial buildings -> arterials(MST) +
/// collectors(gates) -> blocks/subdivision -> roads -> houses -> verge + lights.
///
/// The caller is responsible for constructing the `Editor` (and the `World`
/// behind it) and for flushing/finalising afterwards beyond the final
/// `flush_buffer` performed here.
/// Residents per bed of sleeping capacity. A house's population budget is
/// `max(1, round(beds * POPULATION_PER_BED))`, so a single-bed house houses ~2
/// and a double bed (which sleeps two) ~3 — enough to read as lived-in.
const POPULATION_PER_BED: f32 = 1.5;

pub async fn generate_town(
    editor: &mut Editor,
    seed: Seed,
    culture: crate::generator::buildings_v2::Culture,
) {
    let mut rng = RNG::new(seed);
    let mut rng2 = RNG::new(seed);

    // Infrastructure materials follow the culture: a desert town gets sandstone
    // roads and walls, everyone else the default stone/cobble.
    let desert = matches!(culture, crate::generator::buildings_v2::Culture::Desert);
    let (wall_mat, arterial_mat, collector_mat): (&str, &str, &str) = if desert {
        ("smooth_sandstone", "smooth_sandstone", "sandstone")
    } else {
        ("stone_bricks", "stone_bricks", "cobblestone")
    };

    generate_parcels(seed, editor).await;

    
    let data = LoadedData::load().expect("Failed to load data");

    // ── Resource chain over rural districts ──────────────────────────────
    let rural_analysis: HashMap<_, _> = editor.world().district_analysis_data.iter()
        .filter(|(id, _)| {
            editor.world().districts.get(id)
                .map(|d| d.data.parcel_type == ParcelType::Rural)
                .unwrap_or(false)
        })
        .map(|(id, analysis)| (*id, analysis.clone()))
        .collect();
    // Resolve the rural economy with placement feasibility folded in: parcels that
    // can't physically seat a resource's gather building (footprint too big for any
    // flat enough pad) are excluded during assignment, so the plan never promises a
    // building placement would later drop. (Rural terrain is still natural here —
    // flatten/walls only touch urban.)
    let result = resolve_rural_production(&data, editor, &rural_analysis, &mut rng);

    // Phase 1 — feathered urban flatten.
    let urban = editor.world().get_urban_points();
    // Log (clear) the urban area of trees so roads, buildings, and houses
    // aren't dropped into standing forest.
    log_trees(&*editor, urban.clone()).await;
    println!("Logged {} urban cells of trees", urban.len());
    flatten_urban_area(editor, &urban, 16, 12, true).await;

    // Wall + gates — gates populate world.gate_locations, used by the network.
    let materials = Material::load().expect("Failed to load materials");
    let wall_material = MaterialId::new(wall_mat.to_string());
    let mut placer: Placer = Placer::new(&materials, &mut rng);
    let structures = Structure::load().expect("Failed to load structures");
    let data = LoadedData::load().expect("Failed to load data");
    // Re-skin wall towers into the culture palette so the placed tower NBT
    // matches the rest of the settlement. The tower's oak cap maps to the roof
    // role, so each culture's tower roof follows its building roofs. Desert is
    // the exception: merge a dark-prismarine roof override so desert tower roofs
    // pop against the sandstone body instead of being sandstone-on-sandstone.
    let tower_palette = data.palettes.get(&culture.palette_id()).cloned().map(|p| {
        if desert {
            let roof = data.palettes.get(&"prismarine_roof".into())
                .expect("prismarine_roof palette not found");
            p.merged_with(roof)
        } else {
            p
        }
    });
    build_wall(
        &editor.world().get_urban_points(), editor, &mut rng2,
        &mut placer, &wall_material, &structures, WallType::Standard, tower_palette.as_ref(),
    ).await;
    drop(placer);

    // DEBUG: how many urban super-parcels and gates did we actually get?
    {
        let n_urban = editor.world().districts.values()
            .filter(|sd| sd.data.parcel_type == crate::generator::districts::ParcelType::Urban)
            .count();
        let n_total = editor.world().districts.len();
        println!("URBAN super-parcels: {}/{} total | gates: {}", n_urban, n_total, editor.world().gate_locations.len());
    }

    let mut sd_ids: Vec<_> = result.parcel_assignments.keys().cloned().collect();
        sd_ids.sort_by_key(|id| id.0);
        // Dropped-by-competition-cap parcels, ordered flattest-first per resource:
        // promoted when a primary fails to seat, so a terrain miss costs us a different
        // parcel rather than the building (and the planned economy) entirely.
        let mut fallbacks: HashMap<String, std::collections::VecDeque<_>> = result
            .fallback_assignments
            .iter()
            .map(|(res, list)| (res.clone(), list.iter().cloned().collect()))
            .collect();
        let mut placed = 0usize;
        // Placed rural buildings, collected so the road network can connect them
        // and the production painters can run *after* the roads (R3 below).
        let mut placed_rural: Vec<PlacedRural> = Vec::new();
        for sd_id in &sd_ids {
            let assignment = result.parcel_assignments[sd_id].clone();
            if let Some(p) = try_place_rural(*sd_id, &assignment, &data, editor, &mut rng).await {
                placed += 1;
                placed_rural.push(p);
                continue;
            }
            // Primary couldn't seat — promote the best dropped same-resource parcel(s)
            // until one places, keeping the per-resource count at its cap.
            while let Some((fb_id, fb_assignment)) = fallbacks
                .get_mut(&assignment.primary_resource)
                .and_then(|q| q.pop_front())
            {
                log::info!(
                    "[resource-chain]   promoting fallback {:?} for resource {} after {:?} failed to place",
                    fb_id, assignment.primary_resource, sd_id,
                );
                if let Some(p) = try_place_rural(fb_id, &fb_assignment, &data, editor, &mut rng).await {
                    placed += 1;
                    placed_rural.push(p);
                    break;
                }
            }
        }
        log::info!("Placed {} of {} rural buildings", placed, sd_ids.len());

    // ── Rural road network (built BEFORE the production painters) ─────────
    // Connect every placed rural building to a town gate, predicting and reusing
    // the `rural_road` border ring each painter will lay. Realise + claim the
    // roads here so the painters' border rings skip the cells the road owns.
    let rural_material = MaterialId::new("rural_road".to_string());
    let rural_buildings: Vec<RuralBuilding> = placed_rural.iter().map(|p| RuralBuilding {
        district: p.district,
        structure: p.structure.clone(),
        has_border_ring: p.has_border_ring,
    }).collect();
    let rural_paths = build_rural_road_network(&*editor, &rural_buildings, rural_material, 1).await;
    if !rural_paths.is_empty() {
        // Flatten the routed corridor to the road heights (skipping building /
        // wall cells so a placed structure isn't re-graded), then meld the
        // surface — mirrors the urban road realization.
        let mut corridor: HashMap<Point2D, i32> = HashMap::new();
        for path in &rural_paths {
            let w = path.width() as i32;
            for pt in path.points() {
                let base = pt.drop_y();
                for dx in -w..=w {
                    for dz in -w..=w {
                        let c = Point2D::new(base.x + dx, base.y + dz);
                        corridor.entry(c).and_modify(|y| *y = (*y).min(pt.y)).or_insert(pt.y);
                    }
                }
            }
        }
        let corridor_pts: HashSet<Point3D> = corridor.iter()
            .filter(|(c, _)| !matches!(
                editor.world().get_claim(**c),
                Some(crate::generator::BuildClaim::Structure(_)
                    | crate::generator::BuildClaim::Building(_)
                    | crate::generator::BuildClaim::Wall)
            ))
            .map(|(c, &y)| Point3D::new(c.x, y, c.y))
            .collect();
        force_height(editor, &corridor_pts, false).await;
        build_paths_merged(&*editor, &data, &rural_paths, &mut rng).await;
        for path in &rural_paths {
            let centre: HashSet<Point2D> = path.points().iter().map(|p| p.drop_y()).collect();
            let mut paved = crate::geometry::get_surrounding_set(&centre, path.width().saturating_sub(1));
            paved.extend(centre);
            for c in paved {
                editor.world_mut().claim(c, crate::generator::BuildClaim::Path(crate::generator::paths::PathType::Road));
            }
        }
    }
    println!("Rural roads: {} segments", rural_paths.len());

    // ── R3: paint rural production areas (after the roads) ────────────────
    for p in &placed_rural {
        let Some(painter) = &p.painter else { continue };
        let Some(district) = editor.world().districts.get(&p.district).cloned() else { continue };
        paint_production_area_for(&district, painter, &p.resource, &p.structure, &data, editor, &mut rng).await;
    }


    // ---- Industrial buildings FIRST ----
    // Place a handful of big processing buildings on the flattened ground (no
    // roads yet → sited by flatness). They become the destinations the arterial
    // network connects, plus a `blocked` barrier so nothing — roads, the
    // subdivision, alleys, or houses — ever runs through them. (Fixed set here;
    // the resource chain's `resolve_for_parcels` can supply the real mix later.)
    use crate::generator::BuildClaim;
    use crate::generator::placement::place_urban_buildings;

    let mut ind_counts: HashMap<String, u32> = HashMap::new();
    for b in ["smithy", "mill", "bakery", "carpenter", "tannery", "weaver"] {
        ind_counts.insert(b.to_string(), 1);
    }
    let urban_sds: Vec<_> = editor.world().districts.values()
        .filter(|sd| sd.data.parcel_type == crate::generator::districts::ParcelType::Urban)
        .cloned()
        .collect();
    let urban_sd_refs: Vec<_> = urban_sds.iter().collect();
    let n_before = editor.world().structures.len();
    // Re-skin the industrial NBTs into the settlement's culture palette
    // (their baked `resource_base` blocks → medieval spruce/stone).
    let ind_palette = data.palettes
        .get(&culture.palette_id())
        .expect("industry palette not found").clone();
    if let Err(e) = place_urban_buildings(&urban_sd_refs, &ind_counts, &mut rng, editor, &data, Some(&ind_palette)).await {
        log::warn!("industrial placement failed: {}", e);
    }
    println!(
        "Placed {} / {} industrial buildings",
        editor.world().structures.len() - n_before, ind_counts.values().sum::<u32>(),
    );

    // Footprints → a `blocked` barrier (footprint + margin) and one node per
    // building for the network to connect.
    const IND_MARGIN: i32 = 2;
    let mut ind_footprints: HashMap<u32, Vec<Point2D>> = HashMap::new();
    for &p in &urban {
        if let Some(BuildClaim::Structure(id)) = editor.world().get_claim(p) {
            ind_footprints.entry(id.id).or_default().push(p);
        }
    }
    let building_cells: HashSet<Point2D> = ind_footprints.values().flatten().copied().collect();
    let blocked: HashSet<Point2D> = building_cells.iter()
        .flat_map(|p| {
            (-IND_MARGIN..=IND_MARGIN).flat_map(move |dx| {
                (-IND_MARGIN..=IND_MARGIN).map(move |dz| Point2D::new(p.x + dx, p.y + dz))
            })
        })
        .collect();
    let ind_nodes: Vec<Point3D> = ind_footprints.values()
        .map(|cells| {
            let c = cells.iter().fold(Point2D::ZERO, |a, p| a + *p) / cells.len().max(1) as i32;
            editor.world().add_height(c)
        })
        .collect();

    // Phase 2 — tiered A* road network, connecting the industrial buildings
    // (anchor nodes) and routed around them (the `blocked` barrier).
    let arterial_material = MaterialId::new(arterial_mat.to_string());
    let collector_material = MaterialId::new(collector_mat.to_string());
    // Keep the whole network (not just `.paths`) so the end-of-run town map can
    // overlay the abstract MST/node graph.
    let road_network = build_road_network(
        &*editor, arterial_material, collector_material, true, &ind_nodes, &blocked, 1,
    ).await;
    let paths = road_network.paths.clone();
    println!("Routed {} road segments", paths.len());

    // DEBUG: Phase A merge check — how many of each path's cells coincide
    // with cells already laid by earlier paths? High overlap = routes are
    // merging onto the network instead of crossing it blindly.
    {
        let mut seen: HashSet<Point2D> = HashSet::new();
        for path in &paths {
            let cells: HashSet<Point2D> = path.points().iter().map(|p| p.drop_y()).collect();
            let shared = cells.iter().filter(|c| seen.contains(c)).count();
            println!("  MERGE prio={:?} pts={} shared_with_network={}", path.priority(), cells.len(), shared);
            seen.extend(cells);
        }
    }

    // DEBUG: does the routed path y match the post-flatten heightmap?
    if let Some(path) = paths.first() {
        println!("--- path[0] sample: road_y vs ground_h vs ocean_h ---");
        for p in path.points().iter().take(25) {
            let xz = p.drop_y();
            println!(
                "  ({:>4},{:>4})  road_y={:>3}  ground_h={:>3}  ocean_h={:>3}",
                xz.x, xz.y, p.y,
                editor.world().get_height_at(xz),
                editor.world().get_ocean_floor_height_at(xz),
            );
        }
    }

    // A path's *paved* cells — exactly what `build_paths_merged` lays:
    // centreline ∪ (width-1) ring. Used for block barriers and frontage
    // bands so blocks abut the real road edge (no gap ring).
    let paved = |path: &Path| -> HashSet<Point2D> {
        let centre: HashSet<Point2D> = path.points().iter().map(|p| p.drop_y()).collect();
        let mut cells = crate::geometry::get_surrounding_set(&centre, path.width().saturating_sub(1));
        cells.extend(centre);
        cells
    };

    // Blocks = urban minus the paved main roads and a buffer strip just inside
    // the wall, so houses never butt right up against it. The boundary ring is
    // dilated `WALL_BUFFER` cells inward; the resulting strip is left open (it
    // gets furnished as a green belt / wall-walk by the open-space pass).
    const WALL_BUFFER: i32 = 2;
    let wall_ring: HashSet<Point2D> = urban.iter()
        .filter(|&&c| crate::geometry::CARDINALS_2D.iter().any(|&d| !urban.contains(&(c + d))))
        .copied()
        .collect();
    let mut wall_zone = wall_ring.clone();
    let mut frontier = wall_ring;
    for _ in 0..WALL_BUFFER {
        let mut next: HashSet<Point2D> = HashSet::new();
        for &c in &frontier {
            for d in crate::geometry::CARDINALS_2D {
                let n = c + d;
                if urban.contains(&n) && wall_zone.insert(n) {
                    next.insert(n);
                }
            }
        }
        frontier = next;
    }
    let mut barriers: HashSet<Point2D> = HashSet::new();
    for path in &paths {
        barriers.extend(paved(path));
    }
    barriers.extend(&wall_zone);
    // Industrial buildings (footprint + margin) are barriers too, so blocks —
    // and the subdivision, alleys, and houses inside them — form *around* the
    // buildings, never through them.
    barriers.extend(&blocked);

    // Don't let blocks (and the lots/alleys/houses inside them) span steep
    // terrain. A per-cell cliff test misses a *sustained* slope — a long
    // staircase of 1-block risers passes cell-by-cell yet climbs far. So bar
    // any cell whose local WIN-radius neighbourhood spans more than
    // MAX_LOCAL_RELIEF blocks of height; the flood fill then breaks blocks
    // along slope lines, keeping lots and their lanes on a flat shelf.
    const WIN: i32 = 1; // 3×3 window
    const MAX_LOCAL_RELIEF: i32 = 2;
    let steep: HashSet<Point2D> = urban.iter()
        .filter(|&&c| {
            let (mut lo, mut hi) = (i32::MAX, i32::MIN);
            for dx in -WIN..=WIN {
                for dz in -WIN..=WIN {
                    let n = Point2D::new(c.x + dx, c.y + dz);
                    if !urban.contains(&n) { continue; }
                    let h = editor.world().get_ocean_floor_height_at(n);
                    lo = lo.min(h);
                    hi = hi.max(h);
                }
            }
            hi - lo > MAX_LOCAL_RELIEF
        })
        .copied()
        .collect();
    println!("Marked {} steep cells as barriers", steep.len());
    barriers.extend(&steep);

    let blocks = find_blocks(&urban, &barriers, 12);
    println!("Found {} blocks", blocks.len());

    // All main-road (arterial + collector) paved cells, used to peel a
    // frontage ribbon off each block before subdividing its interior.
    let main_road_cells: HashSet<Point2D> = {
        let mut s = HashSet::new();
        for path in &paths {
            s.extend(paved(path));
        }
        s
    };

    // Per block: first reserve a frontage ribbon — a band one house deep
    // against each main road — so the long arterial/collector-facing edge
    // stays a single continuous lot instead of being chopped into stubs
    // by subdivision. Then subdivide only the interior with tier-3 alleys.
    // BSP cuts span the interior edge-to-edge, so an alley reaches its edge —
    // adjacent (barriers = paved) to either a main road or the ribbon.
    // Deep enough to absorb both the deepest House (depth_range 7..=10) AND
    // the staircase rise of a diagonal frontage (an axis-aligned rect anchored
    // at the slice's interior extreme reaches `rise + depth` into the band).
    const RIBBON_DEPTH: i32 = 14;
    let mut sub_blocks: Vec<HashSet<Point2D>> = Vec::new();
    let mut alley_band: HashSet<Point2D> = HashSet::new();
    let mut ribbon_lot_count = 0usize;
    let mut ribbon_cells: HashSet<Point2D> = HashSet::new(); // DEBUG: all reserved ribbon cells
    for block in &blocks {
        let (mut ribbon_lots, interior) =
            crate::generator::districts::subdivide::reserve_road_ribbon(block, &main_road_cells, RIBBON_DEPTH);
        let (subs, alleys) = crate::generator::districts::subdivide::subdivide_block(&interior, &mut rng, 24);

        // Connect the interior alleys to the main roads by carving through the
        // ribbon, then convert those cells from frontage ribbon to alley.
        let ribbon_union: HashSet<Point2D> = ribbon_lots.iter().flatten().copied().collect();
        let connectors = crate::generator::districts::subdivide::carve_ribbon_connectors(
            &ribbon_union, &alleys, &main_road_cells,
        );
        if !connectors.is_empty() {
            for rp in &mut ribbon_lots { rp.retain(|c| !connectors.contains(c)); }
            ribbon_lots.retain(|rp| !rp.is_empty());
        }

        ribbon_lot_count += ribbon_lots.len();
        for rp in &ribbon_lots { ribbon_cells.extend(rp); }
        sub_blocks.extend(ribbon_lots);
        alley_band.extend(&alleys);
        alley_band.extend(&connectors);
        sub_blocks.extend(subs);
    }
    println!(
        "Subdivided into {} lots ({} road-frontage ribbons), {} subdivider-road cells",
        sub_blocks.len(), ribbon_lot_count, alley_band.len(),
    );

    // Assemble every road into one path list (mains + a synthesised width-1
    // alley path), but DON'T build them yet — we build after the houses so
    // house-foundation earth can't bury the road. Houses are placed first and
    // sit their floor at the level of the road they front (see `road_h`).
    let alley_pts: Vec<Point3D> = alley_band.iter().map(|c| editor.world().add_height(*c)).collect();
    let alley_path = Path::new(alley_pts, 1, MaterialId::new(collector_mat.to_string()), PathPriority::Low);
    let mut all_paths = paths.clone();
    all_paths.push(alley_path);

    // Road-height lookup over the paved band of every road (centreline +
    // width ring, min y on overlap), so a house can pin its floor to the
    // road it fronts. Built from `all_paths` so alley-facing houses get the
    // alley level too.
    let mut road_h: HashMap<Point2D, i32> = HashMap::new();
    for path in &all_paths {
        let w = path.width() as i32;
        for pt in path.points() {
            let base = pt.drop_y();
            for dx in -w..=w {
                for dz in -w..=w {
                    let c = Point2D::new(base.x + dx, base.y + dz);
                    road_h.entry(c).and_modify(|y| *y = (*y).min(pt.y)).or_insert(pt.y);
                }
            }
        }
    }

    // Frontage bands per tier (paved cells, matching the roads we'll build).
    let band = |prio: PathPriority| -> HashSet<Point2D> {
        let mut s = HashSet::new();
        for path in paths.iter().filter(|p| p.priority() == prio) {
            s.extend(paved(path));
        }
        s
    };
    let arterial_band = band(PathPriority::High);
    let collector_band = band(PathPriority::Medium);

    // Build the roads FIRST, then the houses. force_height grades the corridor
    // to the routed road heights, then build_paths_merged lays + melds the
    // surface. We then claim every paved cell as `Path` so the following
    // house foundations' terrain blending skips them (blend_terrain ignores
    // Path claims) — the road can't be buried by foundation earth. The graded
    // corridor is exactly `road_h` (same band, same min-on-overlap height).
    let corridor_pts: HashSet<Point3D> = road_h
        .iter()
        .map(|(c, &y)| Point3D::new(c.x, y, c.y))
        .collect();
    force_height(editor, &corridor_pts, false).await;
    // `build_paths_merged` returns the exact cells where it laid a half-step
    // slab; we raise a house a block over a fronting slab off this set rather
    // than reading the placed road back (the editor cache is keyed by local
    // coords while get_block subtracts the build-area origin, so a read here
    // returns world terrain, not the road).
    let road_slabs: HashSet<Point3D> = build_paths_merged(&*editor, &data, &all_paths, &mut rng).await;
    let slab_y_by_cell: HashMap<Point2D, i32> =
        road_slabs.iter().map(|p| (p.drop_y(), p.y)).collect();

    // Claim every paved road cell so house-foundation terraforming can't
    // touch it (blend_terrain skips `BuildClaim::Path`).
    for path in &all_paths {
        for c in paved(path) {
            editor.world_mut().claim(c, crate::generator::BuildClaim::Path(crate::generator::paths::PathType::Pavement));
        }
    }

    // ---- Phase 4: hierarchical house placement ----
    // Per lot, walk frontage densest-tier first: arterial → collector →
    // subdivider. The lot's single Plot is shared across tiers, so houses
    // placed against the arterial claim the prime frontage and later tiers
    // can't overlap them. Size gradient: houses on roads, cottages on lanes.
    use crate::generator::buildings_v2::{BuildCtx, BuildingContext, Culture, build_house};
    use crate::generator::buildings_v2::roof::RoofStyle;
    use crate::generator::buildings_v2::roof::gable::GablePitch;
    use crate::generator::buildings_v2::footprint::{Footprint, SizeClass};
    use crate::generator::city_houses::{
        frontage_from_roads, plot_from_block, rect_from_frontage,
        synthetic_plot_bounds, SIDE_BUFFER_CELLS,
    };
    use crate::generator::materials::{Palette, PaletteId};
    use crate::geometry::Point2D as P2;

    // Culture for this settlement. Medieval → spruce/stone palette, gable
    // roofs, glass windows, and timber-frame jetties (square_bias 0, so no
    // domes; per-house wood/stone/roof variety via roll_palette below).
    let base_palette: Palette = data.palettes.get(&culture.palette_id())
        .expect("base palette not found").clone();
    let wood_ids: Vec<PaletteId> = vec!["oak".into(), "spruce".into(), "dark_oak".into()];
    let stone_ids: Vec<PaletteId> = vec!["stone_bricks".into(), "cobblestone".into(), "deepslate".into()];
    let roof_ids: Vec<PaletteId> = vec![
        "acacia_wood_roof".into(), "brick_roof".into(), "oak_wood_roof".into(), "red_wood_roof".into(),
    ];
    let roof_styles = culture.roof_styles();

    fn roll_palette(rng: &mut RNG, base: &Palette, data: &LoadedData, woods: &[PaletteId], stones: &[PaletteId], roofs: &[PaletteId]) -> Palette {
        let w = &woods[rng.rand_i32_range(0, woods.len() as i32) as usize];
        let s = &stones[rng.rand_i32_range(0, stones.len() as i32) as usize];
        let r = &roofs[rng.rand_i32_range(0, roofs.len() as i32) as usize];
        base.clone()
            .merged_with(data.palettes.get(w).expect("wood palette not found"))
            .merged_with(data.palettes.get(s).expect("stone palette not found"))
            .merged_with(data.palettes.get(r).expect("roof palette not found"))
    }
    // Densest tier first; size pool per tier (houses on the main roads,
    // cottages on the back lanes).
    // House + Hall on every tier for now (district wealth will refine this
    // later). Slots that can't fit a Hall's wider frontage just skip.
    let tiers: [(&HashSet<Point2D>, &[SizeClass]); 3] = [
        (&arterial_band, &[SizeClass::House, SizeClass::Hall]),
        (&collector_band, &[SizeClass::House, SizeClass::Hall]),
        (&alley_band, &[SizeClass::House, SizeClass::Hall]),
    ];

    let mut total_buildings = 0usize;
    // Per-house NPC anchors + bed-derived population budget, gathered from every
    // house and fed to the town-wide population pass once the town is built.
    let mut town_anchors: Vec<crate::generator::population::HouseAnchors> = Vec::new();
    let mut tier_cells = [0usize; 3];   // frontage cells found per tier
    let mut tier_placed = [0usize; 3];  // houses placed per tier
    let mut tier_fail = [0usize; 3];    // build_house failures per tier
    let mut tier_short = [0usize; 3];   // chains dropped: shorter than min_front
    let mut tier_unfit = [0usize; 3];   // slots skipped: rect didn't fit the lot
    // DEBUG: every cell detected as frontage, per tier, so we can float a
    // marker above it and see what the placement loop actually "sees".
    let mut tier_frontage: [HashSet<Point2D>; 3] = Default::default();
    // Verge cells per main-road tier (arterial, collector): the gap between
    // the road and each house front, which we pave into a forecourt so the
    // unavoidable set-back on a diagonal reads as a shoulder, not bare grass.
    let mut tier_verge: [HashSet<Point2D>; 2] = Default::default();
    for lot in &sub_blocks {
        if lot.is_empty() { continue; }
        let Some(mut plot) = plot_from_block(lot) else { continue; };

        for (ti, (band, pool)) in tiers.iter().enumerate() {
            let min_front = pool.iter().map(|s| *s.front_width_range().start()).min().unwrap_or(0);
            for frontage in frontage_from_roads(lot, band) {
                tier_cells[ti] += frontage.cells.len();
                tier_frontage[ti].extend(&frontage.cells);
                let chain_len = frontage.cells.len() as i32;
                if chain_len < min_front { tier_short[ti] += 1; continue; }
                let mut cursor: i32 = if min_front > 1 { rng.rand_i32_range(0, min_front) } else { 0 };
                // Shallowest depth we'll accept on a slice that can't take the
                // rolled depth — lets diagonal frontage (where an axis-aligned
                // rect overruns the staircased ribbon) still seat a house.
                const MIN_FIT_DEPTH: i32 = 5;
                while cursor + min_front <= chain_len {
                    let size_class = *rng.choose(pool);
                    let fw = rng.rand_i32_range(*size_class.front_width_range().start(), *size_class.front_width_range().end() + 1);
                    let max_depth = rng.rand_i32_range(*size_class.depth_range().start(), *size_class.depth_range().end() + 1);
                    if cursor + fw > chain_len { cursor += 1; continue; }
                    let chain_slice = &frontage.cells[cursor as usize..(cursor + fw) as usize];
                    // Square-frontage bias: with the culture's square chance,
                    // make the house a square (depth = front width) if it fits,
                    // so it gets a dome. Guarded so a 0 bias never draws RNG.
                    // Otherwise pick the deepest depth (down to MIN_FIT_DEPTH)
                    // that fits, shrinking the house to hug a diagonal ribbon.
                    let want_square = culture.square_bias() > 0
                        && rng.percent(culture.square_bias())
                        && plot.is_rect_usable(&rect_from_frontage(chain_slice, frontage.outward, fw));
                    let depth = if want_square {
                        fw
                    } else if let Some(d) = (MIN_FIT_DEPTH..=max_depth).rev()
                        .find(|&d| plot.is_rect_usable(&rect_from_frontage(chain_slice, frontage.outward, d)))
                    {
                        d
                    } else {
                        tier_unfit[ti] += 1; cursor += 1; continue;
                    };
                    let rect = rect_from_frontage(chain_slice, frontage.outward, depth);

                    // Desert keeps a uniform sandstone palette; other cultures
                    // roll wood/stone/roof variants per house for variety.
                    let palette = match culture {
                        Culture::Desert => base_palette.clone(),
                        _ => roll_palette(&mut rng, &base_palette, &data, &wood_ids, &stone_ids, &roof_ids),
                    };
                    let roof_style = roof_styles[rng.rand_i32_range(0, roof_styles.len() as i32) as usize];
                    let plot_bounds = synthetic_plot_bounds(&rect, frontage.outward);
                    let footprint = Footprint::from_rect(rect);
                    // Align the main door with the road it faces: pin the floor
                    // (= door sill) to the height of the *nearest* road cell to
                    // this frontage. Probe outward from every frontage cell and
                    // keep the closest road-height hit.
                    let road_dir = P2::from(frontage.outward);
                    let base_lvl = {
                        let mut best: Option<(i32, i32, P2)> = None; // (dist, height, road cell)
                        for &c in chain_slice {
                            for step in 1..=RIBBON_DEPTH {
                                let probe = c + P2::new(road_dir.x * step, road_dir.y * step);
                                if let Some(&y) = road_h.get(&probe) {
                                    if best.map_or(true, |(bd, _, _)| step < bd) { best = Some((step, y, probe)); }
                                    break;
                                }
                            }
                        }
                        best.map(|(_, y, cell)| {
                            // If the fronting road cell carries a half-step slab,
                            // raise the floor one block above the slab so the door
                            // steps down onto it instead of opening onto a lip.
                            match slab_y_by_cell.get(&cell) {
                                Some(&slab_y) => slab_y + 1,
                                None => y,
                            }
                        })
                    };
                    let mut bctx = BuildingContext::new(culture, size_class, roof_style);
                    bctx.base_y_override = base_lvl;
                    let mut bctx_editor = BuildCtx::new(editor, &data, &palette, &mut rng);
                    match build_house(&mut bctx_editor, footprint, &bctx, plot_bounds).await {
                        Ok(output) => {
                            // Population budget tracks sleeping capacity, not bed
                            // furniture: a double/canopy bed sleeps two. Each
                            // bed-tagged item's capacity is its number of
                            // `part=foot` blocks (the head auto-spawns), min 1.
                            let beds: usize = output
                                .room_plan
                                .rooms
                                .iter()
                                .flat_map(|r| &r.furniture)
                                .filter_map(|f| data.furniture.items.get(&f.name))
                                .filter(|it| it.tags.iter().any(|t| t == "bed"))
                                .map(|it| {
                                    it.blocks
                                        .iter()
                                        .filter(|b| b.block.contains("part=foot"))
                                        .count()
                                        .max(1)
                                })
                                .sum();
                            // Scale capacity so houses feel lived-in, floored at 1.
                            let population =
                                ((beds as f32 * POPULATION_PER_BED).round() as usize).max(1);
                            town_anchors.push(crate::generator::population::HouseAnchors {
                                scenes: output.npc_anchors,
                                population,
                            });
                            plot.mark_rect_used(&rect, SIDE_BUFFER_CELLS);
                            total_buildings += 1;
                            tier_placed[ti] += 1;
                            // Record the verge: from each frontage cell, walk
                            // into the block (−outward) until we reach the
                            // house. On a straight slice this is just the
                            // frontage row; on a diagonal it's the triangular
                            // set-back we want to pave over.
                            if ti < 2 {
                                let road_dir = P2::from(frontage.outward);
                                let into = P2::new(-road_dir.x, -road_dir.y);
                                for &c in chain_slice {
                                    let mut p = c;
                                    let mut guard = 0;
                                    while !rect.contains(p) && guard < 32 {
                                        tier_verge[ti].insert(p);
                                        p = p + into;
                                        guard += 1;
                                    }
                                }
                            }
                            cursor += fw + SIDE_BUFFER_CELLS;
                        }
                        Err(msg) => {
                            tier_fail[ti] += 1;
                            log::warn!("placement build_house failed: {}", msg);
                            cursor += 1;
                        }
                    }
                }
            }
        }
    }
    println!("Placed {} buildings across {} lots", total_buildings, sub_blocks.len());
    println!(
        "Per-tier [frontage cells / placed / failed] — arterial: {}/{}/{}  collector: {}/{}/{}  subdivider: {}/{}/{}",
        tier_cells[0], tier_placed[0], tier_fail[0],
        tier_cells[1], tier_placed[1], tier_fail[1],
        tier_cells[2], tier_placed[2], tier_fail[2],
    );
    println!(
        "Per-tier skips [short-chain / rect-unfit] — arterial: {}/{}  collector: {}/{}  subdivider: {}/{}",
        tier_short[0], tier_unfit[0],
        tier_short[1], tier_unfit[1],
        tier_short[2], tier_unfit[2],
    );

    // Pave the verge: a forecourt of the road's own material in the gap
    // between each main road and its houses, so the diagonal set-back reads
    // as a paved shoulder. Painted at the live ground top (h-1), matching the
    // post-flatten/foundation surface. Arterial verge = stone bricks (its
    // road material), collector verge = cobblestone.
    let verge_blocks = [
        Block { id: arterial_mat.into(), data: None, state: None },
        Block { id: collector_mat.into(), data: None, state: None },
    ];
    let mut verge_total = 0usize;
    for (ti, cells) in tier_verge.iter().enumerate() {
        for c in cells {
            let h = editor.world().get_ocean_floor_height_at(*c);
            editor.place_block(&verge_blocks[ti], Point3D::new(c.x, h - 1, c.y)).await;
            verge_total += 1;
        }
    }
    println!("Paved {} verge cells (arterial {} + collector {})", verge_total, tier_verge[0].len(), tier_verge[1].len());

    // Street lighting: run last, after houses have claimed their cells, so
    // lamps line every road's verge without landing on a building. The city
    // generator picks the lantern type city-wide.
    let city_rect = editor.world().world_rect_2d();
    let city_centre = (city_rect.origin + city_rect.max()) / 2;
    let cold = {
        let n = editor.world().get_surface_biome_at(city_centre);
        let n = n.name();
        n.contains("snowy") || n.contains("frozen") || n.contains("taiga")
    };
    let street_lantern: crate::minecraft::Block = if cold {
        "minecraft:soul_lantern".into()
    } else {
        "minecraft:lantern".into()
    };
    let lamps = crate::generator::paths::place_street_lights(&*editor, &all_paths, &street_lantern).await;
    println!("Placed {} street lamps", lamps.len());

    // Name the roads (layered: landmark → gate/centre → generic) now that all
    // buildings have claimed their cells, then sign the intersections. Runs
    // before the open-space pass; each sign cell is claimed as a path so
    // plazas/parks/etc. won't furnish over it.
    let mut name_rng = RNG::new(seed).derive();
    let road_names = crate::generator::paths::name_roads_layered(
        editor.world(), &road_network.road_labels, &all_paths,
        &editor.world().gate_locations.clone(), culture, &mut name_rng,
    );
    let signs = crate::generator::paths::place_street_signs(
        editor, &all_paths, &road_network.road_labels, &road_names,
    ).await;
    println!("Placed {} street signs", signs.len());

    // ---- Open spaces: furnish the leftover gaps between buildings and roads ----
    // Detect the empty pockets inside the wall and furnish each by type: plazas
    // (paved civic squares), nooks (small ringed gardens), parks (large green
    // commons), and yards (perimeter kitchen gardens).
    let mut place_labels: Vec<(Point2D, String)> = Vec::new();
    // NPC standing-spot scenes harvested from plazas (stage performers, market
    // vendors, onlookers in the crowd). Staffed as fixtures after furnishing,
    // independent of the resident bed budget — a market is busy regardless of
    // how many beds the town has.
    let mut plaza_scenes: Vec<crate::generator::population::AnchorScene> = Vec::new();
    {
        use crate::generator::open_space::{
            detect_regions, furnish_nook, furnish_park, furnish_plaza, furnish_yard, OpenSpaceNames,
            Theme, RegionType,
        };
        let regions = detect_regions(editor.world(), &urban);
        let theme = Theme::for_culture(culture);
        let mut os_rng = rng.derive();
        // Names are picked alongside furnishing so a park is named for the type it
        // was actually built as; `used` keeps every name unique within the town.
        let names = OpenSpaceNames::load();
        let mut used: HashSet<String> = HashSet::new();
        let mut counts = [0usize; 4]; // plaza, nook, park, yard
        for region in &regions {
            match region.region_type() {
                RegionType::Plaza => {
                    let (plaza_type, scenes) = furnish_plaza(&*editor, region, &mut os_rng, &theme).await;
                    plaza_scenes.extend(scenes);
                    if let Some(name) = names.as_ref().and_then(|n| n.name_plaza(plaza_type, culture, &mut os_rng, &mut used)) {
                        place_labels.push((region.centroid(), name));
                    }
                    counts[0] += 1;
                }
                RegionType::Nook => {
                    furnish_nook(&*editor, region, &mut os_rng, &theme).await;
                    counts[1] += 1;
                }
                RegionType::Park => {
                    let park_type = furnish_park(editor, region, &mut os_rng, &theme).await;
                    if let Some(name) = names.as_ref().and_then(|n| n.name_park(park_type, culture, &mut os_rng, &mut used)) {
                        place_labels.push((region.centroid(), name));
                    }
                    counts[2] += 1;
                }
                RegionType::Yard => {
                    furnish_yard(&*editor, region, &mut os_rng, &theme).await;
                    counts[3] += 1;
                }
            }
        }
        println!(
            "Furnished open spaces — plaza {} nook {} park {} yard {}",
            counts[0], counts[1], counts[2], counts[3],
        );
    }

    // ---- Plaza fixtures: staff every harvested plaza scene ----
    // Stage performers, market vendors, and onlookers are fixtures like the
    // industrial workers below — always placed, independent of the resident bed
    // budget. Each scene already carries its own position, facing, dialogue key,
    // and bubble volume (criers/performers yell), so we just hand them a roster
    // and staff them all. Live-only: no-op offline.
    if !plaza_scenes.is_empty() {
        use crate::generator::population::{build_roster, populate_npcs, NpcData};
        match NpcData::load() {
            Ok(npc_data) => {
                let budget = plaza_scenes.len();
                let roster = build_roster(budget, culture, &npc_data, &mut rng.derive());
                match populate_npcs(editor, plaza_scenes, roster, budget, &npc_data, &mut rng).await {
                    Ok(staffed) => println!("Staffed {} plaza NPCs", staffed),
                    Err(e) => log::warn!("plaza staffing failed: {e}"),
                }
            }
            Err(e) => log::warn!("could not load npcs.yaml for plaza staffing: {e}"),
        }
    }

    // ---- Population: size the resident crowd to beds, scatter it town-wide ----
    // Each house's budget is max(1, beds); the town total is their sum. The
    // town-wide draw seeds one resident per house, then fills the rest weighted
    // by anchor weight, halving a house's weights each time it gains a resident
    // so the crowd spreads instead of clustering. Live-only: no-op offline.
    {
        use crate::generator::population::{populate_town, NpcData};
        let budget: usize = town_anchors.iter().map(|h| h.population).sum();
        let candidate_anchors: usize = town_anchors.iter().map(|h| h.scenes.len()).sum();
        println!(
            "Population target: {} residents across {} houses ({} candidate anchors)",
            budget,
            town_anchors.len(),
            candidate_anchors,
        );
        match NpcData::load() {
            Ok(data) => match populate_town(editor, town_anchors, culture, &data, &mut rng).await {
                Ok(placed) => println!("Populated {} NPCs", placed),
                Err(e) => log::warn!("NPC population failed: {e}"),
            },
            Err(e) => log::warn!("failed to load npcs.yaml; skipping NPC population: {e}"),
        }
    }

    // ---- Industrial worker fixtures ----
    // Staff every industrial shop with exactly one worker NPC standing just
    // outside it, facing the workshop, wearing the trade outfit that matches its
    // type (a smith at the smithy, a shepherd at the weaver, ...). These are
    // fixtures: always placed, independent of the resident budget above. The NBT
    // interiors are opaque, so the worker stands on a clear ground cell at the
    // footprint edge — never inside, never on a road or another building.
    {
        use crate::generator::npc::Profession;
        use crate::generator::population::{build_roster, populate_npcs, AnchorScene, NpcData};

        // type name -> trade outfit. smithy/carpenter roll across a small set so
        // shops of the same kind don't all wear the identical profession.
        let mut worker_rng = rng.derive();
        let profession_for = |kind: &str, rng: &mut RNG| -> Option<Profession> {
            Some(match kind {
                "smithy" => *rng.choose(&[
                    Profession::Weaponsmith, Profession::Toolsmith, Profession::Armorer,
                ]),
                "mill" => Profession::Farmer,
                "bakery" => Profession::Butcher, // no baker profession; closest food trade
                "carpenter" => *rng.choose(&[Profession::Fletcher, Profession::Mason]),
                "tannery" => Profession::Leatherworker,
                "weaver" => Profession::Shepherd,
                _ => return None,
            })
        };

        // Re-scan the claim map: industrial buildings are the only `Structure`
        // claims (wall towers claim `Wall`), and claims persist, so this recovers
        // each building's footprint + type without threading state out of the
        // early industrial phase. Group cells by instance id.
        let mut footprints: HashMap<u32, (String, Vec<Point2D>)> = HashMap::new();
        for &p in &urban {
            if let Some(BuildClaim::Structure(id)) = editor.world().get_claim(p) {
                footprints
                    .entry(id.id)
                    .or_insert_with(|| (id.structure_type.0.clone(), Vec::new()))
                    .1
                    .push(p);
            }
        }

        // A cell is a usable stand spot only if nothing else claims it — not a
        // road, wall, or another building. `get_claim` returns `None` only out
        // of bounds, so an in-bounds open cell reads as `BuildClaim::None`.
        let is_clear = |c: Point2D| {
            matches!(
                editor.world().get_claim(c),
                Some(BuildClaim::None) | Some(BuildClaim::Nature)
            )
        };
        let road_side = |c: Point2D| {
            crate::geometry::CARDINALS_2D
                .iter()
                .any(|&d| matches!(editor.world().get_claim(c + d), Some(BuildClaim::Path(_))))
        };

        // Deterministic order over buildings (HashMap iteration isn't stable).
        let mut ids: Vec<u32> = footprints.keys().copied().collect();
        ids.sort_unstable();

        let mut worker_scenes: Vec<AnchorScene> = Vec::new();
        for id in ids {
            let (kind, cells) = &footprints[&id];
            let Some(profession) = profession_for(kind, &mut worker_rng) else { continue };
            let cell_set: HashSet<Point2D> = cells.iter().copied().collect();
            let centroid =
                cells.iter().fold(Point2D::ZERO, |a, p| a + *p) / cells.len().max(1) as i32;

            // Candidate stand cells: cardinally adjacent to the footprint, outside
            // it, in bounds, and clear. Sort for determinism, then prefer a cell
            // that borders a road so the worker reads as standing at the street.
            let mut candidates: Vec<Point2D> = Vec::new();
            let mut seen: HashSet<Point2D> = HashSet::new();
            for &fc in cells {
                for d in crate::geometry::CARDINALS_2D {
                    let c = fc + d;
                    if cell_set.contains(&c) || !editor.world().is_in_bounds_2d(c) || !is_clear(c) {
                        continue;
                    }
                    if seen.insert(c) {
                        candidates.push(c);
                    }
                }
            }
            candidates.sort_unstable_by_key(|c| (c.x, c.y));
            let stand = candidates
                .iter()
                .find(|c| road_side(**c))
                .or_else(|| candidates.first())
                .copied();
            let Some(stand) = stand else {
                log::warn!("no clear stand cell for industrial '{}' (id {})", kind, id);
                continue;
            };

            // Stand on the ground at the cell; face the footprint centroid.
            let y = editor.world().get_ocean_floor_height_at(stand);
            let stand3 = Point3D::new(stand.x, y, stand.y);
            let centre3 = Point3D::new(centroid.x, y, centroid.y);
            let facing = crate::generator::population::yaw_toward(stand3, centre3);
            worker_scenes.push(AnchorScene::worker(stand3, facing, profession));
        }

        if !worker_scenes.is_empty() {
            match NpcData::load() {
                Ok(npc_data) => {
                    // Roster supplies names/dialogue/biome; each scene's slot
                    // overrides the profession, so the roll here is incidental.
                    let worker_roster =
                        build_roster(worker_scenes.len(), culture, &npc_data, &mut rng.derive());
                    let budget = worker_scenes.len();
                    match populate_npcs(editor, worker_scenes, worker_roster, budget, &npc_data, &mut rng).await {
                        Ok(staffed) => println!("Staffed {} industrial workers", staffed),
                        Err(e) => log::warn!("industrial staffing failed: {}", e),
                    }
                }
                Err(e) => log::warn!("could not load npcs.yaml for staffing: {}", e),
            }
        }
    }

    // Top-down town map (SVG) for inspection: footprints + named roads coloured
    // by id + the abstract MST/node overlay, with sign posts marked.
    {
        let svg = crate::generator::paths::render_town_map(
            editor.world(), &urban, &road_network.paths, &road_network.road_labels,
            &road_names, &alley_band, Some(&road_network), &signs, &place_labels,
        );
        std::fs::create_dir_all("output").ok();
        match std::fs::write("output/town.svg", &svg) {
            Ok(()) => println!("Wrote town map to output/town.svg"),
            Err(e) => log::warn!("failed to write town map: {e}"),
        }
        match crate::generator::paths::rasterize_to_png(&svg, "output/town.png") {
            Ok(()) => println!("Wrote town map to output/town.png"),
            Err(e) => log::warn!("failed to render town.png: {e}"),
        }
    }

    editor.flush_buffer().await;
}
