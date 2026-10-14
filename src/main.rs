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
pub mod outputs;

use crate::outputs::Artifact;

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

async fn run_generation_once(full: bool, seed_override: Option<i64>, specs: TownSpecs, out: crate::outputs::Outputs) {
    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    // Random per-run seed so each interactive run is a different town; printed so
    // a good one can be reproduced. `generate_town` takes the seed as a parameter,
    // so tests can still pin it to a fixed value for determinism.
    let seed = crate::noise::Seed(seed_override.unwrap_or_else(random_seed));
    let run_dir = std::path::PathBuf::from("output/runs").join(seed.0.to_string());
    // Save the terrain before generating: generation mutates the World's
    // heightmaps, and the fixture must hold the server's untouched values.
    if out.has(Artifact::Fixture) {
        let fixture_path = run_dir.join("terrain.fixture.nbt.gz");
        if let Err(e) = world.to_fixture().save(&fixture_path) {
            log::warn!("Failed to save terrain fixture to {}: {e:#}", fixture_path.display());
        }
    }
    let mut editor = world.get_editor();
    println!("Generating town with seed {}", seed.0);
    // None → auto-select the culture from the build area's climate.
    let options = crate::generator::settlement::TownOptions { chronicle: full, house_specs: specs };
    let mut report = crate::generator::settlement::generate_town_with(&mut editor, seed, None, &options).await;

    if let Err(e) = save_and_render_town(&editor, seed.0, "live", &run_dir, &mut report, out) {
        log::warn!("Failed to save/render town scene in {}: {e:#}", run_dir.display());
    }
}

/// Write the run's artifacts that `out` asks for into `dir`: `metrics.json`
/// (the [`TownReport`](crate::generator::report::TownReport)), every house's
/// spec (`houses/house_<n>.spec.yaml`), the captured `town.scene`, its rendered
/// views and `viewer.html`. The scene is only captured when something needs it.
fn save_and_render_town(
    editor: &crate::editor::Editor,
    seed: i64,
    source: &str,
    dir: &std::path::Path,
    report: &mut crate::generator::report::TownReport,
    out: crate::outputs::Outputs,
) -> anyhow::Result<()> {
    let captured_at = chrono::Local::now().to_rfc3339();
    report.source = source.into();
    report.captured_at = captured_at.clone();
    std::fs::create_dir_all(dir)?;
    if out.has(Artifact::Metrics) {
        let metrics_path = dir.join("metrics.json");
        report.save(&metrics_path)?;
        println!("Saved metrics to {}", metrics_path.display());
    }
    if out.has(Artifact::Specs) {
        // One spec per buildings_v2 house; the directory is rewritten whole so a
        // rerun with fewer houses leaves no stale specs behind.
        let houses_dir = dir.join("houses");
        if houses_dir.exists() {
            std::fs::remove_dir_all(&houses_dir)?;
        }
        std::fs::create_dir_all(&houses_dir)?;
        for (n, spec) in report.house_specs.iter().enumerate() {
            spec.save(houses_dir.join(format!("house_{n:03}.spec.yaml")))?;
        }
        println!("Saved {} house specs to {}", report.house_specs.len(), houses_dir.display());
    }
    if [Artifact::Scene, Artifact::Renders, Artifact::Viewer].iter().any(|a| out.has(*a)) {
        let mut scene = editor.capture_scene();
        let b = scene.bounds;
        scene.meta.insert("seed".into(), seed.to_string());
        scene.meta.insert("source".into(), source.into());
        scene.meta.insert("captured_at".into(), captured_at);
        scene.meta.insert(
            "build_area".into(),
            format!("origin {} {} {} size {} {} {}", b.origin.x, b.origin.y, b.origin.z, b.size.x, b.size.y, b.size.z),
        );
        if out.has(Artifact::Scene) {
            let scene_path = dir.join("town.scene");
            scene.save(&scene_path)?;
            println!("Saved scene ({} blocks) to {}", scene.blocks.len(), scene_path.display());
        }
        if out.has(Artifact::Renders) {
            for path in crate::render::render_all(&scene, dir)? {
                println!("Rendered {}", path.display());
            }
        }
        if out.has(Artifact::Viewer) {
            write_viewer(&scene, &dir.join("viewer.html"))?;
        }
    }
    println!("Run output in {} ({})", dir.display(), out.names());
    Ok(())
}

