#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use crate::{
        editor::World,
        generator::{
            data::LoadedData,
            districts::{build_wall, generate_parcels, ParcelType, WallType},
            materials::{MaterialId, Placer},
            nbts::{Rotation, StructureType},
            placement::{
                anchor_offset_for_rotation, footprint_dims_for_rotation,
                place_rural_building, place_urban_buildings,
            },
            resource_chain::paint_production_area,
            terrain::log_trees,
        },
        geometry::{Point2D, Point3D},
        http_mod::{GDMCHTTPProvider, HeightMapType},
        minecraft::Block,
        noise::{Seed, RNG},
        util::init_logger,
    };

    /// Change this to any resource building name to place that building in every rural
    /// super-parcel. Useful for quickly eyeballing a single building + its production
    /// area on a flat Minecraft world without changing the resource chain data.
    const OVERRIDE_BUILDING: &str = "iron_mine";

    /// End-to-end rural placement test with a single hardcoded building type.
    /// Identical to `rural_and_urban_placement_with_city_wall` except:
    ///   - No city wall (suited for flat worlds).
    ///   - Every rural super-parcel places `OVERRIDE_BUILDING` instead of the
    ///     resource-chain-assigned building.
    ///   - No urban processing building pass.
    #[tokio::test]
    async fn rural_placement_override_building() {
        init_logger();

        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area().await.expect("Failed to get build area");

        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        generate_parcels(seed, &mut editor).await;

        let data = LoadedData::load().expect("Failed to load generator data");

        let rural_analysis: HashMap<_, _> = editor
            .world()
            .district_analysis_data
            .iter()
            .filter(|(id, _)| {
                editor
                    .world()
                    .districts
                    .get(id)
                    .map(|sd| sd.data.parcel_type == ParcelType::Rural)
                    .unwrap_or(false)
            })
            .map(|(id, analysis)| (*id, analysis.clone()))
            .collect();

        let result = data
            .resource_registry
            .resolve_for_parcels(&rural_analysis, &mut rng);

        let mut sd_ids: Vec<_> = result.parcel_assignments.keys().cloned().collect();
        sd_ids.sort_by_key(|id| id.0);

        let structure_type = StructureType(OVERRIDE_BUILDING.to_string());
        let Some(structure) = data.structures.get(&structure_type).cloned() else {
            log::error!("OVERRIDE_BUILDING '{}' not found in loaded structures", OVERRIDE_BUILDING);
            return;
        };

        // Resolve the painter from the override building's own gather recipe, not from
        // the parcel assignment (which would use the resource-chain's painter instead).
        let override_painter: Option<String> = data.resource_registry.recipes()
            .values()
            .find(|r| r.inputs.is_empty() && r.building == OVERRIDE_BUILDING)
            .and_then(|r| r.production_painter.clone());

        let mut placed_count = 0usize;
        for sd_id in &sd_ids {
            let assignment = &result.parcel_assignments[sd_id];

            let Some(district) = editor.world().districts.get(sd_id).cloned() else {
                continue;
            };

            log::info!(
                "Placing '{}' (override) for resource '{}' in super-parcel {:?}",
                OVERRIDE_BUILDING, assignment.primary_resource, sd_id,
            );

            match place_rural_building(&district, &structure, &mut rng, &mut editor, &data).await {
                Ok(()) => {
                    placed_count += 1;
                    if let Some(painter) = &override_painter {
                        paint_production_area(&district, painter, &data, &mut editor, &mut rng).await;
                    }
                }
                Err(e) => log::warn!(
                    "Failed to place '{}' in super-parcel {:?}: {}",
                    OVERRIDE_BUILDING, sd_id, e
                ),
            }
        }
        log::info!(
            "Placed {} of {} rural buildings (override: '{}')",
            placed_count, sd_ids.len(), OVERRIDE_BUILDING
        );

        editor.flush_buffer().await;
    }

    /// Integration eyeball: urban industrial buildings should land ON the road
    /// network. Order: parcels → force an urban core → wall+gates → flatten →
    /// tiered A* roads → claim `Path` → `place_urban_buildings` (candidates now
    /// seeded from road-adjacent cells, so they front the streets by construction).
    ///
    /// Buildings are a guaranteed synthetic processing mix (forcing the urban core
    /// starves rural supply, so the real resolved count is often tiny); the real
    /// resolved processing-building count is printed for reference.
    #[tokio::test]
    async fn urban_industrial_follows_roads() {
        use std::collections::HashSet;

        use crate::data::Loadable;
        use crate::generator::BuildClaim;
        use crate::generator::districts::{ParcelAnalysis, DistrictID};
        use crate::generator::materials::Material;
        use crate::generator::nbts::Structure;
        use crate::generator::paths::{build_paths_merged, build_road_network, Path, PathType};
        use crate::generator::terrain::{flatten_urban_area, force_height};

        init_logger();

        let seed = Seed(12345);
        let mut rng = RNG::new(seed);
        let mut rng2 = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        generate_parcels(seed, &mut editor).await;

        // EVAL AID (test-only): force a contiguous ~4-parcel urban core, since
        // the live classifier often collapses to a single urban parcel — too
        // degenerate to grow a road network. (Mirrors `districts::hierarchical_roads`.)
        {
            const TARGET_URBAN: usize = 4;
            let mut info: Vec<(DistrictID, Point2D, bool)> = editor.world().districts.iter()
                .filter(|(_, sd)| sd.data.parcel_type != ParcelType::OffLimits)
                .map(|(id, sd)| {
                    let pts = &sd.data.points_2d;
                    let c = pts.iter().fold(Point2D::ZERO, |a, p| a + *p) / pts.len().max(1) as i32;
                    (*id, c, sd.data.parcel_type == ParcelType::Urban)
                })
                .collect();
            let anchor = info.iter().find(|(_, _, u)| *u).map(|(_, c, _)| *c)
                .or_else(|| info.first().map(|(_, c, _)| *c));
            if let Some(anchor) = anchor {
                info.sort_by_key(|(_, c, _)| c.distance_squared(&anchor));
                for (id, _, _) in info.iter().take(TARGET_URBAN) {
                    editor.world_mut().districts.get_mut(id).unwrap().data.parcel_type = ParcelType::Urban;
                }
            }
        }

        let data = LoadedData::load().expect("Failed to load generator data");

        // Wall + gates — gates seed the collector tier of the network.
        let materials = Material::load().expect("Failed to load materials");
        let structures = Structure::load().expect("Failed to load structures");
        let wall_material = MaterialId::new("stone_bricks".to_string());
        let mut placer = Placer::new(&materials, &mut rng);
        build_wall(
            &editor.world().get_urban_points(), &mut editor, &mut rng2,
            &mut placer, &wall_material, &structures, WallType::Standard,
        ).await;
        drop(placer);

        let n_urban = editor.world().districts.values()
            .filter(|sd| sd.data.parcel_type == ParcelType::Urban).count();
        println!("URBAN super-parcels: {} | gates: {}", n_urban, editor.world().gate_locations.len());

        // Flatten, then route the tiered network over the gentled terrain.
        let urban = editor.world().get_urban_points();
        flatten_urban_area(&mut editor, &urban, 16, 12, true).await;

        // --- Place the industrial buildings FIRST, on good flattened ground. With
        // no roads yet, `road_bonus` is 0 — these big buildings are sited by
        // flatness, not road frontage. They become the destinations the network
        // then connects. ---
        let rural_ids: Vec<DistrictID> = editor.world().districts.iter()
            .filter(|(_, sd)| sd.data.parcel_type == ParcelType::Rural)
            .map(|(id, _)| *id)
            .collect();
        let rural_analysis: HashMap<DistrictID, ParcelAnalysis> = rural_ids.iter()
            .filter_map(|id| editor.world().district_analysis_data.get(id).map(|a| (*id, a.clone())))
            .collect();
        let result = data.resource_registry.resolve_for_parcels(&rural_analysis, &mut rng);
        println!(
            "Resolved processing buildings: {} types, {} total",
            result.processing_buildings.len(),
            result.processing_buildings.values().sum::<u32>(),
        );

        // Guarantee something to eyeball: top up to a fixed mix if resolution is thin.
        // A small, realistic handful of big industrial buildings — these are
        // landmarks, not a crowd, so the network connecting them stays legible
        // (and routable).
        let mut counts: HashMap<String, u32> = result.processing_buildings.clone();
        if counts.values().sum::<u32>() < 4 {
            for b in ["smithy", "mill", "bakery", "carpenter"] {
                *counts.entry(b.to_string()).or_insert(0) += 1;
            }
        }
        let want: u32 = counts.values().sum();
        println!("Placing {} industrial buildings (roads will connect them)", want);

        let urban_districts: Vec<_> = editor.world().districts.values()
            .filter(|sd| sd.data.parcel_type == ParcelType::Urban)
            .cloned()
            .collect();
        let urban_refs: Vec<_> = urban_districts.iter().collect();

        let before = editor.world().structures.len();
        if let Err(e) = place_urban_buildings(&urban_refs, &counts, &mut rng, &mut editor, &data, None).await {
            log::warn!("Urban industrial placement failed: {}", e);
        }
        let placed = editor.world().structures.len() - before;

        // --- Derive routing inputs from the placed buildings: a `blocked` barrier
        // (every footprint cell, expanded by a margin so roads keep off the walls)
        // and one node per building (its footprint centroid). The centroid sits
        // *inside* the building, so `build_road_network` relocates each node to the
        // nearest clear cell before routing. ---
        const BLOCK_MARGIN: i32 = 2;
        let mut footprint_by_id: HashMap<u32, Vec<Point2D>> = HashMap::new();
        for &p in &urban {
            if let Some(BuildClaim::Structure(id)) = editor.world().get_claim(p) {
                footprint_by_id.entry(id.id).or_default().push(p);
            }
        }
        let structure_cells: HashSet<Point2D> = footprint_by_id.values().flatten().copied().collect();
        let blocked: HashSet<Point2D> = structure_cells.iter()
            .flat_map(|p| {
                (-BLOCK_MARGIN..=BLOCK_MARGIN).flat_map(move |dx| {
                    (-BLOCK_MARGIN..=BLOCK_MARGIN).map(move |dz| Point2D::new(p.x + dx, p.y + dz))
                })
            })
            .collect();

        let anchor_nodes: Vec<Point3D> = footprint_by_id.values()
            .map(|cells| {
                let c = cells.iter().fold(Point2D::ZERO, |a, p| a + *p) / cells.len().max(1) as i32;
                editor.world().add_height(c)
            })
            .collect();
        println!("Placed {} / {} industrial buildings; {} building nodes for the network", placed, want, anchor_nodes.len());

        // --- Route the network connecting the buildings, forbidden from crossing any
        // footprint (the `blocked` barrier). ---
        let arterial_material = MaterialId::new("stone_bricks".to_string());
        let collector_material = MaterialId::new("cobblestone".to_string());
        let paths = build_road_network(
            &editor, arterial_material, collector_material, true, &anchor_nodes, &blocked, 1,
        ).await.paths;
        println!("Routed {} road segments", paths.len());

        // Realize: grade the corridor to routed heights, lay + meld the surface,
        // then claim each paved cell as `Path`. `build_paths_merged` refuses to lay
        // surface on a Structure cell, so we mirror that here: never claim a
        // building footprint cell as road.
        let paved = |path: &Path| -> HashSet<Point2D> {
            let centre: HashSet<Point2D> = path.points().iter().map(|p| p.drop_y()).collect();
            let mut cells = crate::geometry::get_surrounding_set(&centre, path.width().saturating_sub(1));
            cells.extend(centre);
            cells
        };
        let corridor_pts: HashSet<Point3D> = paths.iter().flat_map(|p| p.points().iter().copied()).collect();
        force_height(&mut editor, &corridor_pts, false).await;
        build_paths_merged(&editor, &data, &paths, &mut rng).await;
        let mut claimed_road: HashSet<Point2D> = HashSet::new();
        for path in &paths {
            for c in paved(path) {
                if !structure_cells.contains(&c) {
                    claimed_road.insert(c);
                    editor.world_mut().claim(c, BuildClaim::Path(PathType::Pavement));
                }
            }
        }

        editor.flush_buffer().await;

        // --- Verify: no routed centreline passes through a building footprint (the
        // hard "roads can't route through buildings" constraint), and no footprint
        // cell was paved/claimed as road. Widened shoulders that graze a footprint
        // are skipped by build_paths_merged, so the surface guarantee holds. ---
        let centreline_through = paths.iter()
            .flat_map(|p| p.points().iter().map(|q| q.drop_y()))
            .filter(|c| structure_cells.contains(c))
            .count();
        println!(
            "VERIFY: {} centreline cells through a footprint (want 0) | {} road cells claimed | {} buildings connected",
            centreline_through, claimed_road.len(), footprint_by_id.len(),
        );
        assert_eq!(claimed_road.intersection(&structure_cells).count(), 0, "road claimed on a building footprint");
        assert_eq!(centreline_through, 0, "a road centreline routed through {} building cells", centreline_through);
    }

    /// End-to-end integration test that exercises every major settlement system
    /// in pipeline order against a live Minecraft server:
    ///   parcels → log trees → terraform → wall → main roads →
    ///   resource chain → secondary roads → buildings_v2
    #[tokio::test]
    async fn full_settlement_pipeline() {
        use crate::data::Loadable;
        use crate::generator::BuildClaim;
        use crate::generator::districts::{ParcelAnalysis, DistrictID};
        use crate::generator::materials::Material;
        use crate::generator::nbts::Structure;
        use crate::generator::paths::{
            build_paths_merged, build_road_network, find_blocks, Path, PathPriority,
        };
        use crate::generator::terrain::{flatten_urban_area, force_height};

        init_logger();
        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();
        let mut rng = RNG::new(32);

        generate_parcels(rng.next_i64().into(), &mut editor).await;
        let urban = editor.world().get_urban_points();

        log_trees(&mut editor, urban.clone()).await;
        flatten_urban_area(&mut editor, &urban, 16, 12, true).await;

        let data = LoadedData::load().expect("Failed to load generator data");
        // Wall + gates — gates seed the collector tier of the network.
        let materials = Material::load().expect("Failed to load materials");
        let structures = Structure::load().expect("Failed to load structures");
        let wall_material = MaterialId::new("stone_bricks".to_string());
        let mut wall_rng = rng.derive();
        let mut placer = Placer::new(&materials, &mut rng);
        build_wall(
            &urban, &mut editor, &mut wall_rng,
            &mut placer, &wall_material, &structures, WallType::Standard,
        ).await;
        drop(placer);

        let n_urban = editor.world().districts.values()
            .filter(|sd| sd.data.parcel_type == ParcelType::Urban).count();
        println!("URBAN super-parcels: {} | gates: {}", n_urban, editor.world().gate_locations.len());
        
        let rural_ids: Vec<DistrictID> = editor.world().districts.iter()
            .filter(|(_, sd)| sd.data.parcel_type == ParcelType::Rural)
            .map(|(id, _)| *id)
            .collect();
        let rural_analysis: HashMap<DistrictID, ParcelAnalysis> = rural_ids.iter()
            .filter_map(|id| editor.world().district_analysis_data.get(id).map(|a| (*id, a.clone())))
            .collect();
        let result = data.resource_registry.resolve_for_parcels(&rural_analysis, &mut rng);
        println!(
            "Resolved processing buildings: {} types, {} total",
            result.processing_buildings.len(),
            result.processing_buildings.values().sum::<u32>(),
        );

         // One placement per rural super-parcel — matches the resource chain's
        // `parcel_assignments`, which is keyed by `DistrictID`.
        let mut sd_ids: Vec<_> = result.parcel_assignments.keys().cloned().collect();
        sd_ids.sort_by_key(|id| id.0);

        let mut placed_count = 0usize;
        for sd_id in &sd_ids {
            let assignment = &result.parcel_assignments[sd_id];

            let Some(district) = editor.world().districts.get(sd_id).cloned() else {
                continue;
            };

            let structure_type = StructureType(assignment.building.clone());
            let Some(structure) = data.structures.get(&structure_type).cloned() else {
                log::warn!(
                    "No structure found for building '{}' assigned to super-parcel {:?}",
                    assignment.building,
                    sd_id
                );
                continue;
            };

            log::info!(
                "Placing '{}' (size {:?}) for resource '{}' in super-parcel {:?} (area {} cells)",
                assignment.building,
                structure.size_xz,
                assignment.primary_resource,
                sd_id,
                district.data.points_2d.len(),
            );

            match place_rural_building(&district, &structure, &mut rng, &mut editor, &data).await {
                Ok(()) => {
                    placed_count += 1;
                    if let Some(painter) = &assignment.production_painter {
                        paint_production_area(&district, painter, &data, &mut editor, &mut rng).await;
                    }
                }
                Err(e) => log::warn!(
                    "Failed to place '{}' in super-parcel {:?}: {}",
                    assignment.building,
                    sd_id,
                    e
                ),
            }
        }
        log::info!(
            "Placed {} of {} rural resource buildings",
            placed_count,
            sd_ids.len()
        );

        let counts: HashMap<String, u32> = result.processing_buildings.clone();
        let want: u32 = counts.values().sum();
        let mut breakdown: Vec<(&String, &u32)> = counts.iter().collect();
        breakdown.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        println!(
            "Placing {} industrial buildings (roads will connect them): {}",
            want,
            breakdown.iter().map(|(n, c)| format!("{}×{}", c, n)).collect::<Vec<_>>().join(", "),
        );

        let urban_districts: Vec<_> = editor.world().districts.values()
            .filter(|sd| sd.data.parcel_type == ParcelType::Urban)
            .cloned()
            .collect();
        let urban_refs: Vec<_> = urban_districts.iter().collect();

        let before = editor.world().structures.len();
        // Re-skin the industrial NBTs into the settlement's culture palette
        // (their baked `resource_base` blocks → desert sandstone).
        let ind_palette = data.palettes
            .get(&crate::generator::buildings_v2::Culture::Desert.palette_id())
            .expect("industry palette not found").clone();
        if let Err(e) = place_urban_buildings(&urban_refs, &counts, &mut rng, &mut editor, &data, Some(&ind_palette)).await {
            log::warn!("Urban industrial placement failed: {}", e);
        }
        let placed = editor.world().structures.len() - before;

        // start of road/building stuff
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
        // (anchor nodes) and routed around them (the `blocked` barrier). One
        // material for every tier — tiers differ by width, not surface block.
        let road_material = MaterialId::new("cobblestone".to_string());
        let road_network = build_road_network(
            &editor, road_material.clone(), road_material, true, &ind_nodes, &blocked, 1,
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

        // Blocks = urban minus the paved main roads and the wall.
        let wall: HashSet<Point2D> = urban.iter()
            .filter(|&&c| crate::geometry::CARDINALS_2D.iter().any(|&d| !urban.contains(&(c + d))))
            .copied()
            .collect();
        let mut barriers: HashSet<Point2D> = HashSet::new();
        for path in &paths {
            barriers.extend(paved(path));
        }
        barriers.extend(&wall);
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
        let mut ribbon_parcel_count = 0usize;
        let mut ribbon_cells: HashSet<Point2D> = HashSet::new(); // DEBUG: all reserved ribbon cells
        for block in &blocks {
            let (ribbon_parcels, interior) =
                crate::generator::districts::subdivide::reserve_road_ribbon(block, &main_road_cells, RIBBON_DEPTH);
            let (subs, alleys) = crate::generator::districts::subdivide::subdivide_block(&interior, &mut rng, 24);

            // The BSP cut lines are the alley *corridors* between back lots. We
            // don't pave them here — they're connected to the main roads and laid
            // down AFTER houses (see `connect_alleys_to_roads` below), so the
            // connector can route around placed buildings to actually reach a road.
            ribbon_parcel_count += ribbon_parcels.len();
            for rp in &ribbon_parcels { ribbon_cells.extend(rp); }
            sub_blocks.extend(ribbon_parcels);
            alley_band.extend(&alleys);
            sub_blocks.extend(subs);
        }
        println!(
            "Subdivided into {} parcels ({} road-frontage ribbons), {} alley-corridor cells",
            sub_blocks.len(), ribbon_parcel_count, alley_band.len(),
        );

        // Build only the MAIN roads now (houses follow, then alleys last). Houses
        // pin their floor to the road they front (`road_h`); alley-fronting houses
        // pin to the alley corridor's ground height, since the corridor cells are
        // known even though they aren't paved yet.
        let all_paths = paths.clone();

        // Road-height lookup: main-road paved band (centreline + width ring, min y
        // on overlap) for road-fronting houses, plus the alley corridor cells at
        // ground height for alley-fronting houses.
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
        for &c in &alley_band {
            road_h.entry(c).or_insert_with(|| editor.world().add_height(c).y);
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
        force_height(&mut editor, &corridor_pts, false).await;
        // `build_paths_merged` returns the exact cells where it laid a half-step
        // slab (the grade lips). We drive door-floor raising and threshold clearing
        // off this set instead of reading the placed road back out of the editor
        // (whose block cache is keyed by local coords while get_block subtracts the
        // build-area origin — reading there returns world terrain, not the road).
        let road_slabs: HashSet<Point3D> = build_paths_merged(&editor, &data, &all_paths, &mut rng).await;
        // Per-cell slab height for the alignment probe.
        let slab_y_by_cell: HashMap<Point2D, i32> =
            road_slabs.iter().map(|p| (p.drop_y(), p.y)).collect();

        // Claim every paved road cell so house-foundation terraforming can't
        // touch it (blend_terrain skips `BuildClaim::Path`).
        for path in &all_paths {
            for c in paved(path) {
                editor.world_mut().claim(c, crate::generator::BuildClaim::Path(crate::generator::paths::PathType::Pavement));
            }
        }

        // Road-label viz: each named road (stroke) gets a wool colour, floated
        // two blocks above the debug tier markers. Segments grouped into one road
        // share a colour, so a long avenue reads as one ribbon even where side
        // streets branch. Alleys (no road_id) are unlabelled. The `label` map is
        // computed unconditionally because the SVG town map below reuses it; only
        // the in-world wool placement is gated.
        // One label per cell so roads sharing pavement (a collector merged onto an
        // arterial) don't braid two colors over one street: the higher tier wins
        // the shared cells, and a road only shows its own colour where it diverges.
        // Thickened to each road's width so it reads as a solid ribbon from the air.
        let prio_rank = |p: PathPriority| match p {
            PathPriority::High => 2u8,
            PathPriority::Medium => 1,
            PathPriority::Low => 0,
        };
        // Approximate per-road footprint, so on a shared trunk the bigger road
        // wins the cells (the main avenue keeps its colour; a smaller road only
        // shows where it diverges) instead of two colours weaving.
        let mut rid_size: HashMap<u32, usize> = HashMap::new();
        for path in &all_paths {
            if let Some(rid) = path.road_id() {
                let span = (2 * (path.width() as usize - 1) + 1).pow(2);
                *rid_size.entry(rid).or_insert(0) += path.points().len() * span;
            }
        }
        let mut label: HashMap<P2, (u8, usize, u32, i32)> = HashMap::new(); // cell -> (tier, road_size, road_id, y)
        for path in &all_paths {
            let Some(rid) = path.road_id() else { continue; };
            let key = (prio_rank(path.priority()), *rid_size.get(&rid).unwrap_or(&0));
            let r = path.width() as i32 - 1;
            for p in path.points() {
                for dx in -r..=r {
                    for dz in -r..=r {
                        let c = P2::new(p.x + dx, p.z + dz);
                        let e = label.entry(c).or_insert((key.0, key.1, rid, p.y));
                        if (key.0, key.1) >= (e.0, e.1) {
                            *e = (key.0, key.1, rid, p.y);
                        }
                    }
                }
            }
        }
        // In-world wool ribbons — disabled for now. Flip `DEBUG_ROAD_WOOL` to
        // restore them; the SVG town map is unaffected either way.
        const DEBUG_ROAD_WOOL: bool = false;
        if DEBUG_ROAD_WOOL {
            const WOOL_COLORS: [&str; 16] = [
                "white_wool", "orange_wool", "magenta_wool", "light_blue_wool",
                "yellow_wool", "lime_wool", "pink_wool", "gray_wool",
                "light_gray_wool", "cyan_wool", "purple_wool", "blue_wool",
                "brown_wool", "green_wool", "red_wool", "black_wool",
            ];
            const LABEL_HEIGHT: i32 = 22; // debug tier markers sit at +20.
            let mut named_roads: HashSet<u32> = HashSet::new();
            for (c, (_, _, rid, y)) in &label {
                named_roads.insert(*rid);
                let wool: Block = WOOL_COLORS[*rid as usize % WOOL_COLORS.len()].into();
                editor.place_block_forced(&wool, Point3D::new(c.x, y + LABEL_HEIGHT, c.y)).await;
            }
            println!("Labelled {} named roads", named_roads.len());
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

        // Culture for this settlement. Desert → sandstone palette, flat roofs,
        // and domed square rects (see buildings_v2::roof::dome).
        let culture = Culture::Desert;
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
        let tiers: [(&HashSet<Point2D>, &[SizeClass]); 3] = [
            (&arterial_band, &[SizeClass::House]),
            (&collector_band, &[SizeClass::House]),
            (&alley_band, &[SizeClass::Cottage]),
        ];

        let mut total_buildings = 0usize;
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
        // Cells just outside each placed door at floor level — collected during
        // placement and checked for a road-slab lip afterward (we can't touch
        // `editor` inside the loop; it's borrowed by the build context).
        let mut door_thresholds: Vec<Point3D> = Vec::new();
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
                        // (= door sill) to the routed height of the *nearest* road cell.
                        // Probe outward from every frontage cell and keep the closest
                        // road-height hit. (Uses road_h — the routed integer height —
                        // NOT a live block read: the editor's block cache is keyed by
                        // local coords while get_block subtracts the build-area origin,
                        // so reading placed road blocks here returns world terrain, not
                        // the road. See note below on the slab detection.)
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
                                    Some(&slab_y) => {
                                        let base = slab_y + 1;
                                        println!("[door-align] facing {:?} road_cell={:?} road_h={} slab_y={} -> base_y={} (raised)",
                                            frontage.outward, (cell.x, cell.y), y, slab_y, base);
                                        base
                                    }
                                    None => y,
                                }
                            })
                        };
                        let mut bctx = BuildingContext::new(culture, size_class, roof_style);
                        bctx.base_y_override = base_lvl;
                        let mut bctx_editor = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
                        match build_house(&mut bctx_editor, footprint, &bctx, plot_bounds).await {
                            Ok(_) => {
                                plot.mark_rect_used(&rect, SIDE_BUFFER_CELLS);
                                total_buildings += 1;
                                tier_placed[ti] += 1;
                                // Collect door-threshold cells: the strip just outside
                                // the house's road-facing wall at floor level, where a
                                // road slab leaves a half-block lip in the doorway. We
                                // clear those slabs after the loop (editor is borrowed
                                // here by the build context).
                                if let Some(sill) = base_lvl {
                                    let rd = P2::from(frontage.outward);
                                    let (mn, mx) = (rect.min(), rect.max());
                                    let front: Vec<P2> = if rd.x != 0 {
                                        let fx = if rd.x > 0 { mx.x } else { mn.x };
                                        (mn.y..=mx.y).map(|z| P2::new(fx, z)).collect()
                                    } else {
                                        let fz = if rd.y > 0 { mx.y } else { mn.y };
                                        (mn.x..=mx.x).map(|x| P2::new(x, fz)).collect()
                                    };
                                    for fc in front {
                                        for step in 1..=2 {
                                            let c = fc + P2::new(rd.x * step, rd.y * step);
                                            door_thresholds.push(Point3D::new(c.x, sill, c.y));
                                        }
                                    }
                                }
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

        // Clear road-slab lips at the collected door thresholds: a slab sitting at
        // or just above the sill reads as an awkward half-step into the doorway;
        // replacing it with air keeps the threshold walkable. We check only the sill
        // and one above — NOT below, since a fronting slab the house was raised over
        // legitimately sits at sill-1 (the door steps down onto it). Driven off the
        // exact slab cells `build_paths_merged` reported, so it never touches a
        // house's own door-ramp blocks.
        let mut cleared_door_slabs = 0usize;
        for p in &door_thresholds {
            for dy in 0..=1 {
                let q = Point3D::new(p.x, p.y + dy, p.z);
                if road_slabs.contains(&q) {
                    editor.place_block_forced(&"air".into(), q).await;
                    cleared_door_slabs += 1;
                }
            }
        }
        println!("Cleared {} road-slab lips at door thresholds", cleared_door_slabs);

        // Pave the verge: a forecourt of the road's material in the gap between
        // each main road and its houses, so the diagonal set-back reads as a paved
        // shoulder. Painted at the live ground top (h-1), matching the
        // post-flatten/foundation surface. One material for every tier.
        let verge_blocks = [
            Block { id: "cobblestone".into(), data: None, state: None },
            Block { id: "cobblestone".into(), data: None, state: None },
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

        // ---- Alleys, built LAST (around the placed houses) ----
        // The BSP corridors are the back-lot lanes; connect each corridor
        // component to the main-road network through open ground — routing around
        // the now-placed houses — then pave corridors + connectors as one width-1
        // network. Building after houses is what guarantees every alley reaches a
        // road (the old straight-punch connector often dead-ended).
        let open: HashSet<P2> = urban.iter().copied()
            .filter(|c| !matches!(
                editor.world().get_claim(*c),
                Some(crate::generator::BuildClaim::Building(_)
                    | crate::generator::BuildClaim::Structure(_)
                    | crate::generator::BuildClaim::Wall
                    | crate::generator::BuildClaim::Path(_))
            ))
            .collect();
        let connectors = crate::generator::districts::subdivide::connect_alleys_to_roads(
            &alley_band, &open, &main_road_cells,
        );
        let mut full_alleys = alley_band.clone();
        full_alleys.extend(&connectors);
        println!(
            "Alleys: {} corridor + {} connector cells -> {} total",
            alley_band.len(), connectors.len(), full_alleys.len(),
        );
        let alley_pts: Vec<Point3D> = full_alleys.iter().map(|c| editor.world().add_height(*c)).collect();
        let alley_path = Path::new(alley_pts, 1, MaterialId::new("cobblestone".to_string()), PathPriority::Low);
        build_paths_merged(&editor, &data, &[alley_path], &mut rng).await;
        for c in &full_alleys {
            editor.world_mut().claim(*c, crate::generator::BuildClaim::Path(crate::generator::paths::PathType::Pavement));
        }

        // --- Settlement summary ---
        // Road surface = every paved cell of the routed network (mains + alleys)
        // that lands inside the urban area, over total urban cells. Verge
        // forecourts are added on top as paved shoulders.
        let road_cells: HashSet<P2> = all_paths.iter()
            .flat_map(|p| paved(p))
            .chain(tier_verge.iter().flatten().copied())
            .chain(full_alleys.iter().copied())
            .filter(|c| urban.contains(c))
            .collect();
        let road_pct = 100.0 * road_cells.len() as f32 / urban.len().max(1) as f32;
        println!(
            "SUMMARY: {} industrial + {} rural resource buildings, {} houses | \
             road surface {} / {} urban cells ({:.1}%)",
            placed, placed_count, total_buildings,
            road_cells.len(), urban.len(), road_pct,
        );

        // ---- Top-down town map (SVG) ----
        // Layers: grass background, water, building/wall footprints, alleys, then
        // the named road network coloured per road (same 16-hue palette as the
        // in-world wool labels), with each road's id printed at its centroid.
        {
            use std::fmt::Write as _;
            const ROAD_SVG: [&str; 16] = [
                "#e9ecec", "#f9801d", "#c74ebd", "#3ab3da", "#fed83d", "#80c71f",
                "#f38baa", "#474f52", "#9d9d97", "#169c9c", "#8932b8", "#3c44aa",
                "#835432", "#5e7c16", "#b02e26", "#1d1d21",
            ];
            // Bounds from the urban footprint, padded.
            let (mut minx, mut minz, mut maxx, mut maxz) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
            for c in &urban {
                minx = minx.min(c.x); maxx = maxx.max(c.x);
                minz = minz.min(c.y); maxz = maxz.max(c.y);
            }
            let pad = 3;
            minx -= pad; minz -= pad; maxx += pad; maxz += pad;
            let (w, h) = (maxx - minx + 1, maxz - minz + 1);

            let mut svg = String::new();
            let _ = write!(svg,
                "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" \
                 width=\"{}\" height=\"{}\" shape-rendering=\"crispEdges\">\n",
                w * 4, h * 4);
            let _ = write!(svg, "<rect x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\" fill=\"#b9d68a\"/>\n");

            // Base layer: water / footprints / wall / alleys (roads drawn on top).
            for z in minz..=maxz {
                for x in minx..=maxx {
                    let c = P2::new(x, z);
                    if !editor.world().is_in_bounds_2d(c) { continue; }
                    let fill = if full_alleys.contains(&c) {
                        "#b8b8b8"
                    } else {
                        match editor.world().get_claim(c) {
                            Some(crate::generator::BuildClaim::Wall) => "#3a3a3a",
                            Some(crate::generator::BuildClaim::Gate) => "#6a6a6a",
                            Some(crate::generator::BuildClaim::Building(_)
                                | crate::generator::BuildClaim::Structure(_)) => "#d9cfa3",
                            // Pavement: named roads get recoloured on top; this
                            // keeps discarded (unnamed) short roads visible as grey.
                            Some(crate::generator::BuildClaim::Path(_)) => "#c4c4c4",
                            _ if editor.world().is_water(c) => "#4a6fb0",
                            _ => continue,
                        }
                    };
                    let _ = write!(svg, "<rect x=\"{}\" y=\"{}\" width=\"1\" height=\"1\" fill=\"{}\"/>\n",
                        x - minx, z - minz, fill);
                }
            }

            // Road layer + per-road centroid for the id label.
            let mut centroid: HashMap<u32, (i64, i64, i64)> = HashMap::new(); // rid -> (sumx, sumz, count)
            for (c, (_, _, rid, _)) in &label {
                let _ = write!(svg, "<rect x=\"{}\" y=\"{}\" width=\"1\" height=\"1\" fill=\"{}\"/>\n",
                    c.x - minx, c.y - minz, ROAD_SVG[*rid as usize % ROAD_SVG.len()]);
                let e = centroid.entry(*rid).or_insert((0, 0, 0));
                e.0 += (c.x - minx) as i64; e.1 += (c.y - minz) as i64; e.2 += 1;
            }
            for (rid, (sx, sz, n)) in &centroid {
                if *n == 0 { continue; }
                let _ = write!(svg,
                    "<text x=\"{}\" y=\"{}\" font-size=\"7\" font-weight=\"bold\" fill=\"#000\" \
                     stroke=\"#fff\" stroke-width=\"0.4\" paint-order=\"stroke\" text-anchor=\"middle\">{}</text>\n",
                    sx / n, sz / n + 2, rid);
            }

            // Abstract graph overlay: the MST + shortcut edges drawn as straight
            // thin lines between their nodes (the data structure, before A* curved
            // it onto the terrain). Arterial edges thicker; shortcuts dashed.
            let nx = |p: Point3D| p.x - minx;
            let nz = |p: Point3D| p.z - minz;
            for e in &road_network.edges {
                let (pa, pb) = (road_network.nodes[e.a], road_network.nodes[e.b]);
                let (sw, dash) = (
                    if e.arterial { "1.2" } else { "0.6" },
                    if e.shortcut { " stroke-dasharray=\"2,2\"" } else { "" },
                );
                let _ = write!(svg,
                    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#111\" \
                     stroke-width=\"{}\" stroke-opacity=\"0.85\"{}/>\n",
                    nx(pa), nz(pa), nx(pb), nz(pb), sw, dash);
            }
            for (i, p) in road_network.nodes.iter().enumerate() {
                let _ = write!(svg,
                    "<circle cx=\"{}\" cy=\"{}\" r=\"1.6\" fill=\"#111\" stroke=\"#fff\" stroke-width=\"0.4\"/>\n",
                    nx(*p), nz(*p));
                let _ = write!(svg,
                    "<text x=\"{}\" y=\"{}\" font-size=\"4\" fill=\"#fff\" text-anchor=\"middle\">{}</text>\n",
                    nx(*p), nz(*p) + 1, i);
            }
            let _ = write!(svg, "</svg>\n");

            std::fs::create_dir_all("output").ok();
            match std::fs::write("output/town.svg", &svg) {
                Ok(_) => println!("Wrote town map to output/town.svg ({}x{} cells)", w, h),
                Err(e) => println!("Failed to write town.svg: {}", e),
            }
        }

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
        let lamps = crate::generator::paths::place_street_lights(&editor, &all_paths, &street_lantern).await;
        println!("Placed {} street lamps", lamps.len());

        editor.flush_buffer().await;

    }

    #[test]
    fn footprint_dims_no_rotation() {
        assert_eq!(footprint_dims_for_rotation((5, 3), Rotation::None), (5, 3));
        assert_eq!(footprint_dims_for_rotation((5, 3), Rotation::Twice), (5, 3));
    }

    #[test]
    fn footprint_dims_quarter_rotations() {
        assert_eq!(footprint_dims_for_rotation((5, 3), Rotation::Once), (3, 5));
        assert_eq!(footprint_dims_for_rotation((5, 3), Rotation::Thrice), (3, 5));
    }

    #[test]
    fn anchor_offset_table_matches_plan() {
        let size = (5, 3);
        let origin_xz = (1, 2);

        assert_eq!(anchor_offset_for_rotation(size, origin_xz, Rotation::None), (1, 2));
        assert_eq!(anchor_offset_for_rotation(size, origin_xz, Rotation::Once), (0, 1));
        assert_eq!(anchor_offset_for_rotation(size, origin_xz, Rotation::Twice), (3, 0));
        assert_eq!(anchor_offset_for_rotation(size, origin_xz, Rotation::Thrice), (2, 3));
    }

    #[test]
    fn anchor_offset_corner_origin() {
        // Origin at (0,0) — equivalent to "rect min corner is anchor".
        let size = (4, 6);
        let origin_xz = (0, 0);

        assert_eq!(anchor_offset_for_rotation(size, origin_xz, Rotation::None), (0, 0));
        assert_eq!(anchor_offset_for_rotation(size, origin_xz, Rotation::Once), (5, 0));
        assert_eq!(anchor_offset_for_rotation(size, origin_xz, Rotation::Twice), (3, 5));
        assert_eq!(anchor_offset_for_rotation(size, origin_xz, Rotation::Thrice), (0, 3));
    }

    /// End-to-end placement test: generates parcels, resolves rural resource assignments,
    /// places each rural super-parcel's gathering building inside one of its constituent
    /// parcels, then paints the ground by parcel type (Urban/Rural/OffLimits) with
    /// distinct wool colours and marks parcel + super-parcel borders.
    #[tokio::test]
    async fn rural_resource_placement_paints_parcels() {
        init_logger();

        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let height_map = provider
            .get_heightmap(
                build_area.origin.x,
                build_area.origin.z,
                build_area.size.x,
                build_area.size.z,
                HeightMapType::MotionBlockingNoPlants,
            )
            .await
            .expect("Failed to get heightmap");

        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        generate_parcels(seed, &mut editor).await;

        let data = LoadedData::load().expect("Failed to load generator data");

        // Only Rural super-parcels produce raw resources.
        let rural_analysis: HashMap<_, _> = editor
            .world()
            .district_analysis_data
            .iter()
            .filter(|(id, _)| {
                editor
                    .world()
                    .districts
                    .get(id)
                    .map(|sd| sd.data.parcel_type == ParcelType::Rural)
                    .unwrap_or(false)
            })
            .map(|(id, analysis)| (*id, analysis.clone()))
            .collect();

        let result = data
            .resource_registry
            .resolve_for_parcels(&rural_analysis, &mut rng);

        // One placement per rural super-parcel — matches the resource chain's
        // `parcel_assignments`, which is keyed by `DistrictID`.
        let mut sd_ids: Vec<_> = result.parcel_assignments.keys().cloned().collect();
        sd_ids.sort_by_key(|id| id.0);

        let mut placed_count = 0usize;
        for sd_id in &sd_ids {
            let assignment = &result.parcel_assignments[sd_id];

            let Some(district) = editor.world().districts.get(sd_id).cloned() else {
                continue;
            };

            let structure_type = StructureType(assignment.building.clone());
            let Some(structure) = data.structures.get(&structure_type).cloned() else {
                log::warn!(
                    "No structure found for building '{}' assigned to super-parcel {:?}",
                    assignment.building,
                    sd_id
                );
                continue;
            };

            log::info!(
                "Placing '{}' (size {:?}) for resource '{}' in super-parcel {:?} (area {} cells)",
                assignment.building,
                structure.size_xz,
                assignment.primary_resource,
                sd_id,
                district.data.points_2d.len(),
            );

            match place_rural_building(&district, &structure, &mut rng, &mut editor, &data).await {
                Ok(()) => {
                    placed_count += 1;
                    if let Some(painter) = &assignment.production_painter {
                        paint_production_area(&district, painter, &data, &mut editor, &mut rng).await;
                    }
                }
                Err(e) => log::warn!(
                    "Failed to place '{}' in super-parcel {:?}: {}",
                    assignment.building,
                    sd_id,
                    e
                ),
            }
        }
        log::info!(
            "Placed {} of {} rural resource buildings",
            placed_count,
            sd_ids.len()
        );

        // Place processing/secondary buildings into the urban region. The resource chain
        // gives us a count per building type; placement order is randomised inside the
        // helper. There's no fixed mapping of building → urban super-parcel, so we pass
        // the whole urban region as one candidate pool.
        let urban_districts: Vec<_> = editor
            .world()
            .districts
            .values()
            .filter(|sd| sd.data.parcel_type == ParcelType::Urban)
            .cloned()
            .collect();
        let urban_refs: Vec<_> = urban_districts.iter().collect();

        log::info!(
            "Placing {} processing-building slots across {} urban super-parcels",
            result.processing_buildings.values().sum::<u32>(),
            urban_refs.len(),
        );
        if let Err(e) = place_urban_buildings(
            &urban_refs,
            &result.processing_buildings,
            &mut rng,
            &mut editor,
            &data,
            None,
        )
        .await
        {
            log::warn!("Urban resource placement failed: {}", e);
        }

        // Paint the ground by super-parcel type and mark borders.
        let urban_wool: Block = "blue_wool".into();
        let rural_wool: Block = "green_wool".into();
        let off_limits_wool: Block = "red_wool".into();
        let unknown_wool: Block = "black_wool".into();
        let glass: Block = "glass".into();
        let bedrock: Block = "bedrock".into();

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                // Skip cells claimed by a placed resource building so we don't paint over it.
                if editor.world().is_claimed(Point2D::new(x, z)) {
                    continue;
                }

                let district_id = editor.world().district_map[x as usize][z as usize];
                let parcel_id = editor.world().parcel_map[x as usize][z as usize];

                let Some(district_id) = district_id else { continue };
                let Some(parcel_id) = parcel_id else { continue };

                let world = editor.world();
                let district_type = world
                    .districts
                    .get(&district_id)
                    .map(|sd| sd.data.parcel_type)
                    .unwrap_or(ParcelType::Unknown);

                let block = match district_type {
                    ParcelType::Urban => &urban_wool,
                    ParcelType::Rural => &rural_wool,
                    ParcelType::OffLimits => &off_limits_wool,
                    ParcelType::Unknown => &unknown_wool,
                };

                let height = height_map[x as usize][z as usize] - build_area.origin.y;
                // Skip cells whose surface lies outside the build area's y range — placing
                // there would emit out-of-bounds warnings and the block would be ignored.
                if height < 0 || height >= build_area.size.y {
                    continue;
                }
                let point = Point3D::new(x, height, z);

                let on_super_edge = world
                    .districts
                    .get(&district_id)
                    .map(|sd| sd.data.edges.contains(&point))
                    .unwrap_or(false);
                let on_parcel_edge = world
                    .parcels
                    .get(&parcel_id)
                    .map(|d| d.data.edges.contains(&point))
                    .unwrap_or(false);

                if on_super_edge {
                    editor.place_block(&bedrock, Point3D::new(x, height, z)).await;
                } else if on_parcel_edge && height >= 1 {
                    editor.place_block(&glass, Point3D::new(x, height, z)).await;
                    editor.place_block(block, Point3D::new(x, height - 1, z)).await;
                } else {
                    editor.place_block(block, Point3D::new(x, height, z)).await;
                }
            }
        }

        editor.flush_buffer().await;
    }

    /// Same end-to-end flow as `rural_resource_placement_paints_parcels`, but builds the
    /// city wall *before* placing buildings. The wall claims its perimeter cells with
    /// `BuildClaim::Wall`, which causes urban placement's `rect_overlaps_claim` check to
    /// keep processing buildings off the wall — making the city border visible as a clear
    /// gap between the painted ground and the placed structures.
    #[tokio::test]
    async fn rural_and_urban_placement_with_city_wall() {
        init_logger();

        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let height_map = provider
            .get_heightmap(
                build_area.origin.x,
                build_area.origin.z,
                build_area.size.x,
                build_area.size.z,
                HeightMapType::MotionBlockingNoPlants,
            )
            .await
            .expect("Failed to get heightmap");

        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        generate_parcels(seed, &mut editor).await;

        let data = LoadedData::load().expect("Failed to load generator data");

        // Build the city wall first so its cells are claimed before any building placement
        // can pick them. Without this ordering, buildings would happily land on top of where
        // the wall later goes.
        let urban_points = editor.world().get_urban_points();
        let material = MaterialId::new("stone_bricks".to_string());
        let mut wall_rng = rng.derive();
        let mut placer_rng = rng.derive();
        let mut placer = Placer::new(&data.materials, &mut placer_rng);
        build_wall(
            &urban_points,
            &mut editor,
            &mut wall_rng,
            &mut placer,
            &material,
            &data.structures,
            WallType::StandardWithInner,
        )
        .await;

        // Resolve resources for rural super-parcels only.
        let rural_analysis: HashMap<_, _> = editor
            .world()
            .district_analysis_data
            .iter()
            .filter(|(id, _)| {
                editor
                    .world()
                    .districts
                    .get(id)
                    .map(|sd| sd.data.parcel_type == ParcelType::Rural)
                    .unwrap_or(false)
            })
            .map(|(id, analysis)| (*id, analysis.clone()))
            .collect();

        let result = data
            .resource_registry
            .resolve_for_parcels(&rural_analysis, &mut rng);

        // One placement per rural super-parcel.
        let mut sd_ids: Vec<_> = result.parcel_assignments.keys().cloned().collect();
        sd_ids.sort_by_key(|id| id.0);

        let mut placed_count = 0usize;
        for sd_id in &sd_ids {
            let assignment = &result.parcel_assignments[sd_id];

            let Some(district) = editor.world().districts.get(sd_id).cloned() else {
                continue;
            };
            let structure_type = StructureType(assignment.building.clone());
            let Some(structure) = data.structures.get(&structure_type).cloned() else {
                log::warn!("No structure for building '{}'", assignment.building);
                continue;
            };

            match place_rural_building(&district, &structure, &mut rng, &mut editor, &data).await {
                Ok(()) => {
                    placed_count += 1;
                    if let Some(painter) = &assignment.production_painter {
                        paint_production_area(&district, painter, &data, &mut editor, &mut rng).await;
                    }
                }
                Err(e) => log::warn!("Rural placement failed for '{}': {}", assignment.building, e),
            }
        }
        log::info!(
            "Placed {} of {} rural resource buildings",
            placed_count,
            sd_ids.len()
        );

        // Place urban processing buildings — they steer around the wall's claimed cells.
        let urban_districts: Vec<_> = editor
            .world()
            .districts
            .values()
            .filter(|sd| sd.data.parcel_type == ParcelType::Urban)
            .cloned()
            .collect();
        let urban_refs: Vec<_> = urban_districts.iter().collect();

        log::info!(
            "Placing {} processing-building slots across {} urban super-parcels (with city wall)",
            result.processing_buildings.values().sum::<u32>(),
            urban_refs.len(),
        );
        if let Err(e) = place_urban_buildings(
            &urban_refs,
            &result.processing_buildings,
            &mut rng,
            &mut editor,
            &data,
            None,
        )
        .await
        {
            log::warn!("Urban resource placement failed: {}", e);
        }

        // Paint the ground by super-parcel type and mark borders. Skips claimed cells,
        // so the wall and placed buildings are visually preserved.
        let urban_wool: Block = "blue_wool".into();
        let rural_wool: Block = "green_wool".into();
        let off_limits_wool: Block = "red_wool".into();
        let unknown_wool: Block = "black_wool".into();
        let glass: Block = "glass".into();
        let bedrock: Block = "bedrock".into();

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                if editor.world().is_claimed(Point2D::new(x, z)) {
                    continue;
                }

                let district_id = editor.world().district_map[x as usize][z as usize];
                let parcel_id = editor.world().parcel_map[x as usize][z as usize];

                let Some(district_id) = district_id else { continue };
                let Some(parcel_id) = parcel_id else { continue };

                let world = editor.world();
                let district_type = world
                    .districts
                    .get(&district_id)
                    .map(|sd| sd.data.parcel_type)
                    .unwrap_or(ParcelType::Unknown);

                let block = match district_type {
                    ParcelType::Urban => &urban_wool,
                    ParcelType::Rural => &rural_wool,
                    ParcelType::OffLimits => &off_limits_wool,
                    ParcelType::Unknown => &unknown_wool,
                };

                let height = height_map[x as usize][z as usize] - build_area.origin.y;
                if height < 0 || height >= build_area.size.y {
                    continue;
                }
                let point = Point3D::new(x, height, z);

                let on_super_edge = world
                    .districts
                    .get(&district_id)
                    .map(|sd| sd.data.edges.contains(&point))
                    .unwrap_or(false);
                let on_parcel_edge = world
                    .parcels
                    .get(&parcel_id)
                    .map(|d| d.data.edges.contains(&point))
                    .unwrap_or(false);

                if on_super_edge {
                    editor.place_block(&bedrock, Point3D::new(x, height, z)).await;
                } else if on_parcel_edge && height >= 1 {
                    editor.place_block(&glass, Point3D::new(x, height, z)).await;
                    editor.place_block(block, Point3D::new(x, height - 1, z)).await;
                } else {
                    editor.place_block(block, Point3D::new(x, height, z)).await;
                }
            }
        }

        editor.flush_buffer().await;
    }
}
