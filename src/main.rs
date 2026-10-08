use crate::{editor::World, http_mod::GDMCHTTPProvider, util::init_logger};
#[cfg(feature = "visualizer")]
use crate::{
    data::Loadable,
    generator::{
        buildings::place_buildings,
        chronicle::generate_chronicle,
        data::LoadedData,
        districts::{build_wall, generate_parcels, WallType},
        materials::{Material, MaterialId, Placer},
        style::Style,
        terrain::log_trees,
    },
    noise::RNG,
};


pub mod geometry;
pub mod minecraft;
pub mod http_mod;
pub mod editor;
pub mod generator;
pub mod noise;
pub mod util;
pub mod data;
pub mod config;
pub mod ai;
pub mod render;

#[cfg(feature = "visualizer")]
pub mod visualizer;

#[cfg(feature = "visualizer")]
async fn run_generation(server: &visualizer::VisualizerServer) {
    let provider = GDMCHTTPProvider::new();
    let world = match World::new(&provider).await {
        Ok(w) => w,
        Err(e) => {
            log::error!("Failed to create world: {e}");
            server.update_error(format!("Failed to create world: {e}"));
            return;
        }
    };
    let mut editor = world.get_editor();
    let mut rng = RNG::new(32);

    // === Parcels ===
    server.update_phase(visualizer::GenerationPhase::Parcels);
    generate_parcels(rng.next_i64().into(), &mut editor).await;
    let snap = visualizer::snapshot::extract_full_snapshot(editor.world(), &visualizer::GenerationPhase::Parcels);
    server.update_snapshot(snap);

    // === Load data ===
    let data = LoadedData::load().expect("Failed to load generator data");
    let materials = Material::load().expect("Failed to load materials");
    let material = MaterialId::new("spruce_planks".to_string());
    let mut placer_rng = rng.derive();
    let mut placer: Placer = Placer::new(&materials, &mut placer_rng);
    let urban_points = &editor.world().get_urban_points();

    // === Terrain (trees) ===
    server.update_phase(visualizer::GenerationPhase::Terrain);
    log_trees(&mut editor, urban_points.clone()).await;
    let snap = visualizer::snapshot::extract_full_snapshot(editor.world(), &visualizer::GenerationPhase::Terrain);
    server.update_snapshot(snap);

    // === Buildings ===
    server.update_phase(visualizer::GenerationPhase::Buildings);
    place_buildings(&mut editor, &mut rng.derive(), &data, Style::Medieval, vec![&"medieval_spruce".into()]).await;
    let snap = visualizer::snapshot::extract_full_snapshot(editor.world(), &visualizer::GenerationPhase::Buildings);
    server.update_snapshot(snap);

    // === Walls ===
    server.update_phase(visualizer::GenerationPhase::Walls);
    build_wall(urban_points, &mut editor, &mut rng.derive(), &mut placer, &material, &data.structures, WallType::Palisade, None).await;
    let snap = visualizer::snapshot::extract_full_snapshot(editor.world(), &visualizer::GenerationPhase::Walls);
    server.update_snapshot(snap);

    // === Flush ===
    server.update_phase(visualizer::GenerationPhase::Flush);
    editor.flush_buffer().await;

    // === Chronicle ===
    server.update_phase(visualizer::GenerationPhase::Chronicle);
    let dossier = crate::generator::settlement::minimal_dossier(
        &editor, crate::generator::buildings_v2::Culture::Medieval, &mut rng.derive(),
    );
    if let Err(e) = generate_chronicle(&mut editor, &dossier).await {
        server.log("warn", &format!("Chronicle generation failed: {e}"));
    }

    // === Done ===
    let snap = visualizer::snapshot::extract_full_snapshot(editor.world(), &visualizer::GenerationPhase::Done);
    server.update_snapshot(snap);
    server.update_phase(visualizer::GenerationPhase::Done);
}

