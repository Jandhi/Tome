use crate::{data::Loadable, editor::World, generator::{buildings::place_buildings, chronicle::{generate_chronicle, SettlementInfo}, data::LoadedData, districts::{build_wall, generate_districts, WallType}, materials::{Material, MaterialId, Placer}, style::Style, terrain::log_trees}, http_mod::GDMCHTTPProvider, noise::RNG, util::init_logger};


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

    // === Districts ===
    server.update_phase(visualizer::GenerationPhase::Districts);
    generate_districts(rng.next_i64().into(), &mut editor).await;
    let mut info = SettlementInfo::new(editor.world());
    let snap = visualizer::snapshot::extract_full_snapshot(editor.world(), &visualizer::GenerationPhase::Districts);
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
    place_buildings(&mut editor, &mut rng.derive(), &data, Style::Medieval, vec![&"medieval_spruce".into()], &info).await;
    info = SettlementInfo::new(editor.world());
    let snap = visualizer::snapshot::extract_full_snapshot(editor.world(), &visualizer::GenerationPhase::Buildings);
    server.update_snapshot(snap);

    // === Walls ===
    server.update_phase(visualizer::GenerationPhase::Walls);
    build_wall(urban_points, &mut editor, &mut rng.derive(), &mut placer, &material, &data.structures, WallType::Palisade).await;
    let snap = visualizer::snapshot::extract_full_snapshot(editor.world(), &visualizer::GenerationPhase::Walls);
    server.update_snapshot(snap);

    // === Flush ===
    server.update_phase(visualizer::GenerationPhase::Flush);
    editor.flush_buffer().await;

    // === Chronicle ===
    server.update_phase(visualizer::GenerationPhase::Chronicle);
    if let Err(e) = generate_chronicle(&mut editor, &mut info).await {
        server.log("warn", &format!("Chronicle generation failed: {e}"));
    }

    // === Done ===
    let snap = visualizer::snapshot::extract_full_snapshot(editor.world(), &visualizer::GenerationPhase::Done);
    server.update_snapshot(snap);
    server.update_phase(visualizer::GenerationPhase::Done);
}

async fn run_generation_once() {
    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();
    let mut rng = RNG::new(32);

    generate_districts(rng.next_i64().into(), &mut editor).await;
    let mut info = SettlementInfo::new(editor.world());

    let data = LoadedData::load().expect("Failed to load generator data");
    let materials = Material::load().expect("Failed to load materials");
    let material = MaterialId::new("spruce_planks".to_string());
    let mut placer_rng = rng.derive();
    let mut placer: Placer = Placer::new(&materials, &mut placer_rng);
    let urban_points = &editor.world().get_urban_points();

    log_trees(&mut editor, urban_points.clone()).await;
    place_buildings(&mut editor, &mut rng.derive(), &data, Style::Medieval, vec![&"medieval_spruce".into()], &info).await;
    info = SettlementInfo::new(editor.world());
    build_wall(urban_points, &mut editor, &mut rng.derive(), &mut placer, &material, &data.structures, WallType::Palisade).await;
    editor.flush_buffer().await;
    let _ = generate_chronicle(&mut editor, &mut info).await;
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    init_logger();
    log::info!("Running placement_in_districts test");

    let use_visualizer = std::env::args().any(|arg| arg == "--visualize");

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
