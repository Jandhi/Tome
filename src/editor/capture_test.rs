//! Offline tests for `Editor::capture_scene` and `TerrainSnapshot`.

use crate::util::HashMap;

use fastnbt::LongArray;

use super::terrain_snapshot::TerrainSnapshot;
use super::World;
use crate::geometry::{Point2D, Point3D, Rect3D};
use crate::minecraft::chunk::{BlockStates, ChunkSection, HeightMaps};
use crate::minecraft::{Block, Chunk};
use crate::render::Scene;

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join("tome_capture_test").join(name)
}

#[tokio::test]
async fn capture_scene_layers_placed_blocks_over_terrain() {
    let area = Rect3D { origin: Point3D::new(100, 0, -50), size: Point3D::new(6, 128, 5) };
    let editor = World::synthetic(area, 64).get_offline_editor();
    let o = area.origin;

    // Local coords: synthetic ground height is 64, so the surface block is at y 63.
    let stairs: Block = Block::new(
        "minecraft:oak_stairs".into(),
        Some(std::collections::HashMap::from([("half".to_string(), "bottom".to_string()), ("facing".to_string(), "north".to_string())])),
        None,
    );
    editor.place_block(&"stone".into(), Point3D::new(2, 64, 2)).await;
    editor.place_block(&stairs, Point3D::new(1, 65, 1)).await;
    editor.place_block_forced(&"minecraft:air".into(), Point3D::new(3, 63, 3)).await;
    editor.place_block_forced(&"minecraft:oak_planks".into(), Point3D::new(4, 63, 4)).await;

    let scene = editor.capture_scene();
    let path = temp_path("capture.scene");
    scene.save(&path).unwrap();
    let s = Scene::load(&path).unwrap();

    assert_eq!(s.bounds.origin, o);
    // Placed blocks at absolute coords, bare ids namespaced, state keys sorted.
    assert_eq!(s.block_str(o + Point3D::new(2, 64, 2)), Some("minecraft:stone"));
    assert_eq!(s.block_str(o + Point3D::new(1, 65, 1)), Some("minecraft:oak_stairs[facing=north,half=bottom]"));
    // Placements override terrain, and air (cleared terrain) survives.
    assert_eq!(s.block_str(o + Point3D::new(4, 63, 4)), Some("minecraft:oak_planks"));
    assert_eq!(s.block_str(o + Point3D::new(3, 63, 3)), Some("minecraft:air"));
    // Flat synthetic terrain: exactly one surface block per column, nothing else.
    for x in 0..area.size.x {
        for z in 0..area.size.z {
            let col: Vec<_> = s.iter().filter(|(p, _)| p.x == o.x + x && p.z == o.z + z && p.y < o.y + 64).collect();
            assert_eq!(col.len(), 1, "column ({x},{z}) should hold one terrain block, got {col:?}");
            assert_eq!(col[0].0.y, o.y + 63);
        }
    }
    assert_eq!(s.block_str(o + Point3D::new(0, 63, 0)), Some("minecraft:grass_block"));
    assert_eq!(s.blocks.len(), (area.size.x * area.size.z) as usize + 2);
}

fn heightmaps(size_x: usize, size_z: usize, f: impl Fn(usize, usize) -> i32) -> Vec<Vec<i32>> {
    (0..size_x).map(|x| (0..size_z).map(|z| f(x, z)).collect()).collect()
}

#[test]
fn terrain_fixture_round_trips() {
    let area = Rect3D { origin: Point3D::new(-40, -64, 20), size: Point3D::new(4, 384, 3) };
    let fixture = TerrainSnapshot::new(
        area,
        Vec::new(),
        heightmaps(4, 3, |x, z| 60 + x as i32 + 2 * z as i32),
        heightmaps(4, 3, |x, _| 58 + x as i32),
        heightmaps(4, 3, |x, z| 65 + (x * z) as i32),
    );
    assert_eq!(fixture.kept_y_range, (58 - 16, 65 + 6 + 16));
    let path = temp_path("terrain.fixture.nbt.gz");
    fixture.save(&path).unwrap();
    let loaded = TerrainSnapshot::load(&path).unwrap();

    let direct = World::from_parts(
        area,
        HashMap::default(),
        fixture.ground_map.clone(),
        fixture.ocean_map.clone(),
        fixture.motion_blocking_map.clone(),
    );
    let world = World::from_snapshot(loaded);
    assert_eq!(world.build_area.origin, area.origin);
    for x in 0..4 {
        for z in 0..3 {
            let p = Point2D::new(x, z);
            assert_eq!(world.get_height_at(p), direct.get_height_at(p));
            assert_eq!(world.get_height_at(p), Some(fixture.ground_map[x as usize][z as usize] - area.origin.y));
            assert_eq!(world.get_ocean_floor_height_at(p), direct.get_ocean_floor_height_at(p));
            assert_eq!(world.get_motion_blocking_height_at(p), direct.get_motion_blocking_height_at(p));
        }
    }
    // to_snapshot restores the absolute heights.
    assert_eq!(world.to_snapshot().ground_map, fixture.ground_map);
}

