//! Command-line modes behind `main`'s flags: live and offline generation, scene
//! rendering, the HTML viewer, scene diffs and the house showcase.

pub mod compare;
pub mod houses;
pub mod live;
pub mod offline;
pub mod render;
pub mod view;

use crate::editor::Editor;
use crate::generator::report::SettlementReport;

/// A fresh seed for an interactive run, taken from the wall clock. Generation is
/// fully deterministic in the seed, so the printed value reproduces the exact
/// settlement (pass it to `generate_settlement` to rebuild it).
pub fn random_seed() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(12345)
}

/// Capture the generated settlement as `settlement.scene` in `dir`, write the run's
/// `metrics.json` (the [`SettlementReport`]) beside it, and render its views next
/// to them.
pub fn save_and_render_settlement(
    editor: &Editor,
    seed: i64,
    source: &str,
    dir: &std::path::Path,
    report: &mut SettlementReport,
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
    let scene_path = dir.join("settlement.scene");
    scene.save(&scene_path)?;
    println!("Saved scene ({} blocks) to {}", scene.blocks.len(), scene_path.display());
    for path in crate::render::render_all(&scene, dir)? {
        println!("Rendered {}", path.display());
    }
    println!("Run output in {}", dir.display());
    Ok(())
}
