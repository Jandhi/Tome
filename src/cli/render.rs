//! `--render`: re-render a saved scene.

use std::time::Duration;

use crate::render::grid::VoxelGrid;
use crate::render::light::{LightMap, Lighting};
use crate::render::{render_grid_views, RenderOptions, Scene};

use super::view::write_viewer;

/// `--render <path.scene> [out_dir] [--light <mode>]`: re-render a saved
/// scene, no server needed. `--light` reports the scene's light map (timing
/// and levels) and also writes every view lit under that mode into
/// `<out_dir>/lit-<mode>/`, beside the unlit ones.
pub fn render_saved_scene(scene_path: &str, out_dir: Option<&str>, lighting: Option<Lighting>) -> anyhow::Result<()> {
    let scene_path = std::path::Path::new(scene_path);
    let scene = Scene::load(scene_path)?;
    let out_dir = match out_dir {
        Some(d) => std::path::PathBuf::from(d),
        None => scene_path.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
    };
    // One grid (and, with --light, one light map) serves the report and
    // both sets of views.
    let t = std::time::Instant::now();
    let grid = VoxelGrid::from_scene(&scene, None);
    let grid_time = t.elapsed();
    match &grid {
        None => println!("Nothing visible in the scene to render"),
        Some(g) => {
            let defaults = RenderOptions::default();
            for path in render_grid_views(&scene, g, None, &defaults, &out_dir, "")? {
                println!("Rendered {}", path.display());
            }
            if let Some(lighting) = lighting {
                let t = std::time::Instant::now();
                let map = LightMap::new(g);
                report_light_map(g, &map, lighting, grid_time, t.elapsed());
                let lit_dir = out_dir.join(format!("lit-{}", lighting.tag()));
                let opts = RenderOptions { lighting: Some(lighting), ..defaults };
                for path in render_grid_views(&scene, g, Some(&map), &opts, &lit_dir, "")? {
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
fn report_light_map(g: &VoxelGrid, map: &LightMap, lighting: Lighting, grid_time: Duration, map_time: Duration) {
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