/// House specs a town run builds, `(file name, spec)`.
type TownSpecs = Vec<(String, crate::generator::buildings_v2::spec::HouseSpec)>;

/// Every `--spec <file>` of a town run (repeatable), read and parsed up front so
/// a typo fails before anything is generated. The town checks the rest once it
/// knows its culture, and seats each on the first road slot it fits.
fn town_specs(args: &[String]) -> anyhow::Result<TownSpecs> {
    args.windows(2)
        .filter(|w| w[0] == "--spec")
        .map(|w| {
            let path = std::path::Path::new(&w[1]);
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| w[1].clone());
            Ok((name, crate::generator::buildings_v2::spec::HouseSpec::load(path)?))
        })
        .collect()
}

/// `--offline <fixture> [seed]`: build a town from a saved terrain fixture with
/// no server at all, then save and render it exactly like a live run, under
/// `output/runs/offline-<seed>/`.
async fn run_offline(fixture_path: &str, seed_arg: Option<&str>, specs: TownSpecs, out: crate::outputs::Outputs) -> anyhow::Result<()> {
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
    let options = crate::generator::settlement::TownOptions { house_specs: specs, ..Default::default() };
    let mut report = crate::generator::settlement::generate_town_with(&mut editor, seed, None, &options).await;
    println!("Generated in {:.1?}", t1.elapsed());
    let dir = std::path::PathBuf::from("output/runs").join(format!("offline-{}", seed.0));
    let t2 = std::time::Instant::now();
    // The terrain came from a fixture file: there is none to write.
    save_and_render_town(&editor, seed.0, "offline", &dir, &mut report, out.without(Artifact::Fixture))?;
    println!("Saved and rendered in {:.1?}; total {:.1?}", t2.elapsed(), t0.elapsed());
    Ok(())
}

/// `--render <path.scene> [out_dir] [--light <mode>]`: re-render a saved
/// scene, no server needed. `--light` reports the town's light map (timing
/// and levels) and also writes every view lit under that mode into
/// `<out_dir>/lit-<mode>/`, beside the unlit ones.
fn render_saved_scene(scene_path: &str, out_dir: Option<&str>, lighting: Option<crate::render::light::Lighting>) -> anyhow::Result<()> {
    let scene_path = std::path::Path::new(scene_path);
    let scene = crate::render::Scene::load(scene_path)?;
    let out_dir = match out_dir {
        Some(d) => std::path::PathBuf::from(d),
        None => scene_path.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
    };
    // One grid (and, with --light, one light map) serves the report and
    // both sets of views.
    let t = std::time::Instant::now();
    let grid = crate::render::grid::VoxelGrid::from_scene(&scene, None);
    let grid_time = t.elapsed();
    match &grid {
        None => println!("Nothing visible in the scene to render"),
        Some(g) => {
            let defaults = crate::render::RenderOptions::default();
            for path in crate::render::render_grid_views(&scene, g, None, &defaults, &out_dir, "")? {
                println!("Rendered {}", path.display());
            }
            if let Some(lighting) = lighting {
                let t = std::time::Instant::now();
                let map = crate::render::light::LightMap::new(g);
                report_light_map(g, &map, lighting, grid_time, t.elapsed());
                let lit_dir = out_dir.join(format!("lit-{}", lighting.tag()));
                let opts = crate::render::RenderOptions { lighting: Some(lighting), ..defaults };
                for path in crate::render::render_grid_views(&scene, g, Some(&map), &opts, &lit_dir, "")? {
                    println!("Rendered {} (lit, {})", path.display(), lighting.tag());
                }
            }
        }
    }
    write_viewer(&scene, &out_dir.join("viewer.html"))?;
    println!("Render output in {}", out_dir.display());
    Ok(())
}

