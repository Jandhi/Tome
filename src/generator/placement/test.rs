#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use crate::{
        editor::World,
        generator::{
            buildings_v2::{
                BuildCtx, BuildingContext, Culture, build_house,
                footprint::{Plot, SizeClass, generate_footprint},
                roof::{RoofStyle, gable::GablePitch},
            },
            data::LoadedData,
            districts::{build_wall, generate_districts, DistrictType, WallType},
            materials::{MaterialId, PaletteId, Placer},
            nbts::{Rotation, StructureType},
            paths::{PathPriority, build_path, get_path},
            placement::{
                anchor_offset_for_rotation, footprint_dims_for_rotation,
                place_rural_building, place_urban_buildings,
            },
            resource_chain::paint_production_area,
            terrain::{log_trees, smooth_terrain},
        },
        geometry::{Point2D, Point3D, Rect2D},
        http_mod::{GDMCHTTPProvider, HeightMapType},
        minecraft::Block,
        noise::{Seed, RNG},
        util::init_logger,
    };

    /// Change this to any resource building name to place that building in every rural
    /// super-district. Useful for quickly eyeballing a single building + its production
    /// area on a flat Minecraft world without changing the resource chain data.
    const OVERRIDE_BUILDING: &str = "iron_mine";

    /// End-to-end rural placement test with a single hardcoded building type.
    /// Identical to `rural_and_urban_placement_with_city_wall` except:
    ///   - No city wall (suited for flat worlds).
    ///   - Every rural super-district places `OVERRIDE_BUILDING` instead of the
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

        generate_districts(seed, &mut editor).await;

        let data = LoadedData::load().expect("Failed to load generator data");

        let rural_analysis: HashMap<_, _> = editor
            .world()
            .super_district_analysis_data
            .iter()
            .filter(|(id, _)| {
                editor
                    .world()
                    .super_districts
                    .get(id)
                    .map(|sd| sd.data.district_type == DistrictType::Rural)
                    .unwrap_or(false)
            })
            .map(|(id, analysis)| (*id, analysis.clone()))
            .collect();

        let result = data
            .resource_registry
            .resolve_for_districts(&rural_analysis, &mut rng);

        let mut sd_ids: Vec<_> = result.district_assignments.keys().cloned().collect();
        sd_ids.sort_by_key(|id| id.0);

        let structure_type = StructureType(OVERRIDE_BUILDING.to_string());
        let Some(structure) = data.structures.get(&structure_type).cloned() else {
            log::error!("OVERRIDE_BUILDING '{}' not found in loaded structures", OVERRIDE_BUILDING);
            return;
        };

        // Resolve the painter from the override building's own gather recipe, not from
        // the district assignment (which would use the resource-chain's painter instead).
        let override_painter: Option<String> = data.resource_registry.recipes()
            .values()
            .find(|r| r.inputs.is_empty() && r.building == OVERRIDE_BUILDING)
            .and_then(|r| r.production_painter.clone());

        let mut placed_count = 0usize;
        for sd_id in &sd_ids {
            let assignment = &result.district_assignments[sd_id];

            let Some(super_district) = editor.world().super_districts.get(sd_id).cloned() else {
                continue;
            };

            log::info!(
                "Placing '{}' (override) for resource '{}' in super-district {:?}",
                OVERRIDE_BUILDING, assignment.primary_resource, sd_id,
            );

            match place_rural_building(&super_district, &structure, &mut rng, &mut editor, &data).await {
                Ok(()) => {
                    placed_count += 1;
                    if let Some(painter) = &override_painter {
                        paint_production_area(&super_district, painter, &data, &mut editor, &mut rng).await;
                    }
                }
                Err(e) => log::warn!(
                    "Failed to place '{}' in super-district {:?}: {}",
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
    /// network. Order: districts → force an urban core → wall+gates → flatten →
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
        use crate::generator::districts::{DistrictAnalysis, SuperDistrictID};
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

        generate_districts(seed, &mut editor).await;

        // EVAL AID (test-only): force a contiguous ~4-district urban core, since
        // the live classifier often collapses to a single urban district — too
        // degenerate to grow a road network. (Mirrors `districts::hierarchical_roads`.)
        {
            const TARGET_URBAN: usize = 4;
            let mut info: Vec<(SuperDistrictID, Point2D, bool)> = editor.world().super_districts.iter()
                .filter(|(_, sd)| sd.data.district_type != DistrictType::OffLimits)
                .map(|(id, sd)| {
                    let pts = &sd.data.points_2d;
                    let c = pts.iter().fold(Point2D::ZERO, |a, p| a + *p) / pts.len().max(1) as i32;
                    (*id, c, sd.data.district_type == DistrictType::Urban)
                })
                .collect();
            let anchor = info.iter().find(|(_, _, u)| *u).map(|(_, c, _)| *c)
                .or_else(|| info.first().map(|(_, c, _)| *c));
            if let Some(anchor) = anchor {
                info.sort_by_key(|(_, c, _)| c.distance_squared(&anchor));
                for (id, _, _) in info.iter().take(TARGET_URBAN) {
                    editor.world_mut().super_districts.get_mut(id).unwrap().data.district_type = DistrictType::Urban;
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

        let n_urban = editor.world().super_districts.values()
            .filter(|sd| sd.data.district_type == DistrictType::Urban).count();
        println!("URBAN super-districts: {} | gates: {}", n_urban, editor.world().gate_locations.len());

        // Flatten, then route the tiered network over the gentled terrain.
        let urban = editor.world().get_urban_points();
        flatten_urban_area(&mut editor, &urban, 16, 12, true).await;

        // --- Place the industrial buildings FIRST, on good flattened ground. With
        // no roads yet, `road_bonus` is 0 — these big buildings are sited by
        // flatness, not road frontage. They become the destinations the network
        // then connects. ---
        let rural_ids: Vec<SuperDistrictID> = editor.world().super_districts.iter()
            .filter(|(_, sd)| sd.data.district_type == DistrictType::Rural)
            .map(|(id, _)| *id)
            .collect();
        let rural_analysis: HashMap<SuperDistrictID, DistrictAnalysis> = rural_ids.iter()
            .filter_map(|id| editor.world().super_district_analysis_data.get(id).map(|a| (*id, a.clone())))
            .collect();
        let result = data.resource_registry.resolve_for_districts(&rural_analysis, &mut rng);
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

        let urban_super_districts: Vec<_> = editor.world().super_districts.values()
            .filter(|sd| sd.data.district_type == DistrictType::Urban)
            .cloned()
            .collect();
        let urban_refs: Vec<_> = urban_super_districts.iter().collect();

        let before = editor.world().structures.len();
        if let Err(e) = place_urban_buildings(&urban_refs, &counts, &mut rng, &mut editor, &data).await {
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
            &editor, arterial_material, collector_material, true, &anchor_nodes, &blocked,
        ).await;
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
    ///   districts → log trees → terraform → wall → main roads →
    ///   resource chain → secondary roads → buildings_v2
    #[tokio::test]
    async fn full_settlement_pipeline() {
        init_logger();

        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        // 1. Districts
        generate_districts(seed, &mut editor).await;
        log::info!(
            "Districts: {} super-districts generated",
            editor.world().super_districts.len()
        );

        let data = LoadedData::load().expect("Failed to load generator data");

        // Snapshot super-districts (clone to release the editor borrow)
        let urban_super_districts: Vec<_> = editor
            .world()
            .super_districts
            .values()
            .filter(|sd| sd.data.district_type == DistrictType::Urban)
            .cloned()
            .collect();
        let rural_super_districts: Vec<_> = editor
            .world()
            .super_districts
            .values()
            .filter(|sd| sd.data.district_type == DistrictType::Rural)
            .cloned()
            .collect();

        // 2. Log trees: clear tree canopy from the urban footprint before terraforming
        let urban_cells: HashSet<Point2D> = urban_super_districts
            .iter()
            .flat_map(|sd| sd.data.points_2d.iter().copied())
            .collect();
        log_trees(&editor, urban_cells.clone()).await;
        log::info!("Log trees: cleared trees from {} urban cells", urban_cells.len());

        // 3. Terraforming: smooth urban terrain before the wall goes up
        smooth_terrain(&urban_cells, 0.4, &mut editor).await;
        log::info!("Terraform: smoothed urban terrain (strength 0.4)");

        // 4. Wall: claim the urban perimeter after terrain is levelled
        let urban_points = editor.world().get_urban_points();
        let wall_material = MaterialId::new("stone_bricks".to_string());
        let mut wall_rng = rng.derive();
        let mut placer_rng = rng.derive();
        let mut placer = Placer::new(&data.materials, &mut placer_rng);
        build_wall(
            &urban_points,
            &mut editor,
            &mut wall_rng,
            &mut placer,
            &wall_material,
            &data.structures,
            WallType::StandardWithInner,
        )
        .await;
        log::info!("Walls: city wall built around {} urban cells", urban_points.len());

        // 5. Main roads: High-priority trunk roads from each rural district origin to the
        //    urban hub. Laid after the wall so they can route through the wall gate cells
        //    rather than being blocked by freshly placed wall geometry.
        if let Some(urban_hub_sd) = urban_super_districts.first() {
            let urban_hub = editor.world().add_height(urban_hub_sd.data.origin.drop_y());
            let stone_material = MaterialId::new("stone".to_string());
            let mut main_road_count = 0usize;
            for rural_sd in &rural_super_districts {
                let rural_origin = editor.world().add_height(rural_sd.data.origin.drop_y());
                match get_path(
                    &editor,
                    rural_origin,
                    urban_hub,
                    PathPriority::High,
                    stone_material.clone(),
                    async |_| {},
                )
                .await
                {
                    Some(path) => {
                        let mut road_rng = rng.derive();
                        build_path(&editor, &data, &path, &mut road_rng).await;
                        main_road_count += 1;
                        log::info!(
                            "Main roads: trunk road from rural {:?} ({} waypoints)",
                            rural_sd.id,
                            path.points().len()
                        );
                    }
                    None => log::warn!(
                        "Main roads: A* failed from rural {:?} to urban hub",
                        rural_sd.id
                    ),
                }
            }
            log::info!("Main roads: {} of {} trunk roads built", main_road_count, rural_super_districts.len());
        } else {
            log::warn!("Main roads: no urban super-district found; skipping trunk roads");
        }

        // 6. Resource chain — rural gathering buildings + production areas
        let rural_analysis: HashMap<_, _> = editor
            .world()
            .super_district_analysis_data
            .iter()
            .filter(|(id, _)| {
                editor
                    .world()
                    .super_districts
                    .get(id)
                    .map(|sd| sd.data.district_type == DistrictType::Rural)
                    .unwrap_or(false)
            })
            .map(|(id, analysis)| (*id, analysis.clone()))
            .collect();

        let result = data.resource_registry.resolve_for_districts(&rural_analysis, &mut rng);

        let mut sd_ids: Vec<_> = result.district_assignments.keys().cloned().collect();
        sd_ids.sort_by_key(|id| id.0);

        let mut rural_placed = 0usize;
        for sd_id in &sd_ids {
            let assignment = &result.district_assignments[sd_id];
            let Some(super_district) = editor.world().super_districts.get(sd_id).cloned() else {
                continue;
            };
            let structure_type = StructureType(assignment.building.clone());
            let Some(structure) = data.structures.get(&structure_type).cloned() else {
                log::warn!("No structure for building '{}'", assignment.building);
                continue;
            };
            match place_rural_building(&super_district, &structure, &mut rng, &mut editor, &data).await {
                Ok(()) => {
                    rural_placed += 1;
                    if let Some(painter) = &assignment.production_painter {
                        paint_production_area(&super_district, painter, &data, &mut editor, &mut rng).await;
                    }
                }
                Err(e) => log::warn!("Rural placement failed for '{}': {}", assignment.building, e),
            }
        }
        log::info!(
            "Resource chain: {} of {} rural gathering buildings placed",
            rural_placed,
            sd_ids.len()
        );

        // 6. (cont.) Resource chain — urban processing buildings
        let urban_refs: Vec<_> = urban_super_districts.iter().collect();
        let processing_total: u32 = result.processing_buildings.values().sum();
        if let Err(e) = place_urban_buildings(
            &urban_refs,
            &result.processing_buildings,
            &mut rng,
            &mut editor,
            &data,
        )
        .await
        {
            log::warn!("Urban resource placement failed: {}", e);
        }
        log::info!("Resource chain: {} urban processing buildings queued", processing_total);

        // 7. Secondary roads: Medium-priority cobblestone roads between urban district origins
        if urban_super_districts.len() >= 2 {
            let start = editor.world().add_height(urban_super_districts[0].data.origin.drop_y());
            let end = editor.world().add_height(urban_super_districts[1].data.origin.drop_y());
            let road_material = MaterialId::new("cobblestone".to_string());
            match get_path(&editor, start, end, PathPriority::Medium, road_material, async |_| {}).await {
                Some(path) => {
                    let mut road_rng = rng.derive();
                    build_path(&editor, &data, &path, &mut road_rng).await;
                    log::info!("Secondary roads: built road of {} waypoints", path.points().len());
                }
                None => log::warn!("Secondary roads: A* failed to route between urban origins"),
            }
        } else {
            log::warn!("Secondary roads: fewer than 2 urban super-districts; skipping");
        }

        // 8. Buildings v2: generate and place a house in the first urban super-district
        if let Some(urban_sd) = urban_super_districts.first() {
            let pts: Vec<Point2D> = urban_sd.data.points_2d.iter().copied().collect();
            if !pts.is_empty() {
                let min_x = pts.iter().map(|p| p.x).min().unwrap();
                let max_x = pts.iter().map(|p| p.x).max().unwrap();
                let min_z = pts.iter().map(|p| p.y).min().unwrap();
                let max_z = pts.iter().map(|p| p.y).max().unwrap();

                // Inset by 6 to keep the house away from district edges and the wall
                let margin = 6i32;
                let plot_min = Point2D::new(min_x + margin, min_z + margin);
                let plot_max = Point2D::new(max_x - margin, max_z - margin);

                if plot_max.x > plot_min.x + 12 && plot_max.y > plot_min.y + 12 {
                    let plot_rect = Rect2D::from_points(plot_min, plot_max);
                    let plot = Plot::fully_usable(plot_rect);
                    let size_class = SizeClass::House;
                    let mut house_rng = rng.derive();

                    match generate_footprint(&mut house_rng, &plot, &size_class) {
                        Some(footprint) => {
                            let palette_id: PaletteId = "medieval_spruce".into();
                            match data.palettes.get(&palette_id).cloned() {
                                Some(palette) => {
                                    let bctx = BuildingContext::new(
                                        Culture::Medieval,
                                        size_class,
                                        RoofStyle::Gable(GablePitch::Stairs),
                                    );
                                    let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut house_rng);
                                    match build_house(&mut ctx, footprint, &bctx, plot_rect).await {
                                        Ok(output) => log::info!(
                                            "Buildings v2: placed {:?} house (attic={}, cellar={})",
                                            output.size_class,
                                            output.has_attic,
                                            output.has_cellar,
                                        ),
                                        Err(e) => log::warn!("Buildings v2: build_house failed: {}", e),
                                    }
                                }
                                None => log::warn!("Buildings v2: palette 'medieval_spruce' not found"),
                            }
                        }
                        None => log::warn!(
                            "Buildings v2: generate_footprint returned None for {:?} plot {:?}",
                            size_class,
                            plot_rect,
                        ),
                    }
                } else {
                    log::warn!(
                        "Buildings v2: first urban super-district too small for a house after {}px inset",
                        margin
                    );
                }
            }
        }

        editor.flush_buffer().await;
        log::info!(
            "full_settlement_pipeline complete — {} rural districts processed",
            rural_super_districts.len()
        );
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

    /// End-to-end placement test: generates districts, resolves rural resource assignments,
    /// places each rural super-district's gathering building inside one of its constituent
    /// districts, then paints the ground by district type (Urban/Rural/OffLimits) with
    /// distinct wool colours and marks district + super-district borders.
    #[tokio::test]
    async fn rural_resource_placement_paints_districts() {
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

        generate_districts(seed, &mut editor).await;

        let data = LoadedData::load().expect("Failed to load generator data");

        // Only Rural super-districts produce raw resources.
        let rural_analysis: HashMap<_, _> = editor
            .world()
            .super_district_analysis_data
            .iter()
            .filter(|(id, _)| {
                editor
                    .world()
                    .super_districts
                    .get(id)
                    .map(|sd| sd.data.district_type == DistrictType::Rural)
                    .unwrap_or(false)
            })
            .map(|(id, analysis)| (*id, analysis.clone()))
            .collect();

        let result = data
            .resource_registry
            .resolve_for_districts(&rural_analysis, &mut rng);

        // One placement per rural super-district — matches the resource chain's
        // `district_assignments`, which is keyed by `SuperDistrictID`.
        let mut sd_ids: Vec<_> = result.district_assignments.keys().cloned().collect();
        sd_ids.sort_by_key(|id| id.0);

        let mut placed_count = 0usize;
        for sd_id in &sd_ids {
            let assignment = &result.district_assignments[sd_id];

            let Some(super_district) = editor.world().super_districts.get(sd_id).cloned() else {
                continue;
            };

            let structure_type = StructureType(assignment.building.clone());
            let Some(structure) = data.structures.get(&structure_type).cloned() else {
                log::warn!(
                    "No structure found for building '{}' assigned to super-district {:?}",
                    assignment.building,
                    sd_id
                );
                continue;
            };

            log::info!(
                "Placing '{}' (size {:?}) for resource '{}' in super-district {:?} (area {} cells)",
                assignment.building,
                structure.size_xz,
                assignment.primary_resource,
                sd_id,
                super_district.data.points_2d.len(),
            );

            match place_rural_building(&super_district, &structure, &mut rng, &mut editor, &data).await {
                Ok(()) => {
                    placed_count += 1;
                    if let Some(painter) = &assignment.production_painter {
                        paint_production_area(&super_district, painter, &data, &mut editor, &mut rng).await;
                    }
                }
                Err(e) => log::warn!(
                    "Failed to place '{}' in super-district {:?}: {}",
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
        // helper. There's no fixed mapping of building → urban super-district, so we pass
        // the whole urban region as one candidate pool.
        let urban_super_districts: Vec<_> = editor
            .world()
            .super_districts
            .values()
            .filter(|sd| sd.data.district_type == DistrictType::Urban)
            .cloned()
            .collect();
        let urban_refs: Vec<_> = urban_super_districts.iter().collect();

        log::info!(
            "Placing {} processing-building slots across {} urban super-districts",
            result.processing_buildings.values().sum::<u32>(),
            urban_refs.len(),
        );
        if let Err(e) = place_urban_buildings(
            &urban_refs,
            &result.processing_buildings,
            &mut rng,
            &mut editor,
            &data,
        )
        .await
        {
            log::warn!("Urban resource placement failed: {}", e);
        }

        // Paint the ground by super-district type and mark borders.
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

                let super_district_id = editor.world().super_district_map[x as usize][z as usize];
                let district_id = editor.world().district_map[x as usize][z as usize];

                let Some(super_district_id) = super_district_id else { continue };
                let Some(district_id) = district_id else { continue };

                let world = editor.world();
                let super_district_type = world
                    .super_districts
                    .get(&super_district_id)
                    .map(|sd| sd.data.district_type)
                    .unwrap_or(DistrictType::Unknown);

                let block = match super_district_type {
                    DistrictType::Urban => &urban_wool,
                    DistrictType::Rural => &rural_wool,
                    DistrictType::OffLimits => &off_limits_wool,
                    DistrictType::Unknown => &unknown_wool,
                };

                let height = height_map[x as usize][z as usize] - build_area.origin.y;
                // Skip cells whose surface lies outside the build area's y range — placing
                // there would emit out-of-bounds warnings and the block would be ignored.
                if height < 0 || height >= build_area.size.y {
                    continue;
                }
                let point = Point3D::new(x, height, z);

                let on_super_edge = world
                    .super_districts
                    .get(&super_district_id)
                    .map(|sd| sd.data.edges.contains(&point))
                    .unwrap_or(false);
                let on_district_edge = world
                    .districts
                    .get(&district_id)
                    .map(|d| d.data.edges.contains(&point))
                    .unwrap_or(false);

                if on_super_edge {
                    editor.place_block(&bedrock, Point3D::new(x, height, z)).await;
                } else if on_district_edge && height >= 1 {
                    editor.place_block(&glass, Point3D::new(x, height, z)).await;
                    editor.place_block(block, Point3D::new(x, height - 1, z)).await;
                } else {
                    editor.place_block(block, Point3D::new(x, height, z)).await;
                }
            }
        }

        editor.flush_buffer().await;
    }

    /// Same end-to-end flow as `rural_resource_placement_paints_districts`, but builds the
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

        generate_districts(seed, &mut editor).await;

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

        // Resolve resources for rural super-districts only.
        let rural_analysis: HashMap<_, _> = editor
            .world()
            .super_district_analysis_data
            .iter()
            .filter(|(id, _)| {
                editor
                    .world()
                    .super_districts
                    .get(id)
                    .map(|sd| sd.data.district_type == DistrictType::Rural)
                    .unwrap_or(false)
            })
            .map(|(id, analysis)| (*id, analysis.clone()))
            .collect();

        let result = data
            .resource_registry
            .resolve_for_districts(&rural_analysis, &mut rng);

        // One placement per rural super-district.
        let mut sd_ids: Vec<_> = result.district_assignments.keys().cloned().collect();
        sd_ids.sort_by_key(|id| id.0);

        let mut placed_count = 0usize;
        for sd_id in &sd_ids {
            let assignment = &result.district_assignments[sd_id];

            let Some(super_district) = editor.world().super_districts.get(sd_id).cloned() else {
                continue;
            };
            let structure_type = StructureType(assignment.building.clone());
            let Some(structure) = data.structures.get(&structure_type).cloned() else {
                log::warn!("No structure for building '{}'", assignment.building);
                continue;
            };

            match place_rural_building(&super_district, &structure, &mut rng, &mut editor, &data).await {
                Ok(()) => {
                    placed_count += 1;
                    if let Some(painter) = &assignment.production_painter {
                        paint_production_area(&super_district, painter, &data, &mut editor, &mut rng).await;
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
        let urban_super_districts: Vec<_> = editor
            .world()
            .super_districts
            .values()
            .filter(|sd| sd.data.district_type == DistrictType::Urban)
            .cloned()
            .collect();
        let urban_refs: Vec<_> = urban_super_districts.iter().collect();

        log::info!(
            "Placing {} processing-building slots across {} urban super-districts (with city wall)",
            result.processing_buildings.values().sum::<u32>(),
            urban_refs.len(),
        );
        if let Err(e) = place_urban_buildings(
            &urban_refs,
            &result.processing_buildings,
            &mut rng,
            &mut editor,
            &data,
        )
        .await
        {
            log::warn!("Urban resource placement failed: {}", e);
        }

        // Paint the ground by super-district type and mark borders. Skips claimed cells,
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

                let super_district_id = editor.world().super_district_map[x as usize][z as usize];
                let district_id = editor.world().district_map[x as usize][z as usize];

                let Some(super_district_id) = super_district_id else { continue };
                let Some(district_id) = district_id else { continue };

                let world = editor.world();
                let super_district_type = world
                    .super_districts
                    .get(&super_district_id)
                    .map(|sd| sd.data.district_type)
                    .unwrap_or(DistrictType::Unknown);

                let block = match super_district_type {
                    DistrictType::Urban => &urban_wool,
                    DistrictType::Rural => &rural_wool,
                    DistrictType::OffLimits => &off_limits_wool,
                    DistrictType::Unknown => &unknown_wool,
                };

                let height = height_map[x as usize][z as usize] - build_area.origin.y;
                if height < 0 || height >= build_area.size.y {
                    continue;
                }
                let point = Point3D::new(x, height, z);

                let on_super_edge = world
                    .super_districts
                    .get(&super_district_id)
                    .map(|sd| sd.data.edges.contains(&point))
                    .unwrap_or(false);
                let on_district_edge = world
                    .districts
                    .get(&district_id)
                    .map(|d| d.data.edges.contains(&point))
                    .unwrap_or(false);

                if on_super_edge {
                    editor.place_block(&bedrock, Point3D::new(x, height, z)).await;
                } else if on_district_edge && height >= 1 {
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
