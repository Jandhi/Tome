//! Integration tests for the city_houses frontage pass. These run offline
//! against a synthetic world — no Minecraft server required.

use std::collections::HashSet;

use crate::editor::World;
use crate::generator::BuildClaim;
use crate::generator::buildings_v2::BuildCtx;
use crate::generator::buildings_v2::footprint::SizeClass;
use crate::generator::buildings_v2::roof::RoofStyle;
use crate::generator::buildings_v2::Culture;
use crate::generator::city_houses::{default_frontage_size_pool, place_block_frontage, plot_from_block};
use crate::generator::data::LoadedData;
use crate::generator::paths::PathType;
use crate::geometry::{Cardinal, Point2D, Point3D, Rect3D};
use crate::noise::RNG;

fn make_block_rect(min_x: i32, min_z: i32, width: i32, depth: i32) -> HashSet<Point2D> {
    (min_x..min_x + width)
        .flat_map(|x| (min_z..min_z + depth).map(move |z| Point2D::new(x, z)))
        .collect()
}

/// Drive the offline pipeline for a single synthetic block with a road on the
/// north side, and assert that the houses face north (toward the road).
#[tokio::test]
async fn frontage_pass_places_houses_with_doors_facing_road() {
    let build_area = Rect3D::from_points(
        Point3D::new(0, 0, 0),
        Point3D::new(255, 127, 255),
    );
    let world = World::synthetic(build_area, 64);
    let mut editor = world.get_offline_editor();

    // 32×16 block, with a single road on its north side.
    let block_min_x = 60;
    let block_min_z = 60;
    let block_width = 32;
    let block_depth = 16;
    let block = make_block_rect(block_min_x, block_min_z, block_width, block_depth);

    for x in block_min_x..block_min_x + block_width {
        editor.world_mut().claim(
            Point2D::new(x, block_min_z - 1),
            BuildClaim::Path(PathType::Pavement),
        );
    }

    let data = LoadedData::load().expect("Failed to load data");
    let palette = data.palettes.get(&Culture::Medieval.palette_id()).expect("palette").clone();
    let mut rng = RNG::new(42);
    let mut plot = plot_from_block(&block).expect("block has cells");

    let pool = default_frontage_size_pool();
    let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
    let houses = place_block_frontage(
        &block,
        &mut plot,
        &mut ctx,
        Culture::Medieval,
        RoofStyle::Gable(crate::generator::buildings_v2::roof::gable::GablePitch::Slab),
        &pool,
    )
    .await;

    assert!(!houses.is_empty(), "Expected at least one house on a 32-cell frontage");

    // Verify each placed house has at least one door, and at least 80% of
    // doors are on the road-facing wall. The road is north of the block, so
    // the road-facing wall is the building's north wall. In this codebase
    // `WallSegment::facing` is the interior-pointing direction, so the north
    // wall reports `Cardinal::South` (see existing
    // `place_door_picks_nearest_plot_edge` test in walls/test.rs).
    let mut door_total = 0;
    let mut door_road = 0;
    for house in &houses {
        for (seg, _) in house.wall_segs.doors() {
            door_total += 1;
            if seg.facing == Cardinal::South {
                door_road += 1;
            }
        }
    }
    assert!(door_total > 0, "Expected placed houses to have doors");
    let pct = (door_road as f32) / (door_total as f32);
    assert!(
        pct >= 0.8,
        "Expected ≥80% of doors on the road-facing wall, got {:.0}% ({}/{})",
        pct * 100.0, door_road, door_total,
    );
}

/// Asserts the walker doesn't lay houses on top of each other.
#[tokio::test]
async fn frontage_pass_places_non_overlapping_houses() {
    let build_area = Rect3D::from_points(
        Point3D::new(0, 0, 0),
        Point3D::new(255, 127, 255),
    );
    let world = World::synthetic(build_area, 64);
    let mut editor = world.get_offline_editor();

    let block = make_block_rect(60, 60, 32, 16);
    for x in 60..92 {
        editor.world_mut().claim(Point2D::new(x, 59), BuildClaim::Path(PathType::Pavement));
    }

    let data = LoadedData::load().expect("Failed to load data");
    let palette = data.palettes.get(&Culture::Medieval.palette_id()).expect("palette").clone();
    let mut rng = RNG::new(7);
    let mut plot = plot_from_block(&block).expect("block has cells");

    let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
    let houses = place_block_frontage(
        &block,
        &mut plot,
        &mut ctx,
        Culture::Medieval,
        RoofStyle::Gable(crate::generator::buildings_v2::roof::gable::GablePitch::Slab),
        &[SizeClass::Cottage],
    )
    .await;

    // No two houses share a footprint cell.
    let mut seen: HashSet<Point2D> = HashSet::new();
    for h in &houses {
        for p in h.footprint.filled_points() {
            assert!(seen.insert(p), "Cell {:?} placed twice", p);
        }
    }
}

