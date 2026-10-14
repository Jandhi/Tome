//! Runs against the live GDMC server.

use crate::editor::World;
use crate::generator::settlement::{generate_settlement_with, SettlementOptions};
use crate::http_mod::GDMCHTTPProvider;
use crate::noise::Seed;

use crate::outputs::{Artifact, Outputs};

use super::{random_seed, save_and_render_settlement, SettlementSpecs};

/// Generate a settlement on the server, then save the terrain snapshot, scene,
/// metrics and renders under `output/runs/<seed>/` (what `out` asks for).
/// `full` turns on the AI chronicle; `seed_override` pins the seed (default:
/// wall clock); `specs` are house specs to build into the settlement.
pub async fn run_generation_once(full: bool, seed_override: Option<i64>, specs: SettlementSpecs, out: Outputs) {
    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let seed = Seed(seed_override.unwrap_or_else(random_seed));
    let run_dir = std::path::PathBuf::from("output/runs").join(seed.0.to_string());
    // Save the terrain before generating: generation mutates the World's
    // heightmaps, and the snapshot must hold the server's untouched values.
    if out.has(Artifact::Snapshot) {
        let snapshot_path = run_dir.join("terrain.fixture.nbt.gz");
        if let Err(e) = world.to_snapshot().save(&snapshot_path) {
            log::warn!("Failed to save terrain snapshot to {}: {e:#}", snapshot_path.display());
        }
    }
    let mut editor = world.get_editor();
    println!("Generating settlement with seed {}", seed.0);
    // None → auto-select the culture from the build area's climate.
    let options = SettlementOptions { chronicle: full, house_specs: specs };
    let mut report = generate_settlement_with(&mut editor, seed, None, &options).await;

    if let Err(e) = save_and_render_settlement(&editor, seed.0, "live", &run_dir, &mut report, out) {
        log::warn!("Failed to save/render settlement scene in {}: {e:#}", run_dir.display());
    }
}

/// `--snapshot <out_path>`: load the World from the server and save only its
/// terrain snapshot; generates nothing.
pub async fn save_snapshot_only(out_path: &str) -> anyhow::Result<()> {
    let t = std::time::Instant::now();
    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await?;
    println!("World loaded from server in {:.1?}", t.elapsed());
    let t = std::time::Instant::now();
    let snapshot = world.to_snapshot();
    let b = snapshot.build_area;
    let sections: usize = snapshot.chunks.iter().map(|c| c.sections.len()).sum();
    snapshot.save(out_path)?;
    println!(
        "Saved terrain snapshot to {out_path} in {:.1?}: build area origin {} {} {} size {} {} {}, {} chunks, {} sections kept in Y {:?}, {} bytes",
        t.elapsed(), b.origin.x, b.origin.y, b.origin.z, b.size.x, b.size.y, b.size.z,
        snapshot.chunks.len(), sections, snapshot.kept_y_range,
        std::fs::metadata(out_path).map(|m| m.len()).unwrap_or(0)
    );
    let t = std::time::Instant::now();
    let reloaded = World::from_snapshot(crate::editor::terrain_snapshot::TerrainSnapshot::load(out_path)?);
    println!("Reloaded snapshot into a World in {:.1?} ({} chunks)", t.elapsed(), reloaded.chunks.len());
    Ok(())
}
