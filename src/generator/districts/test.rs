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

    /// Show off the parcel partition: paint every parcel cell in a wool colour
    /// keyed to its parcel id, then raise the parcel's edge cells one block as a
    /// wool ridge so boundaries stand up clearly above the painted ground.
    /// Covers all parcel types (urban, rural, off-limits) — not just urban.
    /// Needs a live Minecraft server.
    /// Run with: `cargo test parcel_wool_borders -- --nocapture`.
    #[tokio::test]
    async fn parcel_wool_borders() {
        init_logger();

        let seed = Seed(12345);

        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let height_map = provider.get_heightmap(
            build_area.origin.x, build_area.origin.z,
            build_area.size.x, build_area.size.z,
            HeightMapType::WorldSurface,
        ).await.expect("Failed to get heightmap");

        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        let _parcels = generate_parcels(seed, &mut editor).await;

        // Snapshot the per-parcel fill cells and edge cells before borrowing the
        // editor for block writes. Keys: ParcelID → (points_2d, edges).
        let parcel_snapshots: Vec<(usize, HashSet<Point2D>, HashSet<Point3D>)> = editor
            .world()
            .parcels
            .iter()
            .map(|(id, p)| (id.0 as usize, p.data.points_2d.clone(), p.data.edges.clone()))
            .collect();

        let mut total_cells = 0usize;
        let mut total_edge_cells = 0usize;

        for (pid, fill_cells, edge_cells) in &parcel_snapshots {
            let wool = get_block_for_id(*pid);

            // Base fill: wool at the surface height for every cell in the parcel.
            for p in fill_cells {
                if p.x < 0 || p.y < 0 || p.x >= build_area.size.x || p.y >= build_area.size.z {
                    continue;
                }
                let h = height_map[p.x as usize][p.y as usize] - build_area.origin.y;
                editor.place_block(&wool, Point3D::new(p.x, h, p.y)).await;
                total_cells += 1;
            }

            // Ridge: one extra wool block directly above each edge cell, so the
            // border stands a block proud of the parcel's painted floor.
            for e in edge_cells {
                if e.x < 0 || e.z < 0 || e.x >= build_area.size.x || e.z >= build_area.size.z {
                    continue;
                }
                editor.place_block(&wool, Point3D::new(e.x, e.y + 1, e.z)).await;
                total_edge_cells += 1;
            }
        }

        println!(
            "Painted {} parcels — {} fill cells, {} edge ridge blocks",
            parcel_snapshots.len(), total_cells, total_edge_cells,
        );

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
                log::info!("Super-parcel {} final type={:?} size={} cells, centre={:?} out of bounds â€” no sign", id, parcel_type, size, centre);
                continue;
            }

            // height_map holds absolute world Y; the editor places at coords local to the build area.
            let surface_y = height_map[centre.x as usize][centre.y as usize];
            let (world_x, world_z) = (centre.x + build_area.origin.x, centre.y + build_area.origin.z);
            log::info!(
                "Super-parcel {} final type={:?} size={} cells â€” sign at world ({}, {}, {})  /tp @s {} {} {}",
                id, parcel_type, size, world_x, surface_y + 4, world_z, world_x, surface_y + 5, world_z
            );

            // A 3-tall pole so the marker pokes above terrain, with the numbered sign on top.
            let h = surface_y - build_area.origin.y;
            for dy in 1..=3 {
                editor.place_block(&pole, Point3D::new(centre.x, h + dy, centre.y)).await;
            }
            editor.place_block(&sign_block(&id.to_string()), Point3D::new(centre.x, h + 4, centre.y)).await;
        }

        // Verify the size band: every Urban/Rural (interior) parcel should be within Â±50% of the
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

        build_wall(&editor.world().get_urban_points(), &mut editor, &mut rng2, &mut placer, &material, &structures, WallType::StandardWithInner, None).await;

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
        // Medieval-feeling woods and stones â€” skipping tropical (jungle, acacia)
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
        // blending will still raise the heightmap on them â€” meaning the
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

            // Frontage pass â€” one house per slot along each frontage chain.
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

            // Interior pass disabled â€” frontage only for now.
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
            "Done â€” {} total buildings across {} sub-blocks",
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

        println!("\nâ•”â•â• Parcel Resource Production Report â•â•â•â•â•â•â•â•â•â•â•â•â•—");

        println!("â•‘ Producing Super-Parcels ({} rural of {} total):", producing_ids.len(), editor.world().district_analysis_data.len());
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
            println!("â•‘   Super-Parcel {:>3} ({:<25}) â†’ {} x2 [{}]",
                id.0, biome_names, a.primary_resource, a.building);
        }

        println!("â•‘");
        println!("â•‘ Resource Supply:");
        let mut supply_sorted: Vec<(&String, &u32)> = result.supply.iter().collect();
        supply_sorted.sort_by_key(|(r, _)| r.as_str());
        for (resource, qty) in supply_sorted {
            println!("â•‘   {:<20} x{}", resource, qty);
        }

        println!("â•‘");
        println!("â•‘ Goods Produced:");
        if result.finished_goods.is_empty() && result.leftover_goods.is_empty() {
            println!("â•‘   (none)");
        }
        for (good, qty) in &result.finished_goods {
            println!("â•‘   {:<20} x{}", good, qty);
        }
        for (good, qty) in &result.leftover_goods {
            println!("â•‘   {:<20} x{}  (unused)", good, qty);
        }

        println!("â•‘");
        println!("â•‘ Gathering Buildings:");
        let mut gb_sorted: Vec<(&String, &u32)> = result.gather_buildings.iter().collect();
        gb_sorted.sort_by_key(|(b, _)| b.as_str());
        for (building, count) in gb_sorted {
            println!("â•‘   {:<20} x{}", building, count);
        }

        println!("â•‘");
        println!("â•‘ Processing Buildings Required:");
        if result.processing_buildings.is_empty() {
            println!("â•‘   (none)");
        }
        let mut pb_sorted: Vec<(&String, &u32)> = result.processing_buildings.iter().collect();
        pb_sorted.sort_by(|(a, ac), (b, bc)| bc.cmp(ac).then(a.cmp(b)));
        for (building, count) in pb_sorted {
            println!("â•‘   {:<20} x{}", building, count);
        }

        println!("â•šâ•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•\n");
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
            false, // not forcing over equally-dense blocks
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

        build_wall(&editor.world().get_urban_points(), &mut editor, &mut rng2, &mut placer, &material, &structures, WallType::Palisade, None).await;

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

        build_wall(&editor.world().get_urban_points(), &mut editor, &mut rng2, &mut placer, &material, &structures, WallType::Standard, None).await;

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

        build_wall(&editor.world().get_urban_points(), &mut editor, &mut rng2, &mut placer, &material, &structures, WallType::StandardWithInner, None).await;

    }

    /// Prototype: feathered urban flatten + tiered A* road network.
    /// parcels -> wall+gates -> flatten -> arterials(MST)+collectors(gates) -> build_path.
    #[tokio::test]
    async fn hierarchical_roads() {
        init_logger();
        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();
        crate::generator::settlement::generate_town(
            &mut editor,
            Seed(12345),
            crate::generator::buildings_v2::Culture::Medieval,
        ).await;
    }

    /// Generate a full town (which now furnishes the open spaces itself) and
    /// report the region-type split for a quick sanity check.
    #[tokio::test]
    async fn open_space_regions() {
        use crate::generator::open_space::{detect_regions, RegionType};

        init_logger();
        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();
        crate::generator::settlement::generate_town(
            &mut editor,
            Seed(12345),
            crate::generator::buildings_v2::Culture::Desert,
        ).await;

        let urban = editor.world().get_urban_points();
        let regions = detect_regions(editor.world(), &urban);

        let total_cells: usize = regions.iter().map(|r| r.area).sum();
        let count = |t: RegionType| regions.iter().filter(|r| r.region_type() == t).count();
        println!(
            "Open-space regions: {} ({} cells) | plaza {} nook {} park {} yard {}",
            regions.len(),
            total_cells,
            count(RegionType::Plaza),
            count(RegionType::Nook),
            count(RegionType::Park),
            count(RegionType::Yard),
        );

        editor.flush_buffer().await;
    }
}
