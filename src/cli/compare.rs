//! `--compare`: block-level diff of two saved scenes.

use crate::render::{diff, Scene};

/// `--compare <a.scene> <b.scene> [out_dir]`: block-level diff of two saved
/// scenes. Prints a summary table and writes `diff.json` and `diff_top.png`
/// into `out_dir` (default: the directory of A).
pub fn compare_scenes(a_path: &str, b_path: &str, out_dir: Option<&str>) -> anyhow::Result<()> {
    let a_path = std::path::Path::new(a_path);
    let a = Scene::load(a_path)?;
    let b = Scene::load(b_path)?;
    let out_dir = match out_dir {
        Some(d) => std::path::PathBuf::from(d),
        None => a_path.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
    };
    let scene_diff = diff::diff_scenes(&a, &b);
    println!("A = {}\nB = {}", a_path.display(), b_path);
    print!("{}", scene_diff.table(10));
    std::fs::create_dir_all(&out_dir)?;
    let json_path = out_dir.join("diff.json");
    scene_diff.save(&json_path)?;
    let png_path = out_dir.join("diff_top.png");
    diff::render_diff_top(&a, &b, 4)?.save_png(&png_path)?;
    println!("\nWrote {} and {} (red = only in A, green = only in B, yellow = changed)", json_path.display(), png_path.display());
    Ok(())
}
