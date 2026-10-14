//! Command-line modes behind `main`'s flags: live and offline generation, scene
//! rendering, the HTML viewer, scene diffs and the house showcase.

pub mod compare;
pub mod houses;
pub mod live;
pub mod offline;
pub mod render;
pub mod view;

use crate::editor::Editor;
use crate::generator::buildings_v2::spec::HouseSpec;
use crate::generator::report::SettlementReport;
use crate::outputs::{Artifact, Outputs};

/// A fresh seed for an interactive run, taken from the wall clock. Generation is
/// fully deterministic in the seed, so the printed value reproduces the exact
/// settlement (pass it to `generate_settlement` to rebuild it).
pub fn random_seed() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(12345)
}

/// House specs a settlement run builds, `(file name, spec)`.
pub type SettlementSpecs = Vec<(String, HouseSpec)>;

/// Every `--spec <file>` of a settlement run (repeatable), read and parsed up
/// front so a typo fails before anything is generated. The settlement checks
/// the rest once it knows its culture, and seats each on the first road slot
/// it fits.
pub fn settlement_specs(args: &[String]) -> anyhow::Result<SettlementSpecs> {
    args.windows(2)
        .filter(|w| w[0] == "--spec")
        .map(|w| {
            let path = std::path::Path::new(&w[1]);
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| w[1].clone());
            Ok((name, HouseSpec::load(path)?))
        })
        .collect()
}

/// Write the run's artifacts that `out` asks for into `dir`: `metrics.json`
/// (the [`SettlementReport`]), every house's spec
/// (`houses/house_<n>.spec.yaml`), the captured `settlement.scene`, its
/// rendered views and `viewer.html`. The scene is only captured when something
/// needs it.
pub fn save_and_render_settlement(
    editor: &Editor,
    seed: i64,
    source: &str,
    dir: &std::path::Path,
    report: &mut SettlementReport,
    out: Outputs,
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
            let scene_path = dir.join("settlement.scene");
            scene.save(&scene_path)?;
            println!("Saved scene ({} blocks) to {}", scene.blocks.len(), scene_path.display());
        }
        if out.has(Artifact::Renders) {
            for path in crate::render::render_all(&scene, dir)? {
                println!("Rendered {}", path.display());
            }
        }
        if out.has(Artifact::Viewer) {
            view::write_viewer(&scene, &dir.join("viewer.html"))?;
        }
    }
    println!("Run output in {} ({})", dir.display(), out.names());
    Ok(())
}