/// Chunks hold fastnbt `LongArray`s, whose serde form is a special token map;
/// make sure it survives the NBT round trip and block lookups still work.
#[test]
fn terrain_fixture_round_trips_chunk_data() {
    let mut longs = vec![0i64; 256]; // 4096 blocks × 4 bits (min palette width)
    longs[0] = 1; // block index 0 (local x0 y0 z0) → palette entry 1
    let la = || LongArray::new(vec![0; 37]);
    let block = |name: &str| crate::minecraft::chunk::Block { name: name.into(), properties: None };
    let section = |y: i32| ChunkSection {
        y,
        block_states: Some(BlockStates { palette: vec![block("minecraft:stone")], data: None }),
        biomes: None,
    };
    let chunk = Chunk {
        x_pos: 0,
        y_pos: 4,
        z_pos: 0,
        sections: vec![section(-2), section(20), ChunkSection {
            y: 4,
            block_states: Some(BlockStates {
                palette: vec![block("minecraft:air"), block("minecraft:stone")],
                data: Some(LongArray::new(longs)),
            }),
            biomes: None,
        }],
        heightmaps: HeightMaps {
            motion_blocking: la(),
            motion_blocking_no_leaves: la(),
            ocean_floor: la(),
            ocean_floor_wg: None,
            world_surface: la(),
            world_surface_wg: Some(la()),
        },
        block_entities: Some(vec![fastnbt::Value::String("chest".into())]),
    };
    let area = Rect3D { origin: Point3D::new(0, 64, 0), size: Point3D::new(2, 16, 2) };
    let fixture = TerrainSnapshot::new(
        area,
        vec![chunk],
        heightmaps(2, 2, |_, _| 65),
        heightmaps(2, 2, |_, _| 65),
        heightmaps(2, 2, |_, _| 65),
    );
    // Band [49, 81] keeps sections 3..=5: section 4 survives, -2 and 20 are dropped.
    assert_eq!(fixture.chunks[0].sections.iter().map(|s| s.y).collect::<Vec<_>>(), vec![4]);
    let path = temp_path("chunk.fixture.nbt.gz");
    fixture.save(&path).unwrap();
    let loaded = TerrainSnapshot::load(&path).unwrap();
    assert_eq!(loaded.kept_y_range, (49, 81));
    assert_eq!(loaded.chunks[0].heightmaps.world_surface_wg.as_ref().map(|l| l.len()), Some(37));
    assert!(loaded.chunks[0].heightmaps.ocean_floor_wg.is_none());
    let world = World::from_snapshot(loaded);
    assert_eq!(world.get_block(Point3D::new(0, 0, 0)).map(|b| b.id), Some("minecraft:stone".into()));
    assert_eq!(world.get_block(Point3D::new(1, 0, 0)).map(|b| b.id), Some("minecraft:air".into()));
    assert_eq!(world.get_ground_block(Point2D::new(0, 0)).map(|b| b.id.as_str()), Some("minecraft:stone"));
}

/// Same fixture + same seed must give the same town, block for block. Guards
/// against per-process nondeterminism (e.g. `RandomState` hash iteration order
/// leaking into RNG draws). Slow (two full towns), hence `#[ignore]`:
///
/// ```text
/// TOME_FIXTURE=output/runs/<seed>/terrain.fixture.nbt.gz cargo test offline_settlement_is_deterministic -- --ignored --nocapture
/// ```
///
/// std `RandomState` re-keys every new map, so a stray std map feeding the RNG
/// shows up here even within one process.
#[tokio::test]
#[ignore]
async fn offline_settlement_is_deterministic() {
    let Ok(path) = std::env::var("TOME_FIXTURE") else {
        eprintln!("offline_settlement_is_deterministic: TOME_FIXTURE not set, skipping");
        return;
    };
    // generate_settlement ends with the AI chronicle (a written book, no blocks),
    // which reads OPENAI_KEY from .env like main does.
    dotenv::dotenv().ok();
    let seed = crate::noise::Seed(1791494029742476500);

    let mut towns = Vec::new();
    for _ in 0..2 {
        let fixture = TerrainSnapshot::load(&path).expect("load fixture");
        let mut editor = World::from_snapshot(fixture).get_offline_editor();
        crate::generator::settlement::generate_settlement(&mut editor, seed, None).await;
        let scene = editor.capture_scene();
        let blocks: HashMap<Point3D, String> = scene.iter().map(|(p, s)| (p, s.to_string())).collect();
        towns.push(blocks);
    }
    let (a, b) = (&towns[0], &towns[1]);
    let diff = a.iter().filter(|(p, s)| b.get(p) != Some(s)).count()
        + b.keys().filter(|p| !a.contains_key(p)).count();
    assert_eq!(diff, 0, "towns differ in {diff} cells ({} vs {} blocks)", a.len(), b.len());
}