/// Print how long the scene's grid and light map took and how the map's
/// light is distributed.
fn report_light_map(
    g: &crate::render::grid::VoxelGrid,
    map: &crate::render::light::LightMap,
    lighting: crate::render::light::Lighting,
    grid_time: std::time::Duration,
    map_time: std::time::Duration,
) {
    let (sky, block) = map.histogram();
    let open: usize = sky.iter().sum();
    let pct = |n: usize| if open == 0 { 0.0 } else { 100.0 * n as f64 / open as f64 };
    let lit_by_blocks: usize = block[1..].iter().sum();
    let mean = |levels: &[usize; 16]| {
        if open == 0 { 0.0 } else { levels.iter().enumerate().map(|(l, &n)| l * n).sum::<usize>() as f64 / open as f64 }
    };
    println!(
        "Light map ({}, daylight {:.2}): {}x{}x{} = {} cells (grid {:.1?}, light {:.1?}); {} emitters; \
         of {} non-solid cells {:.1}% have full sky light, {:.1}% are in deep shade (sky <= 4), \
         {:.1}% get any block light; mean sky {:.1}, mean block {:.1}",
        lighting.tag(), lighting.daylight, g.nx, g.ny, g.nz, map.len(), grid_time, map_time, map.emitters,
        open, pct(sky[15]), pct(sky[..=4].iter().sum()), pct(lit_by_blocks), mean(&sky), mean(&block),
    );
}

/// `--view <path.scene> [out.html] [--light <mode>]`: write the interactive
/// first-person viewer page for a saved scene (default: `viewer.html` beside
/// the scene, `viewer-<day|night|dusk|light0.30>.html` when `--light` bakes
/// Minecraft-style lighting into it). Open the file in a browser; no server
/// needed.
fn view_saved_scene(scene_path: &str, out: Option<&str>, lighting: Option<crate::render::light::Lighting>) -> anyhow::Result<()> {
    let scene_path = std::path::Path::new(scene_path);
    let scene = crate::render::Scene::load(scene_path)?;
    let out = match (out, lighting) {
        (Some(p), _) => std::path::PathBuf::from(p),
        (None, Some(l)) => scene_path.with_file_name(format!("viewer-{}.html", l.tag())),
        (None, None) => scene_path.with_file_name("viewer.html"),
    };
    write_viewer_with(&scene, &out, lighting)
}

fn write_viewer(scene: &crate::render::Scene, out: &std::path::Path) -> anyhow::Result<()> {
    write_viewer_with(scene, out, None)
}