/// A fresh seed for an interactive run, taken from the wall clock. Generation is
/// fully deterministic in the seed, so the value printed by [`run_generation_once`]
/// reproduces the exact town (pass it to `generate_town` to rebuild it).
fn random_seed() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(12345)
}

async fn run_generation_once() {
    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    // Random per-run seed so each interactive run is a different town; printed so
    // a good one can be reproduced. `generate_town` takes the seed as a parameter,
    // so tests can still pin it to a fixed value for determinism.
    let seed = crate::noise::Seed(random_seed());
    let run_dir = std::path::PathBuf::from("output/runs").join(seed.0.to_string());
    // Save the terrain before generating: generation mutates the World's
    // heightmaps, and the fixture must hold the server's untouched values.
    let fixture_path = run_dir.join("terrain.fixture.nbt.gz");
    if let Err(e) = world.to_fixture().save(&fixture_path) {
        log::warn!("Failed to save terrain fixture to {}: {e:#}", fixture_path.display());
    }
    let mut editor = world.get_editor();
    println!("Generating town with seed {}", seed.0);
    // None → auto-select the culture from the build area's climate.
    crate::generator::settlement::generate_town(&mut editor, seed, None).await;

    if let Err(e) = save_and_render_town(&editor, seed.0, "live", &run_dir) {
        log::warn!("Failed to save/render town scene in {}: {e:#}", run_dir.display());
    }
}

/// Capture the generated town as `town.scene` in `dir` and render its views
/// next to it.
fn save_and_render_town(editor: &crate::editor::Editor, seed: i64, source: &str, dir: &std::path::Path) -> anyhow::Result<()> {
    let mut scene = editor.capture_scene();
    let b = scene.bounds;
    scene.meta.insert("seed".into(), seed.to_string());
    scene.meta.insert("source".into(), source.into());
    scene.meta.insert("captured_at".into(), chrono::Local::now().to_rfc3339());
    scene.meta.insert(
        "build_area".into(),
        format!("origin {} {} {} size {} {} {}", b.origin.x, b.origin.y, b.origin.z, b.size.x, b.size.y, b.size.z),
    );
    let scene_path = dir.join("town.scene");
    scene.save(&scene_path)?;
    println!("Saved scene ({} blocks) to {}", scene.blocks.len(), scene_path.display());
    for path in crate::render::render_all(&scene, dir)? {
        println!("Rendered {}", path.display());
    }
    println!("Run output in {}", dir.display());
    Ok(())
}

/// `--offline <fixture> [seed]`: build a town from a saved terrain fixture with
/// no server at all, then save and render it exactly like a live run, under
/// `output/runs/offline-<seed>/`.
async fn run_offline(fixture_path: &str, seed_arg: Option<&str>) -> anyhow::Result<()> {
    let t0 = std::time::Instant::now();
    let fixture = crate::editor::fixture::TerrainFixture::load(fixture_path)?;
    let world = World::from_fixture(fixture);
    println!("Loaded fixture {fixture_path} in {:.1?}", t0.elapsed());
    let seed = match seed_arg {
        Some(s) => crate::noise::Seed(s.parse()?),
        None => crate::noise::Seed(random_seed()),
    };
    let mut editor = world.get_offline_editor();
    println!("Generating town offline with seed {}", seed.0);
    let t1 = std::time::Instant::now();
    crate::generator::settlement::generate_town(&mut editor, seed, None).await;
    println!("Generated in {:.1?}", t1.elapsed());
    let dir = std::path::PathBuf::from("output/runs").join(format!("offline-{}", seed.0));
    let t2 = std::time::Instant::now();
    save_and_render_town(&editor, seed.0, "offline", &dir)?;
    println!("Saved and rendered in {:.1?}; total {:.1?}", t2.elapsed(), t0.elapsed());
    Ok(())
}

