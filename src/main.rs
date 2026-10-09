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

async fn run_generation_once(full: bool, seed_override: Option<i64>) {
    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    // Random per-run seed so each interactive run is a different town; printed so
    // a good one can be reproduced. `generate_town` takes the seed as a parameter,
    // so tests can still pin it to a fixed value for determinism.
    let seed = crate::noise::Seed(seed_override.unwrap_or_else(random_seed));
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
    let options = crate::generator::settlement::TownOptions { chronicle: full };
    let mut report = crate::generator::settlement::generate_town_with(&mut editor, seed, None, &options).await;

    if let Err(e) = save_and_render_town(&editor, seed.0, "live", &run_dir, &mut report) {
        log::warn!("Failed to save/render town scene in {}: {e:#}", run_dir.display());
    }
}

/// Capture the generated town as `town.scene` in `dir`, write the run's
/// `metrics.json` (the [`TownReport`](crate::generator::report::TownReport))
/// beside it, and render its views next to them.
fn save_and_render_town(
    editor: &crate::editor::Editor,
    seed: i64,
    source: &str,
    dir: &std::path::Path,
    report: &mut crate::generator::report::TownReport,
) -> anyhow::Result<()> {
    let captured_at = chrono::Local::now().to_rfc3339();
    report.source = source.into();
    report.captured_at = captured_at.clone();
    let metrics_path = dir.join("metrics.json");
    report.save(&metrics_path)?;
    println!("Saved metrics to {}", metrics_path.display());
    let mut scene = editor.capture_scene();
    let b = scene.bounds;
    scene.meta.insert("seed".into(), seed.to_string());
    scene.meta.insert("source".into(), source.into());
    scene.meta.insert("captured_at".into(), captured_at);
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
    let options = crate::generator::settlement::TownOptions::default();
    let mut report = crate::generator::settlement::generate_town_with(&mut editor, seed, None, &options).await;
    println!("Generated in {:.1?}", t1.elapsed());
    let dir = std::path::PathBuf::from("output/runs").join(format!("offline-{}", seed.0));
    let t2 = std::time::Instant::now();
    save_and_render_town(&editor, seed.0, "offline", &dir, &mut report)?;
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

/// `--compare <a.scene> <b.scene> [out_dir]`: block-level diff of two saved
/// scenes. Prints a summary table and writes `diff.json` and `diff_top.png`
/// into `out_dir` (default: the directory of A).
fn compare_scenes(a_path: &str, b_path: &str, out_dir: Option<&str>) -> anyhow::Result<()> {
    let a_path = std::path::Path::new(a_path);
    let a = crate::render::Scene::load(a_path)?;
    let b = crate::render::Scene::load(b_path)?;
    let out_dir = match out_dir {
        Some(d) => std::path::PathBuf::from(d),
        None => a_path.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
    };
    let diff = crate::render::diff::diff_scenes(&a, &b);
    println!("A = {}\nB = {}", a_path.display(), b_path);
    print!("{}", diff.table(10));
    std::fs::create_dir_all(&out_dir)?;
    let json_path = out_dir.join("diff.json");
    diff.save(&json_path)?;
    let png_path = out_dir.join("diff_top.png");
    crate::render::diff::render_diff_top(&a, &b, 4)?.save_png(&png_path)?;
    println!("\nWrote {} and {} (red = only in A, green = only in B, yellow = changed)", json_path.display(), png_path.display());
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

/// `--houses [--culture c] [--seed n] [--count n] [--size s] [--rects "x,z,w,h;..."] [--jetty] [--knee-wall] [--tall-top]
/// [--variants] [--live] [--px n]`: build individual houses and render each one,
/// under `output/houses/[live-]<culture>-<seed>[...]/`. Offline on a blank flat
/// world by default; `--live` builds into the server's build area. `--variants`
/// builds one row per upper-storey variant (base, knee, tall, both).
async fn run_houses(args: &[String]) -> anyhow::Result<()> {
    let arg_after = |flag: &str, n: usize| {
        args.iter().position(|a| a == flag).and_then(|i| args.get(i + n)).map(String::as_str)
    };
    use crate::generator::buildings_v2::{footprint::SizeClass, showcase::{parse_rects, run_showcase, ShowcaseOptions, SizeSpec}, Culture};
    let culture = match arg_after("--culture", 1).unwrap_or("medieval") {
        "medieval" => Culture::Medieval,
        "desert" => Culture::Desert,
        "japanese" => Culture::Japanese,
        other => anyhow::bail!("unknown --culture {other} (medieval|desert|japanese)"),
    };
    let size = match arg_after("--size", 1).unwrap_or("mixed") {
        "mixed" => SizeSpec::Mixed,
        "cottage" => SizeSpec::One(SizeClass::Cottage),
        "house" => SizeSpec::One(SizeClass::House),
        "hall" => SizeSpec::One(SizeClass::Hall),
        "manor" => SizeSpec::One(SizeClass::Manor),
        other => anyhow::bail!("unknown --size {other} (cottage|house|hall|manor|mixed)"),
    };
    let opts = ShowcaseOptions {
        culture,
        seed: arg_after("--seed", 1).map(str::parse).transpose()?.unwrap_or(42),
        count: arg_after("--count", 1).map(str::parse).transpose()?.unwrap_or(6),
        size,
        jetty: args.iter().any(|a| a == "--jetty"),
        knee_wall: args.iter().any(|a| a == "--knee-wall"),
        tall_top: args.iter().any(|a| a == "--tall-top"),
        variants: args.iter().any(|a| a == "--variants"),
        live: args.iter().any(|a| a == "--live"),
        px: arg_after("--px", 1).map(str::parse).transpose()?.unwrap_or(16),
        rects: arg_after("--rects", 1).map(parse_rects).transpose()?,
    };
    run_showcase(&opts).await?;
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
    if args.iter().any(|a| a == "--houses") {
        if let Err(e) = run_houses(&args).await {
            eprintln!("House showcase failed: {e:#}");
            std::process::exit(1);
        }
        return;
    }
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
    if args.iter().any(|a| a == "--compare") {
        let (Some(a), Some(b)) = (arg_after("--compare", 1), arg_after("--compare", 2)) else {
            eprintln!("usage: --compare <a.scene> <b.scene> [out_dir]");
            std::process::exit(2);
        };
        if let Err(e) = compare_scenes(a, b, arg_after("--compare", 3)) {
            eprintln!("Compare failed: {e:#}");
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
    // Opt-in extras for a live run: currently the AI-written chronicle book.
    let full = args.iter().any(|arg| arg == "--full");
    // `--seed <n>` pins a live run's seed (default: wall clock) so a town can be
    // rebuilt, or compared against its offline twin.
    let seed_arg = arg_after("--seed", 1).map(|s| s.parse::<i64>().expect("--seed takes an integer"));

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
        run_generation_once(full, seed_arg).await;
    }
}