fn write_viewer_with(scene: &crate::render::Scene, out: &std::path::Path, lighting: Option<crate::render::light::Lighting>) -> anyhow::Result<()> {
    let t = std::time::Instant::now();
    let s = crate::render::viewer::write_viewer_html_with(scene, None, out, lighting)?;
    let lit = match lighting {
        Some(l) => format!(", lit {} ({:.1} MB light, light map {:.1?})", l.tag(), s.light_bytes as f64 / 1e6, s.light_map),
        None => String::new(),
    };
    println!(
        "Viewer: {} faces ({} merged full-block, {} partial), {:.1} MB buffer{lit}, {:.1} MB html in {:.1?} -> {}",
        s.faces,
        s.merged_faces,
        s.partial_faces,
        s.buffer_bytes as f64 / 1e6,
        s.html_bytes as f64 / 1e6,
        t.elapsed(),
        out.display()
    );
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
/// [--variants] [--live] [--px n] [--light day|night|dusk|<0..1>] [--spec file.yaml ...]`: build individual houses and render each one,
/// under `output/houses/[live-]<culture>-<seed>[...]/`. `--spec` (repeatable) builds
/// house specs instead, under `output/houses/spec-<stem>[+n][-x<count>]/`: one house per
/// spec, or `--count N` of each (a sweep: only the spec's open fields vary). The flags fill
/// a spec's open fields (`--culture`, `--size`, `--jetty`, `--knee-wall`, `--tall-top`, `--rects`). Offline on a blank flat
/// world by default; `--live` builds into the server's build area. `--variants`
/// builds one row per upper-storey variant (base, knee, tall, both). `--light`
/// lights the room views with sky light and lamps (the directory gains `-day`, `-night`, ...).
async fn run_houses(args: &[String], out: crate::outputs::Outputs) -> anyhow::Result<()> {
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
    // Every `--spec <file>` (repeatable).
    let specs: Vec<std::path::PathBuf> = args
        .windows(2)
        .filter(|w| w[0] == "--spec")
        .map(|w| std::path::PathBuf::from(&w[1]))
        .collect();
    let opts = ShowcaseOptions {
        culture,
        seed: arg_after("--seed", 1).map(str::parse).transpose()?.unwrap_or(42),
        // One house per spec unless --count asks for a sweep.
        count: arg_after("--count", 1).map(str::parse).transpose()?.unwrap_or(if specs.is_empty() { 6 } else { 1 }),
        size,
        jetty: args.iter().any(|a| a == "--jetty"),
        knee_wall: args.iter().any(|a| a == "--knee-wall"),
        tall_top: args.iter().any(|a| a == "--tall-top"),
        variants: args.iter().any(|a| a == "--variants"),
        live: args.iter().any(|a| a == "--live"),
        px: arg_after("--px", 1).map(str::parse).transpose()?.unwrap_or(16),
        rects: arg_after("--rects", 1).map(parse_rects).transpose()?,
        lighting: arg_after("--light", 1).map(crate::render::light::Lighting::parse).transpose()?,
        specs,
        out,
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
    // `--out` picks which artifacts a run writes (see `outputs`); checked
    // before anything is generated so a typo fails at once.
    use crate::outputs::Outputs;
    let houses = args.iter().any(|a| a == "--houses");
    let defaults = if houses { Outputs::houses_default() } else { Outputs::town_default() };
    let out = match Outputs::parse(arg_after("--out", 1), defaults) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e:#}");
            return;
        }
    };
    if houses {
        if let Err(e) = run_houses(&args, out).await {
            eprintln!("House showcase failed: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    if args.iter().any(|a| a == "--furniture") {
        let Some(names) = arg_after("--furniture", 1) else {
            eprintln!("usage: --furniture <name[,name...]> [out_dir]");
            std::process::exit(2);
        };
        let names: Vec<String> = names.split(',').map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect();
        let out_dir = std::path::PathBuf::from(arg_after("--furniture", 2).unwrap_or("output/furniture"));
        let result = crate::generator::buildings_v2::furnish::data::FurnitureData::load()
            .and_then(|data| crate::generator::buildings_v2::furnish::preview::render_previews(&data, &names, &out_dir));
        match result {
            Ok(paths) => paths.iter().for_each(|p| println!("Rendered {}", p.display())),
            Err(e) => {
                eprintln!("Furniture preview failed: {e:#}");
                std::process::exit(1);
            }
        }
        return;
    }
    let specs = match town_specs(&args) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e:#}");
            std::process::exit(2);
        }
    };
    if args.iter().any(|a| a == "--offline") {
        let Some(fixture_path) = arg_after("--offline", 1) else {
            eprintln!("usage: --offline <terrain.fixture.nbt.gz> [seed]");
            std::process::exit(2);
        };
        // The seed is optional, so a flag right after the fixture is not it.
        let seed = arg_after("--offline", 2).filter(|s| !s.starts_with("--"));
        if let Err(e) = run_offline(fixture_path, seed, specs, out).await {
            eprintln!("Offline run failed: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    if args.iter().any(|a| a == "--render") {
        let Some(scene_path) = arg_after("--render", 1) else {
            eprintln!("usage: --render <path.scene> [out_dir] [--light day|night|dusk|<0..1>]");
            std::process::exit(2);
        };
        // The optional out_dir must not be the --light flag itself.
        let out_dir = arg_after("--render", 2).filter(|d| !d.starts_with("--"));
        let lighting = match arg_after("--light", 1).map(crate::render::light::Lighting::parse).transpose() {
            Ok(l) => l,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(2);
            }
        };
        if let Err(e) = render_saved_scene(scene_path, out_dir, lighting) {
            eprintln!("Render failed: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    if args.iter().any(|a| a == "--view") {
        let Some(scene_path) = arg_after("--view", 1) else {
            eprintln!("usage: --view <path.scene> [out.html] [--light day|night|dusk|<0..1>]");
            std::process::exit(2);
        };
        // The optional out.html must not be the --light flag itself.
        let out = arg_after("--view", 2).filter(|d| !d.starts_with("--"));
        let lighting = match arg_after("--light", 1).map(crate::render::light::Lighting::parse).transpose() {
            Ok(l) => l,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(2);
            }
        };
        if let Err(e) = view_saved_scene(scene_path, out, lighting) {
            eprintln!("Viewer export failed: {e:#}");
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
        run_generation_once(full, seed_arg, specs, out).await;
    }
}
