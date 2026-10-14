//! `--offline`: generation from a saved terrain snapshot, no server.

use crate::editor::{terrain_snapshot::TerrainSnapshot, World};
use crate::generator::settlement::{generate_settlement_with, SettlementOptions};
use crate::noise::Seed;

use super::{random_seed, save_and_render_settlement};

/// `--offline <snapshot> [seed]`: build a settlement from a saved terrain snapshot
/// with no server at all, then save and render it exactly like a live run, under
/// `output/runs/offline-<seed>/`.
pub async fn run_offline(snapshot_path: &str, seed_arg: Option<&str>) -> anyhow::Result<()> {
    let t0 = std::time::Instant::now();
    let world = World::from_snapshot(TerrainSnapshot::load(snapshot_path)?);
    println!("Loaded snapshot {snapshot_path} in {:.1?}", t0.elapsed());
    let seed = match seed_arg {
        Some(s) => Seed(s.parse()?),
        None => Seed(random_seed()),
    };
    let mut editor = world.get_offline_editor();
    println!("Generating settlement offline with seed {}", seed.0);
    let t1 = std::time::Instant::now();
    let mut report = generate_settlement_with(&mut editor, seed, None, &SettlementOptions::default()).await;
    println!("Generated in {:.1?}", t1.elapsed());
    let dir = std::path::PathBuf::from("output/runs").join(format!("offline-{}", seed.0));
    let t2 = std::time::Instant::now();
    save_and_render_settlement(&editor, seed.0, "offline", &dir, &mut report)?;
    println!("Saved and rendered in {:.1?}; total {:.1?}", t2.elapsed(), t0.elapsed());
    Ok(())
}
