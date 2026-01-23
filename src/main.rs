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

#[tokio::main]
async fn main() {
    println!("Running placement_in_districts test");
    dotenv::dotenv().ok();
    init_logger();

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
    let mut placer: Placer = Placer::new(
        &materials,
        &mut placer_rng,
    );
    let urban_points = &editor.world().get_urban_points();
    log_trees(&mut editor, urban_points.clone()).await;

    place_buildings(&mut editor, &mut rng.derive(), &data, Style::Medieval, vec![&"medieval_spruce".into()], &info).await;
    info = SettlementInfo::new(editor.world());
    build_wall(urban_points, &mut editor, &mut rng.derive(), &mut placer, &material, &data.structures, WallType::Palisade).await;
    editor.flush_buffer().await;
    let _ = generate_chronicle(&mut editor, &mut info).await;
}