/// Full end-to-end settlement test using buildings_v2 + city_houses. Generates
/// parcels, partitions urban area into city blocks, paves roads (which also
/// claims `BuildClaim::Path`), then per-block runs the frontage pass followed
/// by the interior fill. Requires a live Minecraft server with the GDMC HTTP
/// mod.
#[tokio::test]
async fn settlement_with_city_houses() {
    use crate::editor::World as RealWorld;
    use crate::generator::buildings::{
        PavingType, get_city_blocks_and_off_limits, smooth_and_pave_road,
    };
    use crate::generator::city_houses::{
        default_interior_size_pool, fill_interior, place_block_frontage, plot_from_block,
    };
    use crate::generator::districts::generate_parcels;
    use crate::generator::materials::PaletteId;
    use crate::generator::buildings_v2::roof::gable::GablePitch;
    use crate::geometry::get_edge;
    use crate::http_mod::GDMCHTTPProvider;
    use crate::util::init_logger;

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = RealWorld::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let mut rng = RNG::new(13);

    // Parcels (creates urban classification used by get_city_blocks_and_off_limits).
    generate_parcels(rng.next_i64().into(), &mut editor).await;

    let data = LoadedData::load().expect("Failed to load data");
    let base_palette_id: PaletteId = "medieval_spruce".into();
    let base_palette = data.palettes.get(&base_palette_id).expect("Base palette not found").clone();

    let roof_palette_ids: Vec<PaletteId> = vec![
        "acacia_wood_roof".into(),
        "brick_roof".into(),
        "oak_wood_roof".into(),
        "red_wood_roof".into(),
    ];

    // City blocks from the urban area.
    let (city_blocks, off_limits) = get_city_blocks_and_off_limits(&mut editor, &mut rng.derive());

    // Pave + claim roads on the outer ring of each block, minus the urban-area
    // edge (avoid paving the outside-of-town border).
    let urban_area_edge = get_edge(&editor.world().get_urban_points());
    let mut outers: HashSet<Point2D> = HashSet::new();
    for block in &city_blocks {
        let (outer, _inner) = crate::geometry::get_outer_and_inner_points(block, 3);
        outers.extend(outer);
    }
    outers.extend(off_limits.iter().copied());
    let road_points: HashSet<Point2D> = outers.difference(&urban_area_edge).copied().collect();

    let paving = crate::generator::settlement::dominant_biome(editor.world())
        .map(PavingType::from_biome)
        .unwrap_or(PavingType::Stone);
    smooth_and_pave_road(&mut editor, &mut rng, &road_points, paving).await;

    let pitches = [
        RoofStyle::Gable(GablePitch::Slab),
        RoofStyle::Gable(GablePitch::Stairs),
        RoofStyle::Gable(GablePitch::Double),
    ];
    let frontage_pool = default_frontage_size_pool();
    let interior_pool = default_interior_size_pool();

    let mut total_buildings = 0usize;
    for (block_idx, block) in city_blocks.iter().enumerate() {
        let (_outer, inner) = crate::geometry::get_outer_and_inner_points(block, 3);
        if inner.is_empty() {
            continue;
        }

        let mut plot = match plot_from_block(&inner) {
            Some(p) => p,
            None => continue,
        };

        let roof_idx = rng.rand_i32_range(0, roof_palette_ids.len() as i32) as usize;
        let roof_palette = data.palettes.get(&roof_palette_ids[roof_idx]).expect("Roof palette not found");
        let palette = base_palette.clone().merged_with(roof_palette);
        let roof_style = pitches[block_idx % pitches.len()];

        let frontage_houses = {
            let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
            place_block_frontage(
                &inner,
                &mut plot,
                &mut ctx,
                Culture::Medieval,
                roof_style,
                &frontage_pool,
            ).await
        };

        let interior_houses = {
            let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
            fill_interior(
                &mut plot,
                &mut ctx,
                Culture::Medieval,
                roof_style,
                &interior_pool,
                20,
            ).await
        };

        total_buildings += frontage_houses.len() + interior_houses.len();
        println!(
            "Block {}: {} frontage + {} interior buildings (block has {} inner cells)",
            block_idx, frontage_houses.len(), interior_houses.len(), inner.len(),
        );
    }

    editor.flush_buffer().await;
    println!(
        "Done — {} total buildings across {} city blocks",
        total_buildings, city_blocks.len(),
    );
}
