#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{
        editor::World,
        generator::{
            data::LoadedData,
            districts::{build_wall, generate_districts, DistrictType, WallType},
            materials::{MaterialId, Placer},
            nbts::{Rotation, StructureId},
            placement::{
                anchor_offset_for_rotation, footprint_dims_for_rotation,
                place_rural_building, place_urban_buildings,
            },
        },
        geometry::{Point2D, Point3D},
        http_mod::{GDMCHTTPProvider, HeightMapType},
        minecraft::Block,
        noise::{Seed, RNG},
        util::init_logger,
    };

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

            let structure_id = StructureId(assignment.building.clone());
            let Some(structure) = data.structures.get(&structure_id).cloned() else {
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
                assignment.resource,
                sd_id,
                super_district.data.points_2d.len(),
            );

            match place_rural_building(&super_district, &structure, &mut rng, &mut editor, &data).await {
                Ok(()) => placed_count += 1,
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
            let structure_id = StructureId(assignment.building.clone());
            let Some(structure) = data.structures.get(&structure_id).cloned() else {
                log::warn!("No structure for building '{}'", assignment.building);
                continue;
            };

            match place_rural_building(&super_district, &structure, &mut rng, &mut editor, &data).await {
                Ok(()) => placed_count += 1,
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
