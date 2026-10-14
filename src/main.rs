use crate::util::init_logger;
#[cfg(feature = "visualizer")]
use crate::{
    data::Loadable,
    editor::World,
    http_mod::GDMCHTTPProvider,
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
pub mod cli;

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
        if let Err(e) = cli::houses::run_houses(&args).await {
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
        if let Err(e) = cli::houses::run_furniture(names, arg_after("--furniture", 2)) {
            eprintln!("Furniture preview failed: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    if args.iter().any(|a| a == "--offline") {
        let Some(snapshot_path) = arg_after("--offline", 1) else {
            eprintln!("usage: --offline <terrain.fixture.nbt.gz> [seed]");
            std::process::exit(2);
        };
        if let Err(e) = cli::offline::run_offline(snapshot_path, arg_after("--offline", 2)).await {
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
        if let Err(e) = cli::render::render_saved_scene(scene_path, out_dir, lighting) {
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
        if let Err(e) = cli::view::view_saved_scene(scene_path, out, lighting) {
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
        if let Err(e) = cli::compare::compare_scenes(a, b, arg_after("--compare", 3)) {
            eprintln!("Compare failed: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    if args.iter().any(|a| a == "--snapshot") {
        let Some(out_path) = arg_after("--snapshot", 1) else {
            eprintln!("usage: --snapshot <out_path>");
            std::process::exit(2);
        };
        if let Err(e) = cli::live::save_snapshot_only(out_path).await {
            eprintln!("Snapshot capture failed: {e:#}");
            std::process::exit(1);
        }
        return;
    }

    let use_visualizer = args.iter().any(|arg| arg == "--visualize");
    // Opt-in extras for a live run: currently the AI-written chronicle book.
    let full = args.iter().any(|arg| arg == "--full");
    // `--seed <n>` pins a live run's seed (default: wall clock) so a settlement can be
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
        cli::live::run_generation_once(full, seed_arg).await;
    }
}
