#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use crate::{data::Loadable, editor::World, generator::districts::{WallType, build_wall, HasParcelData, parcel::{self, generate_parcels}, parcel_painter::{replace_ground, replace_ground_smooth}}, geometry::{Point2D, Point3D}, http_mod::{GDMCHTTPProvider, HeightMapType}, minecraft::Block, noise::{RNG, Seed}, util::init_logger};
    use crate::generator::materials::{Placer, Material, MaterialId};
    use crate::generator::nbts::Structure;

    fn get_block_for_id(id : usize) -> Block {
        // List of all 16 wool colors in order
        let wool_colors = [
            "white_wool", "orange_wool", "magenta_wool", "light_blue_wool",
            "yellow_wool", "lime_wool", "pink_wool", "gray_wool",
            "light_gray_wool", "cyan_wool", "purple_wool", "blue_wool",
            "brown_wool", "green_wool", "red_wool", "black_wool",
        ];
        Block {
            id: wool_colors[id % wool_colors.len()].into(),
            data: None,
            state: None,
        }
    }

    /// A standing oak sign whose front first line is `text`.
    fn sign_block(text: &str) -> Block {
        let data = format!(
            "{{front_text:{{messages:['\"{}\"','\"\"','\"\"','\"\"']}}}}",
            text
        );
        Block::new("oak_sign".into(), None, Some(data))
    }

    fn get_block_for_parcel_type(parcel_type: parcel::ParcelType) -> Block {
        match parcel_type {
            parcel::ParcelType::Urban => Block { id: "blue_wool".into(), data: None, state: None },
            parcel::ParcelType::Rural => Block { id: "green_wool".into(), data: None, state: None },
            parcel::ParcelType::OffLimits => Block { id: "red_wool".into(), data: None, state: None },
            _ => Block { id: "bedrock".into(), data: None, state: None }, // Default case for unknown types
        }
    }

    #[tokio::test]
    async fn parcel_test() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);

        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        println!("Build area: {:?}", build_area);
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::WorldSurface).await.expect("Failed to get heightmap");
        
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _parcels = generate_parcels(seed, &mut editor).await;
        let glass = Block {
            id: "glass".into(),
            data: None,
            state: None,
        };

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                let parcel_id = editor.world_mut().parcel_map[x as usize][z as usize];

                let Some(parcel_id) = parcel_id else {
                    continue;
                };
                

                let block = get_block_for_id(parcel_id.0 as usize);
                let height = height_map[x as usize][z as usize] - build_area.origin.y;
                let point = Point3D::new(x, height, z);
                //editor.place_block(&block, Point3D::new(x, height - build_area.origin.y, z)).await;
                if let Some(parcel) = editor.world_mut().parcels.get(&parcel_id) {
                    
                    if parcel.data.edges.contains(&point) {
                        editor.place_block(&glass, Point3D::new(x, height , z)).await;
                        editor.place_block(&block, Point3D::new(x, height - 1, z)).await;
                    } else {
                        editor.place_block(&block, Point3D::new(x, height, z)).await;
                    }
                }
            }
        }

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn district_test() {
        init_logger();
        println!("hello");

        // Initialize the test data
        let seed = Seed(12345);

        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::MotionBlockingNoPlants).await.expect("Failed to get heightmap");
        
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _parcels = generate_parcels(seed, &mut editor).await;

        let glass = Block {
            id: "glass".into(),
            data: None,
            state: None,
        };
        let bedrock = Block {
            id: "bedrock".into(),
            data: None,
            state: None,
        };

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                let district_id = editor.world_mut().district_map[x as usize][z as usize];
                let parcel_id = editor.world_mut().parcel_map[x as usize][z as usize];

                let Some(parcel_id) = parcel_id else {
                    continue;
                };
                let Some(district_id) = district_id else {
                    continue;
                };

                let block = get_block_for_id(district_id.0 as usize);
                let height = height_map[x as usize][z as usize] - build_area.origin.y;
                let point = Point3D::new(x, height, z);

                let World {parcels,districts, .. } = editor.world_mut();

                let district = districts.get(&district_id).expect("Failed to get super parcel");
                let parcel = parcels.get(&parcel_id).expect("Failed to get parcel");
                if district.data.edges.contains(&point) {
                    editor.place_block(&bedrock, Point3D::new(x, height, z)).await;
                }
                else if parcel.data.edges.contains(&point) {
                    editor.place_block(&glass, Point3D::new(x, height, z)).await;
                    editor.place_block(&block, Point3D::new(x, height - 1, z)).await;
                }
                else {
                    editor.place_block(&block, Point3D::new(x, height, z)).await;
                }

            }
        }

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn district_classification() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);

        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        println!("Build area: {:?}", build_area);
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::MotionBlockingNoPlants).await.expect("Failed to get heightmap");
        
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _parcels = generate_parcels(seed, &mut editor).await;
        let glass = Block {
            id: "glass".into(),
            data: None,
            state: None,
        };
        let bedrock  = Block {
            id: "bedrock".into(),
            data: None,
            state: None,
        };

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                let district_id = editor.world_mut().district_map[x as usize][z as usize];
                let parcel_id = editor.world_mut().parcel_map[x as usize][z as usize];

                let Some(parcel_id) = parcel_id else {
                    continue;
                };
                let Some(district_id) = district_id else {
                    continue;
                };

                let block = get_block_for_parcel_type(editor.world_mut().districts.get(&district_id).expect("Failed to get parcel").data.parcel_type);
                let height = height_map[x as usize][z as usize] - build_area.origin.y;
                let point = Point3D::new(x, height, z);

                let World {parcels,districts, .. } = editor.world_mut();
                let district = districts.get(&district_id).expect("Failed to get super parcel");
                let parcel = parcels.get(&parcel_id).expect("Failed to get parcel");

                if district.data.edges.contains(&point) {
                    editor.place_block(&bedrock, Point3D::new(x, height, z)).await;
                }
                else if parcel.data.edges.contains(&point) {
                    editor.place_block(&glass, Point3D::new(x, height, z)).await;
                    editor.place_block(&block, Point3D::new(x, height - 1, z)).await;
                }
                else {
                    editor.place_block(&block, Point3D::new(x, height, z)).await;
                }

            }
        }

        // Log each super-parcel's final type/size and snapshot its centre while we hold
        // the borrow; drop the borrow before placing the signs.
        let sign_info: Vec<(usize, parcel::ParcelType, Point2D, usize)> = editor.world()
            .districts.values()
            .map(|sd| (sd.id().0, sd.data.parcel_type, sd.data.average().drop_y(), sd.data.points_2d.len()))
            .collect();

        let pole: Block = "oak_fence".into();
        for (id, parcel_type, centre, size) in sign_info {
            if centre.x < 0 || centre.y < 0 || centre.x >= build_area.size.x || centre.y >= build_area.size.z {
                log::info!("Super-parcel {} final type={:?} size={} cells, centre={:?} out of bounds — no sign", id, parcel_type, size, centre);
                continue;
            }

            // height_map holds absolute world Y; the editor places at coords local to the build area.
            let surface_y = height_map[centre.x as usize][centre.y as usize];
            let (world_x, world_z) = (centre.x + build_area.origin.x, centre.y + build_area.origin.z);
            log::info!(
                "Super-parcel {} final type={:?} size={} cells — sign at world ({}, {}, {})  /tp @s {} {} {}",
                id, parcel_type, size, world_x, surface_y + 4, world_z, world_x, surface_y + 5, world_z
            );

            // A 3-tall pole so the marker pokes above terrain, with the numbered sign on top.
            let h = surface_y - build_area.origin.y;
            for dy in 1..=3 {
                editor.place_block(&pole, Point3D::new(centre.x, h + dy, centre.y)).await;
            }
            editor.place_block(&sign_block(&id.to_string()), Point3D::new(centre.x, h + 4, centre.y)).await;
        }

        // Verify the size band: every Urban/Rural (interior) parcel should be within ±50% of the
        // interior average block count. Off-limits parcels are exempt. See
        // docs/plans/parcel_size_balancing.md.
        let interior_sizes: Vec<(usize, parcel::ParcelType, usize)> = editor.world()
            .districts.values()
            .filter(|sd| matches!(sd.data.parcel_type, parcel::ParcelType::Urban | parcel::ParcelType::Rural))
            .map(|sd| (sd.id().0, sd.data.parcel_type, sd.data.points_2d.len()))
            .collect();

        if !interior_sizes.is_empty() {
            let total: usize = interior_sizes.iter().map(|(_, _, s)| *s).sum();
            let avg = total as f32 / interior_sizes.len() as f32;
            let (lo, hi) = (avg * 0.5, avg * 1.5);
            let mut out_of_band = 0usize;
            for (id, parcel_type, size) in &interior_sizes {
                let ratio = *size as f32 / avg;
                let in_band = (*size as f32) >= lo && (*size as f32) <= hi;
                if !in_band { out_of_band += 1; }
                log::info!(
                    "Band check: super-parcel {} type={:?} size={} avg={:.0} ratio={:.2} in_band={}",
                    id, parcel_type, size, avg, ratio, in_band
                );
            }
            let min = interior_sizes.iter().map(|(_, _, s)| *s).min().unwrap();
            let max = interior_sizes.iter().map(|(_, _, s)| *s).max().unwrap();
            log::info!(
                "Band summary: {} interior parcels, avg={:.0}, min={}, max={}, max/min={:.2}, band [{:.0}, {:.0}], {} out of band",
                interior_sizes.len(), avg, min, max, max as f32 / min.max(1) as f32, lo, hi, out_of_band
            );
        }

        let materials = Material::load().expect("Failed to load materials");
        let material = MaterialId::new("stone_bricks".to_string());

        // Separate RNGs: the placer holds its own for its whole lifetime, so the wall
        // builder needs an independent one (see standard_wall_with_inner).
        let mut rng = RNG::new(seed);
        let mut rng2 = RNG::new(seed);

        let mut placer: Placer = Placer::new(
            &materials,
            &mut rng,
        );

        let structures = Structure::load().expect("Failed to load structures");
        println!("Structures: {:?}", structures.keys());

        build_wall(&editor.world().get_urban_points(), &mut editor, &mut rng2, &mut placer, &material, &structures, WallType::StandardWithInner).await;

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn subdivide_urban_test() {
        init_logger();

        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::MotionBlockingNoPlants).await.expect("Failed to get heightmap");

        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _parcels = generate_parcels(seed, &mut editor).await;

        // Snapshot the cell-sets of every urban super-parcel while we hold a borrow,
        // then drop the borrow before touching the editor again.
        let urban_blocks: Vec<HashSet<Point2D>> = editor.world().districts.values()
            .filter(|sd| sd.data.parcel_type == parcel::ParcelType::Urban)
            .map(|sd| sd.data.points_2d.clone())
            .collect();

        println!("Subdividing {} urban super-parcels", urban_blocks.len());

        let alley_block = Block { id: "polished_andesite".into(), data: None, state: None };

        let mut color_idx: usize = 0;
        let mut total_sub_blocks = 0usize;
        let mut total_alley_cells = 0usize;

        for block_cells in urban_blocks {
            let (sub_blocks, alleys) = crate::generator::districts::subdivide::subdivide_block(
                &block_cells, &mut rng, 32,
            );
            total_sub_blocks += sub_blocks.len();
            total_alley_cells += alleys.len();

            for sub in &sub_blocks {
                let paint = get_block_for_id(color_idx);
                color_idx += 1;
                for p in sub {
                    if p.x < 0 || p.y < 0 || p.x >= build_area.size.x || p.y >= build_area.size.z {
                        continue;
                    }
                    let h = height_map[p.x as usize][p.y as usize] - build_area.origin.y;
                    editor.place_block(&paint, Point3D::new(p.x, h, p.y)).await;
                }
            }

            for p in &alleys {
                if p.x < 0 || p.y < 0 || p.x >= build_area.size.x || p.y >= build_area.size.z {
                    continue;
                }
                let h = height_map[p.x as usize][p.y as usize] - build_area.origin.y;
                editor.place_block(&alley_block, Point3D::new(p.x, h, p.y)).await;
            }
        }

        println!("Produced {} sub-blocks, {} alley cells", total_sub_blocks, total_alley_cells);
        editor.flush_buffer().await;
    }

    /// Subdivide urban super-parcels as in `subdivide_urban_test`, then mark
    /// the alley cells as `BuildClaim::Path` and run the buildings_v2 +
    /// city_houses frontage/interior fill on each sub-block. Used to judge how
    /// well subdivision sizing produces buildable sub-blocks. Requires a live
    /// Minecraft server.
    #[tokio::test]
    async fn subdivide_urban_with_houses() {
        use crate::generator::BuildClaim;
        use crate::generator::buildings_v2::{BuildCtx, Culture};
        use crate::generator::buildings_v2::roof::RoofStyle;
        use crate::generator::buildings_v2::roof::gable::GablePitch;
        use crate::generator::buildings_v2::footprint::SizeClass;
        use crate::generator::city_houses::plot_from_block;
        use crate::generator::data::LoadedData;
        use crate::generator::materials::PaletteId;
        use crate::generator::paths::PathType;

        init_logger();

        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area().await.expect("Failed to get build area");

        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _parcels = generate_parcels(seed, &mut editor).await;

        let urban_blocks: Vec<HashSet<Point2D>> = editor.world().districts.values()
            .filter(|sd| sd.data.parcel_type == parcel::ParcelType::Urban)
            .map(|sd| sd.data.points_2d.clone())
            .collect();
        println!("Subdividing {} urban super-parcels", urban_blocks.len());

        // Take a 2-cell-thick ring at the edge of each urban super-parcel as
        // the perimeter road. The interior cells are what we actually subdivide.
        let mut perimeter_roads: HashSet<Point2D> = HashSet::new();
        let mut interior_blocks: Vec<HashSet<Point2D>> = Vec::new();
        for sd_cells in &urban_blocks {
            let (outer, inner) = crate::geometry::get_outer_and_inner_points(sd_cells, 2);
            perimeter_roads.extend(outer);
            interior_blocks.push(inner);
        }

        let data = LoadedData::load().expect("Failed to load data");
        let base_palette_id: PaletteId = "medieval_spruce".into();
        let base_palette = data.palettes.get(&base_palette_id).expect("Base palette not found").clone();
        let roof_palette_ids: Vec<PaletteId> = vec![
            "acacia_wood_roof".into(),
            "brick_roof".into(),
            "oak_wood_roof".into(),
            "red_wood_roof".into(),
        ];
        // Medieval-feeling woods and stones — skipping tropical (jungle, acacia)
        // and nether (blackstone) variants. Each is a partial palette that only
        // overrides its respective roles, so stacking them gives ~36 combos.
        let wood_palette_ids: Vec<PaletteId> = vec![
            "oak".into(),
            "spruce".into(),
            "dark_oak".into(),
        ];
        let stone_palette_ids: Vec<PaletteId> = vec![
            "stone_bricks".into(),
            "cobblestone".into(),
            "deepslate".into(),
        ];
        let pitches = [
            RoofStyle::Gable(GablePitch::Slab),
            RoofStyle::Gable(GablePitch::Stairs),
            RoofStyle::Gable(GablePitch::Double),
        ];
        let frontage_pool = vec![SizeClass::House];
        let interior_pool = vec![SizeClass::House];

        // Subdivide the INTERIOR of each urban super-parcel, alternating
        // between BSP (axis-aligned cuts) and voronoi (organic partitions) so
        // adjacent parcels visually compare the two patterns.
        let mut all_sub_blocks: Vec<HashSet<Point2D>> = Vec::new();
        let mut all_alleys: HashSet<Point2D> = HashSet::new();
        for (i, inner) in interior_blocks.iter().enumerate() {
            let (sub_blocks, alleys) = if i % 2 == 0 {
                println!("Super-parcel {}: BSP partition", i);
                crate::generator::districts::subdivide::subdivide_block(inner, &mut rng, 32)
            } else {
                let sections = (inner.len() / 400).max(2);
                println!("Super-parcel {}: voronoi partition ({} sections)", i, sections);
                crate::generator::districts::subdivide::voronoi_subdivide_block(inner, &mut rng, sections)
            };
            all_sub_blocks.extend(sub_blocks);
            all_alleys.extend(alleys);
        }
        println!(
            "Partitioning produced {} sub-blocks, {} alley cells, {} perimeter road cells",
            all_sub_blocks.len(), all_alleys.len(), perimeter_roads.len(),
        );

        // All road cells: perimeter + alleys. Claim as PathPlanned so the
        // frontage walker treats them as roads, but foundation terrain
        // blending will still raise the heightmap on them — meaning the
        // post-house pave step picks up the foundation-influenced heights.
        let road_cells: HashSet<Point2D> = perimeter_roads.iter().chain(all_alleys.iter()).copied().collect();
        for p in &road_cells {
            editor.world_mut().claim(*p, BuildClaim::PathPlanned(PathType::Pavement));
        }

        // Place houses one at a time so we can roll a fresh roof style and
        // palette per building. `place_block_frontage` / `fill_interior` lock a
        // single style for the whole sub-block, so we replicate their loops
        // here with the per-house roll.
        use crate::generator::buildings_v2::{BuildingContext, build_house};
        use crate::generator::city_houses::{
            SIDE_BUFFER_CELLS, detect_frontages,
            detect_perimeter_frontages, rect_from_frontage, synthetic_plot_bounds,
        };
        use crate::generator::buildings_v2::footprint::Footprint;
        use crate::geometry::Point2D as P2;

        fn roll_palette(
            rng: &mut RNG,
            base: &crate::generator::materials::Palette,
            data: &crate::generator::data::LoadedData,
            woods: &[crate::generator::materials::PaletteId],
            stones: &[crate::generator::materials::PaletteId],
            roofs: &[crate::generator::materials::PaletteId],
        ) -> crate::generator::materials::Palette {
            let w = &woods[rng.rand_i32_range(0, woods.len() as i32) as usize];
            let s = &stones[rng.rand_i32_range(0, stones.len() as i32) as usize];
            let r = &roofs[rng.rand_i32_range(0, roofs.len() as i32) as usize];
            base.clone()
                .merged_with(data.palettes.get(w).expect("wood palette not found"))
                .merged_with(data.palettes.get(s).expect("stone palette not found"))
                .merged_with(data.palettes.get(r).expect("roof palette not found"))
        }

        fn mark_rect_used(plot: &mut crate::generator::buildings_v2::footprint::Plot, rect: &crate::geometry::Rect2D, buffer: i32) {
            let plot_min = plot.bounds.min();
            for x in (rect.min().x - buffer)..=(rect.max().x + buffer) {
                for z in (rect.min().y - buffer)..=(rect.max().y + buffer) {
                    let lx = x - plot_min.x;
                    let lz = z - plot_min.y;
                    if lx < 0 || lz < 0 { continue; }
                    let lx = lx as usize;
                    let lz = lz as usize;
                    if lx < plot.usable.len() && lz < plot.usable[0].len() {
                        plot.usable[lx][lz] = false;
                    }
                }
            }
        }
        fn mark_footprint_used(plot: &mut crate::generator::buildings_v2::footprint::Plot, footprint: &Footprint, buffer: i32) {
            let plot_min = plot.bounds.min();
            for point in footprint.filled_points() {
                for dx in -buffer..=buffer {
                    for dz in -buffer..=buffer {
                        let lx = point.x + dx - plot_min.x;
                        let lz = point.y + dz - plot_min.y;
                        if lx < 0 || lz < 0 { continue; }
                        let lx = lx as usize;
                        let lz = lz as usize;
                        if lx < plot.usable.len() && lz < plot.usable[0].len() {
                            plot.usable[lx][lz] = false;
                        }
                    }
                }
            }
        }

        let mut total_buildings = 0usize;
        for sub_block in all_sub_blocks.iter() {
            if sub_block.is_empty() {
                continue;
            }
            let mut plot = match plot_from_block(sub_block) {
                Some(p) => p,
                None => continue,
            };

            // Frontage pass — one house per slot along each frontage chain.
            let frontages = {
                let detected = detect_frontages(sub_block, &editor);
                if detected.is_empty() {
                    detect_perimeter_frontages(sub_block)
                } else {
                    detected
                }
            };
            for frontage in &frontages {
                if frontage.cells.is_empty() { continue; }
                let min_front = frontage_pool.iter().map(|s| *s.front_width_range().start()).min().unwrap_or(0);
                let chain_len = frontage.cells.len() as i32;
                if chain_len < min_front { continue; }
                let mut cursor: i32 = if min_front > 1 { rng.rand_i32_range(0, min_front) } else { 0 };
                while cursor + min_front <= chain_len {
                    let size_class = *rng.choose(&frontage_pool);
                    let fw = rng.rand_i32_range(*size_class.front_width_range().start(), *size_class.front_width_range().end() + 1);
                    let depth = rng.rand_i32_range(*size_class.depth_range().start(), *size_class.depth_range().end() + 1);
                    if cursor + fw > chain_len { cursor += 1; continue; }
                    let chain_slice = &frontage.cells[cursor as usize..(cursor + fw) as usize];
                    let rect = rect_from_frontage(chain_slice, frontage.outward, depth);
                    let cells_ok = rect.iter().all(|p: P2| {
                        let lx = p.x - plot.bounds.min().x;
                        let lz = p.y - plot.bounds.min().y;
                        lx >= 0 && lz >= 0
                            && (lx as usize) < plot.usable.len()
                            && (lz as usize) < plot.usable[0].len()
                            && plot.usable[lx as usize][lz as usize]
                    });
                    if !cells_ok { cursor += 1; continue; }

                    // Per-house roll: roof + palette.
                    let palette = roll_palette(
                        &mut rng, &base_palette, &data,
                        &wood_palette_ids, &stone_palette_ids, &roof_palette_ids,
                    );
                    let roof_style = pitches[rng.rand_i32_range(0, pitches.len() as i32) as usize];

                    let plot_bounds = synthetic_plot_bounds(&rect, frontage.outward);
                    let footprint = Footprint::from_rect(rect);
                    let bctx = BuildingContext::new(Culture::Medieval, size_class, roof_style);
                    let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
                    match build_house(&mut ctx, footprint, &bctx, plot_bounds).await {
                        Ok(_) => {
                            mark_rect_used(&mut plot, &rect, SIDE_BUFFER_CELLS);
                            total_buildings += 1;
                            cursor += fw + SIDE_BUFFER_CELLS;
                        }
                        Err(msg) => {
                            log::warn!("frontage build_house failed: {}", msg);
                            cursor += 1;
                        }
                    }
                }
            }

            // Interior pass disabled — frontage only for now.
            // let max_interior = 10usize;
            // let mut placed = 0usize;
            // while placed < max_interior {
            //     let size_class = *rng.choose(&interior_pool);
            //     let footprint = match generate_footprint(&mut rng, &plot, &size_class) {
            //         Some(fp) => fp,
            //         None => break,
            //     };
            //     let palette = roll_palette(
            //         &mut rng, &base_palette, &data,
            //         &wood_palette_ids, &stone_palette_ids, &roof_palette_ids,
            //     );
            //     let roof_style = pitches[rng.rand_i32_range(0, pitches.len() as i32) as usize];
            //     mark_footprint_used(&mut plot, &footprint, INTERIOR_BUFFER_CELLS);
            //     let bctx = BuildingContext::new(Culture::Medieval, size_class, roof_style);
            //     let plot_bounds = plot.bounds;
            //     let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
            //     match build_house(&mut ctx, footprint, &bctx, plot_bounds).await {
            //         Ok(_) => { placed += 1; total_buildings += 1; }
            //         Err(msg) => log::warn!("interior build_house failed: {}", msg),
            //     }
            // }
        }

        // Post-house pave pass: now that foundations have raised the heightmap
        // around each building (including the PathPlanned road cells in their
        // blend ring), read the live heightmap and paint pavement one block
        // below it. Convert claims to Path so any subsequent passes treat
        // these as proper roads.
        let alley_block = Block { id: "polished_andesite".into(), data: None, state: None };
        for p in &road_cells {
            if p.x < 0 || p.y < 0 || p.x >= build_area.size.x || p.y >= build_area.size.z {
                continue;
            }
            let h = editor.world().get_ocean_floor_height_at(*p);
            editor.place_block(&alley_block, Point3D::new(p.x, h - 1, p.y)).await;
            editor.world_mut().claim(*p, BuildClaim::Path(PathType::Pavement));
        }

        editor.flush_buffer().await;
        println!(
            "Done — {} total buildings across {} sub-blocks",
            total_buildings, all_sub_blocks.len(),
        );
    }

    #[tokio::test]
    async fn parcel_classification() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);

        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        println!("Build area: {:?}", build_area);
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::MotionBlockingNoPlants).await.expect("Failed to get heightmap");
        
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _parcels = generate_parcels(seed, &mut editor).await;
        let glass = Block {
            id: "glass".into(),
            data: None,
            state: None,
        };

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                let parcel_id = editor.world_mut().parcel_map[x as usize][z as usize];

                let Some(parcel_id) = parcel_id else {
                    continue;
                };
                

                let block = get_block_for_parcel_type(editor.world_mut().parcels.get(&parcel_id).expect("Failed to get parcel").data.parcel_type);
                let height = height_map[x as usize][z as usize] - build_area.origin.y;
                let point = Point3D::new(x, height, z);
                //editor.place_block(&block, Point3D::new(x, height - build_area.origin.y, z)).await;
                if let Some(parcel) = editor.world_mut().parcels.get(&parcel_id) {
                    
                    if parcel.data.edges.contains(&point) {
                        editor.place_block(&glass, Point3D::new(x, height , z)).await;
                        editor.place_block(&block, Point3D::new(x, height - 1, z)).await;
                    } else {
                        editor.place_block(&block, Point3D::new(x, height, z)).await;
                    }
                }
            }
        }

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn parcel_classification_parcel_points() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);

        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        println!("Build area: {:?}", build_area);
        
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _parcels = generate_parcels(seed, &mut editor).await;
        let glass = Block {
            id: "glass".into(),
            data: None,
            state: None,
        };

        // Collect parcel ids and their points to avoid multiple mutable borrows
        let parcel_points: Vec<_> = {
            let world = editor.world_mut();
            world.parcels.iter().map(|(parcel_id, parcel)| {
                (*parcel_id, parcel.data.parcel_type, parcel.data.points.clone(), parcel.data.edges.clone())
            }).collect()
        };

        for (_parcel_id, parcel_type, points, edges) in parcel_points {
            let block = get_block_for_parcel_type(parcel_type);
            for point in points.iter() {
                if edges.contains(point) {
                    editor.place_block(&glass, *point).await;
                    editor.place_block(&block, Point3D::new(point.x, point.y - 1, point.z)).await;
                } else {
                    editor.place_block(&block, *point).await;
                }
            }
        }

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn parcel_resource_production_report() {
        init_logger();

        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        generate_parcels(seed, &mut editor).await;

        let registry = crate::generator::resource_chain::ResourceRegistry::load()
            .expect("Failed to load resource registry");

        // Only Rural super-parcels produce raw resources.
        let rural_analysis: HashMap<_, _> = editor.world().district_analysis_data.iter()
            .filter(|(id, _)| {
                editor.world().districts.get(id)
                    .map(|sd| sd.data.parcel_type == crate::generator::districts::ParcelType::Rural)
                    .unwrap_or(false)
            })
            .map(|(id, analysis)| (*id, analysis.clone()))
            .collect();

        let result = registry.resolve_for_parcels(&rural_analysis, &mut rng);

        // Sort producing super-parcel IDs for display.
        let mut producing_ids: Vec<_> = result.parcel_assignments.keys().cloned().collect();
        producing_ids.sort_by_key(|id| id.0);

        println!("\n╔══ Parcel Resource Production Report ════════════╗");

        println!("║ Producing Super-Parcels ({} rural of {} total):", producing_ids.len(), editor.world().district_analysis_data.len());
        for id in &producing_ids {
            let analysis = &editor.world().district_analysis_data[id];
            let biome_names = {
                let mut names: Vec<&str> = analysis.major_biomes().iter()
                    .map(|b| b.as_str().strip_prefix("minecraft:").unwrap_or(b.as_str()))
                    .collect();
                names.sort();
                names.join("+")
            };
            let a = &result.parcel_assignments[id];
            println!("║   Super-Parcel {:>3} ({:<25}) → {} x2 [{}]",
                id.0, biome_names, a.primary_resource, a.building);
        }

        println!("║");
        println!("║ Resource Supply:");
        let mut supply_sorted: Vec<(&String, &u32)> = result.supply.iter().collect();
        supply_sorted.sort_by_key(|(r, _)| r.as_str());
        for (resource, qty) in supply_sorted {
            println!("║   {:<20} x{}", resource, qty);
        }

        println!("║");
        println!("║ Goods Produced:");
        if result.finished_goods.is_empty() && result.leftover_goods.is_empty() {
            println!("║   (none)");
        }
        for (good, qty) in &result.finished_goods {
            println!("║   {:<20} x{}", good, qty);
        }
        for (good, qty) in &result.leftover_goods {
            println!("║   {:<20} x{}  (unused)", good, qty);
        }

        println!("║");
        println!("║ Gathering Buildings:");
        let mut gb_sorted: Vec<(&String, &u32)> = result.gather_buildings.iter().collect();
        gb_sorted.sort_by_key(|(b, _)| b.as_str());
        for (building, count) in gb_sorted {
            println!("║   {:<20} x{}", building, count);
        }

        println!("║");
        println!("║ Processing Buildings Required:");
        if result.processing_buildings.is_empty() {
            println!("║   (none)");
        }
        let mut pb_sorted: Vec<(&String, &u32)> = result.processing_buildings.iter().collect();
        pb_sorted.sort_by(|(a, ac), (b, bc)| bc.cmp(ac).then(a.cmp(b)));
        for (building, count) in pb_sorted {
            println!("║   {:<20} x{}", building, count);
        }

        println!("╚═══════════════════════════════════════════════════╝\n");
    }

    #[tokio::test]
    async fn parcel_replace_ground() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");

        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _parcels = generate_parcels(seed, &mut editor).await;

        let block_vec : Vec<Block> = vec![
            "stone".into(), "cobblestone".into(), "stone_bricks".into(), "andesite".into(), "gravel".into(),
        ];

        let block_dict: HashMap<usize, f32> = [
            (0, 3.0),  // Stone
            (1, 2.0),  // Cobblestone
            (2, 8.0),  // Stone Bricks
            (3, 3.0),  // Andesite
            (4, 1.0),  // Gravel
        ].into_iter().collect();

        let mut road_points = HashSet::new();

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                road_points.insert(Point2D::new(x, z));
            }
        }

        replace_ground(
            &road_points,
            &block_dict,
            &block_vec,
            &mut rng,
            &mut editor,
            Some(0),
            None, // No permit blocks
            Some(false), // Ignore water
        ).await;

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn parcel_replace_ground_smooth() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");

        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _parcels = generate_parcels(seed, &mut editor).await;

        let block_vec : Vec<Block> = vec![
            "stone".into(), "cobblestone".into(), "stone_bricks".into(), "andesite".into(), "gravel".into(),
            "stone_stairs".into(), "cobblestone_stairs".into(), "stone_brick_stairs".into(), "andesite_stairs".into(),
            "stone_slab".into(), "cobblestone_slab".into(), "stone_brick_slab".into(), "andesite_slab".into(),
        ];

        let mut blocks_dict: HashMap<usize, HashMap<usize, f32>> = HashMap::new();

        let block_dict = [
            (0, 3.0),  // Stone
            (1, 2.0),  // Cobblestone
            (2, 8.0),  // Stone Bricks
            (3, 3.0),  // Andesite
            (4, 1.0),  // Gravel
        ].into_iter().collect();
        blocks_dict.insert(0, block_dict);

        let stair_dict = [
            (5, 3.0),  // Stone stairs
            (6, 2.0),  // Cobblestone stairs
            (7, 8.0),  // Stone Bricks stairs
            (8, 4.0),  // Andesite stairs
        ].into_iter().collect();
        blocks_dict.insert(1, stair_dict);

        let slab_dict = [
            (9, 3.0),   // Stone slab
            (10, 2.0),  // Cobblestone slab
            (11, 8.0),  // Stone Bricks slab
            (12, 4.0),  // Andesite slab
        ].into_iter().collect();
        blocks_dict.insert(2, slab_dict);


        let mut road_points = HashSet::new();

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                road_points.insert(Point2D::new(x, z));
            }
        }

        replace_ground_smooth(
            &road_points,
            &blocks_dict,
            &block_vec,
            &mut rng,
            &mut editor,
            Some(0),
            None, // No permit blocks
            Some(false), // Ignore water
        ).await;

        editor.flush_buffer().await;
    }

    #[tokio::test]
    async fn get_wall_points() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);

        
        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let height_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::MotionBlockingNoPlants).await.expect("Failed to get heightmap");

        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        generate_parcels(seed, &mut editor).await;

         let glass = Block {
            id: "glass".into(),
            data: None,
            state: None,
        };
        let bedrock  = Block {
            id: "bedrock".into(),
            data: None,
            state: None,
        };
        let black_wool: Block  = Block {
            id: "black_wool".into(),
            data: None,
            state: None,
        };
        let lime_wool: Block  = Block {
            id: "lime_wool".into(),
            data: None,
            state: None,
        };

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                let district_id = editor.world().district_map[x as usize][z as usize];
                let parcel_id = editor.world().parcel_map[x as usize][z as usize];

                let Some(parcel_id) = parcel_id else {
                    continue;
                };
                let Some(district_id) = district_id else {
                    continue;
                };

                let block = get_block_for_parcel_type(editor.world().districts.get(&district_id).expect("Failed to get parcel").data.parcel_type);
                let height = height_map[x as usize][z as usize] - build_area.origin.y - 1;
                let point = Point3D::new(x, height + 1, z);

                let World {parcels,districts, .. } = editor.world();
                let district = districts.get(&district_id).expect("Failed to get super parcel");
                let parcel = parcels.get(&parcel_id).expect("Failed to get parcel");

                if district.data.edges.contains(&point) {
                    editor.place_block(&bedrock, Point3D::new(x, height, z)).await;
                }
                else if parcel.data.edges.contains(&point) {
                    editor.place_block(&glass, Point3D::new(x, height, z)).await;
                    editor.place_block(&block, Point3D::new(x, height - 1, z)).await;
                }
                else {
                    editor.place_block(&block, Point3D::new(x, height, z)).await;
                }

            }
        }
        let wall_points = crate::generator::districts::wall::get_wall_points(&editor.world().get_urban_points(), &mut editor);
        for point in wall_points.clone() {
            let height = height_map[point.x as usize][point.y as usize] - build_area.origin.y;
            editor.place_block(&black_wool, Point3D::new(point.x, height, point.y)).await;
        }
        for point in editor.world().get_urban_points().difference(&wall_points) {
            let height = height_map[point.x as usize][point.y as usize] - build_area.origin.y;
            editor.place_block(&lime_wool, Point3D::new(point.x, height, point.y)).await;
        }
        editor.flush_buffer().await;

    }

    #[tokio::test]
    async fn palisade() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);
        let mut rng = RNG::new(seed);
        let mut rng2 = RNG::new(seed);
        
        let provider = GDMCHTTPProvider::new();
        
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        generate_parcels(seed, &mut editor).await;

        let materials = Material::load().expect("Failed to load materials");
        let material = MaterialId::new("oak_planks".to_string());

        let mut placer: Placer = Placer::new(
            &materials,
            &mut rng,
        );

        let structures = Structure::load().expect("Failed to load structures");

        build_wall(&editor.world().get_urban_points(), &mut editor, &mut rng2, &mut placer, &material, &structures, WallType::Palisade).await;

    }

    #[tokio::test]
    async fn standard_wall() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);
        let mut rng = RNG::new(seed);
        let mut rng2 = RNG::new(seed);
        
        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        generate_parcels(seed, &mut editor).await;

        let materials = Material::load().expect("Failed to load materials");
        let material = MaterialId::new("stone_bricks".to_string());

        let mut placer: Placer = Placer::new(
            &materials,
            &mut rng,
        );

        let structures = Structure::load().expect("Failed to load structures");

        build_wall(&editor.world().get_urban_points(), &mut editor, &mut rng2, &mut placer, &material, &structures, WallType::Standard).await;

    }

    #[tokio::test]
    async fn standard_wall_with_inner() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);
        let mut rng = RNG::new(seed);
        let mut rng2 = RNG::new(seed);
        
        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        generate_parcels(seed, &mut editor).await;

        let materials = Material::load().expect("Failed to load materials");
        let material = MaterialId::new("stone_bricks".to_string());

        let mut placer: Placer = Placer::new(
            &materials,
            &mut rng,
        );

        let structures = Structure::load().expect("Failed to load structures");
        println!("Structures: {:?}", structures.keys());

        build_wall(&editor.world().get_urban_points(), &mut editor, &mut rng2, &mut placer, &material, &structures, WallType::StandardWithInner).await;

    }

    /// Prototype: feathered urban flatten + tiered A* road network.
    /// parcels -> wall+gates -> flatten -> arterials(MST)+collectors(gates) -> build_path.
    #[tokio::test]
    async fn hierarchical_roads() {
        use crate::generator::data::LoadedData;
        use crate::generator::paths::{build_paths_merged, build_road_network, find_blocks, Path, PathPriority};
        use crate::generator::terrain::{flatten_urban_area, force_height, log_trees};

        init_logger();

        let seed = Seed(12345);
        let mut rng = RNG::new(seed);
        let mut rng2 = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        generate_parcels(seed, &mut editor).await;

        // EVAL AID (test-only): the live server hands out a different build area
        // each run, and the urban classifier frequently collapses to a single
        // parcel — too degenerate to evaluate the road network. Force a
        // contiguous ~4-parcel urban core: keep the prime, then promote the
        // nearest non-off-limits super-parcels to Urban.
        {
            use crate::generator::districts::ParcelType;
            const TARGET_URBAN: usize = 4;
            let mut info: Vec<(crate::generator::districts::DistrictID, Point2D, bool)> =
                editor.world().districts.iter()
                    .filter(|(id, sd)| {
                        if sd.data.parcel_type == ParcelType::OffLimits {
                            return false;
                        }
                        // Never force a water-heavy parcel urban — it would build the
                        // town on a lake. (Matches URBAN_WATER_LIMIT in classification.)
                        let water = editor.world().district_analysis_data
                            .get(id)
                            .map_or(0.0, |a| a.water_percentage());
                        water <= 0.33
                    })
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

        // Wall + gates — gates populate world.gate_locations, used by the network.
        let materials = Material::load().expect("Failed to load materials");
        let wall_material = MaterialId::new("stone_bricks".to_string());
        let mut placer: Placer = Placer::new(&materials, &mut rng);
        let structures = Structure::load().expect("Failed to load structures");
        build_wall(
            &editor.world().get_urban_points(), &mut editor, &mut rng2,
            &mut placer, &wall_material, &structures, WallType::Standard,
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

        // Phase 1 — feathered urban flatten.
        let urban = editor.world().get_urban_points();
        // Log (clear) the urban area of trees so roads, buildings, and houses
        // aren't dropped into standing forest.
        log_trees(&editor, urban.clone()).await;
        println!("Logged {} urban cells of trees", urban.len());
        flatten_urban_area(&mut editor, &urban, 16, 12, true).await;

        let data = LoadedData::load().expect("Failed to load data");

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
        // (their baked `resource_base` blocks → desert sandstone).
        let ind_palette = data.palettes
            .get(&crate::generator::buildings_v2::Culture::Desert.palette_id())
            .expect("industry palette not found").clone();
        if let Err(e) = place_urban_buildings(&urban_sd_refs, &ind_counts, &mut rng, &mut editor, &data, Some(&ind_palette)).await {
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
        let arterial_material = MaterialId::new("stone_bricks".to_string());
        let collector_material = MaterialId::new("cobblestone".to_string());
        let paths = build_road_network(
            &editor, arterial_material, collector_material, true, &ind_nodes, &blocked, 1,
        ).await.paths;
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
        let alley_path = Path::new(alley_pts, 1, MaterialId::new("cobblestone".to_string()), PathPriority::Low);
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
        force_height(&mut editor, &corridor_pts, false).await;
        // `build_paths_merged` returns the exact cells where it laid a half-step
        // slab; we raise a house a block over a fronting slab off this set rather
        // than reading the placed road back (the editor cache is keyed by local
        // coords while get_block subtracts the build-area origin, so a read here
        // returns world terrain, not the road).
        let road_slabs: HashSet<Point3D> = build_paths_merged(&editor, &data, &all_paths, &mut rng).await;
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
        // House + Hall on every tier for now (district wealth will refine this
        // later). Slots that can't fit a Hall's wider frontage just skip.
        let tiers: [(&HashSet<Point2D>, &[SizeClass]); 3] = [
            (&arterial_band, &[SizeClass::House, SizeClass::Hall]),
            (&collector_band, &[SizeClass::House, SizeClass::Hall]),
            (&alley_band, &[SizeClass::House, SizeClass::Hall]),
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
                        let mut bctx_editor = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
                        match build_house(&mut bctx_editor, footprint, &bctx, plot_bounds).await {
                            Ok(_) => {
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
            Block { id: "stone_bricks".into(), data: None, state: None },
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

        editor.flush_buffer().await;
    }
}