/// `--render <path.scene> [out_dir]`: re-render a saved scene, no server needed.
fn render_saved_scene(scene_path: &str, out_dir: Option<&str>) -> anyhow::Result<()> {
    let scene_path = std::path::Path::new(scene_path);
    let scene = crate::render::Scene::load(scene_path)?;
    let out_dir = match out_dir {
        Some(d) => std::path::PathBuf::from(d),
        None => scene_path.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
    };
    for path in crate::render::render_all(&scene, &out_dir)? {
        println!("Rendered {}", path.display());
    }
    println!("Render output in {}", out_dir.display());
    Ok(())
}

/// `--fixture <out_path>`: load the World from the server and save only its
/// terrain fixture; generates nothing.
async fn save_fixture_only(out_path: &str) -> anyhow::Result<()> {
    let t = std::time::Instant::now();
    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await?;
    println!("World loaded from server in {:.1?}", t.elapsed());
    let t = std::time::Instant::now();
    let fixture = world.to_fixture();
    let b = fixture.build_area;
    let sections: usize = fixture.chunks.iter().map(|c| c.sections.len()).sum();
    fixture.save(out_path)?;
    println!(
        "Saved terrain fixture to {out_path} in {:.1?}: build area origin {} {} {} size {} {} {}, {} chunks, {} sections kept in Y {:?}, {} bytes",
        t.elapsed(), b.origin.x, b.origin.y, b.origin.z, b.size.x, b.size.y, b.size.z,
        fixture.chunks.len(), sections, fixture.kept_y_range,
        std::fs::metadata(out_path).map(|m| m.len()).unwrap_or(0)
    );
    let t = std::time::Instant::now();
    let reloaded = World::from_fixture(crate::editor::fixture::TerrainFixture::load(out_path)?);
    println!("Reloaded fixture into a World in {:.1?} ({} chunks)", t.elapsed(), reloaded.chunks.len());
    Ok(())
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    init_logger();
    log::info!("Running placement_in_parcels test");

    let args: Vec<String> = std::env::args().collect();
    let arg_after = |flag: &str, n: usize| {
        args.iter().position(|a| a == flag).and_then(|i| args.get(i + n)).map(String::as_str)
    };
    if args.iter().any(|a| a == "--offline") {
        let Some(fixture_path) = arg_after("--offline", 1) else {
            eprintln!("usage: --offline <terrain.fixture.nbt.gz> [seed]");
            std::process::exit(2);
        };
        if let Err(e) = run_offline(fixture_path, arg_after("--offline", 2)).await {
            eprintln!("Offline run failed: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    if args.iter().any(|a| a == "--render") {
        let Some(scene_path) = arg_after("--render", 1) else {
            eprintln!("usage: --render <path.scene> [out_dir]");
            std::process::exit(2);
        };
        if let Err(e) = render_saved_scene(scene_path, arg_after("--render", 2)) {
            eprintln!("Render failed: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    if args.iter().any(|a| a == "--fixture") {
        let Some(out_path) = arg_after("--fixture", 1) else {
            eprintln!("usage: --fixture <out_path>");
            std::process::exit(2);
        };
        if let Err(e) = save_fixture_only(out_path).await {
            eprintln!("Fixture capture failed: {e:#}");
            std::process::exit(1);
        }
        return;
    }

    let use_visualizer = args.iter().any(|arg| arg == "--visualize");

    #[cfg(feature = "visualizer")]
    if use_visualizer {
        let server = visualizer::VisualizerServer::new();
        server.start().await;
        server.update_phase(visualizer::GenerationPhase::Idle);
        log::info!("Visualizer running at http://localhost:3000");
        log::info!("Click 'Generate' in the browser to start generation.");

        loop {
            server.wait_for_generate().await;
            log::info!("Generation requested, starting...");
            run_generation(&server).await;
            log::info!("Generation complete. Waiting for next request...");
        }
    }

    #[cfg(not(feature = "visualizer"))]
    if use_visualizer {
        log::warn!("--visualize flag requires the 'visualizer' feature. Rebuild with: cargo build --features visualizer");
    }

    if !use_visualizer {
        run_generation_once().await;
    }
}
