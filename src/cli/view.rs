//! `--view`: the first-person HTML viewer for a saved scene.

use crate::render::light::Lighting;
use crate::render::Scene;

/// `--view <path.scene> [out.html] [--light <mode>]`: write the interactive
/// first-person viewer page for a saved scene (default: `viewer.html` beside
/// the scene, `viewer-<day|night|dusk|light0.30>.html` when `--light` bakes
/// Minecraft-style lighting into it). Open the file in a browser; no server
/// needed.
pub fn view_saved_scene(scene_path: &str, out: Option<&str>, lighting: Option<Lighting>) -> anyhow::Result<()> {
    let scene_path = std::path::Path::new(scene_path);
    let scene = Scene::load(scene_path)?;
    let out = match (out, lighting) {
        (Some(p), _) => std::path::PathBuf::from(p),
        (None, Some(l)) => scene_path.with_file_name(format!("viewer-{}.html", l.tag())),
        (None, None) => scene_path.with_file_name("viewer.html"),
    };
    write_viewer_with(&scene, &out, lighting)
}

pub fn write_viewer(scene: &Scene, out: &std::path::Path) -> anyhow::Result<()> {
    write_viewer_with(scene, out, None)
}

fn write_viewer_with(scene: &Scene, out: &std::path::Path, lighting: Option<Lighting>) -> anyhow::Result<()> {
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
