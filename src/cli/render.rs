//! `--render`: re-render a saved scene.

use std::time::Duration;

use crate::geometry::{Point3D, Rect2D, Rect3D};
use crate::render::grid::VoxelGrid;
use crate::render::light::{LightMap, Lighting};
use crate::render::{render_grid_views, RenderOptions, Scene};

use super::view::write_viewer;

/// `--render <path.scene> [out_dir] [--light <mode>] [--region x,z,w,h] [--px N]`:
/// re-render a saved scene, no server needed. `--light` reports the scene's
/// light map (timing and levels) and also writes every view lit under that
/// mode into `<out_dir>/lit-<mode>/`, beside the unlit ones. `--region` keeps
/// only that box (absolute x, z, width along x, depth along z; every height)
/// for every view and the viewer, into `region_<x>_<z>_<w>x<h>/` beside the
/// scene unless `out_dir` is given. `--px` sets pixels per block (default 4,
/// the isometric tile twice that).
pub fn render_saved_scene(
    scene_path: &str,
    out_dir: Option<&str>,
    lighting: Option<Lighting>,
    region: Option<Rect2D>,
    px: Option<u32>,
) -> anyhow::Result<()> {
    let scene_path = std::path::Path::new(scene_path);
    let mut scene = Scene::load(scene_path)?;
    if let Some(r) = region {
        scene = crop(&scene, r);
        if scene.blocks.is_empty() {
            anyhow::bail!("--region {},{},{},{} holds no blocks of the scene", r.min().x, r.min().y, r.size.x, r.size.y);
        }
    }
    let out_dir = match (out_dir, region) {
        (Some(d), _) => std::path::PathBuf::from(d),
        (None, r) => {
            let dir = scene_path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            match r {
                Some(r) => dir.join(format!("region_{}_{}_{}x{}", r.min().x, r.min().y, r.size.x, r.size.y)),
                None => dir,
            }
        }
    };
    // One grid (and, with --light, one light map) serves the report and
    // both sets of views.
    let t = std::time::Instant::now();
    let grid = VoxelGrid::from_scene(&scene, None);
    let grid_time = t.elapsed();
    match &grid {
        None => println!("Nothing visible in the scene to render"),
        Some(g) => {
            let defaults = match px {
                Some(px) => RenderOptions { px, iso_tile: 2 * px, ..RenderOptions::default() },
                None => RenderOptions::default(),
            };
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

/// `--region x,z,w,h`: absolute x and z of a corner, width along x and depth
/// along z, both at least 1.
pub fn parse_region(arg: &str) -> anyhow::Result<Rect2D> {
    let nums: Vec<i32> = arg
        .split(',')
        .map(|n| n.trim().parse::<i32>().map_err(|e| anyhow::anyhow!("--region {arg:?}: bad number {n:?}: {e}")))
        .collect::<Result<_, _>>()?;
    let [x, z, w, h] = nums[..] else { anyhow::bail!("--region takes x,z,w,h (corner, then width and depth), not {arg:?}") };
    if w < 1 || h < 1 {
        anyhow::bail!("--region {arg:?}: width and depth must be at least 1");
    }
    Ok(Rect2D::new(crate::geometry::Point2D::new(x, z), crate::geometry::Point2D::new(w, h)))
}

/// The blocks of `scene` inside `region` (absolute x/z), at every height.
fn crop(scene: &Scene, region: Rect2D) -> Scene {
    let b = scene.bounds;
    let bounds = Rect3D::from_points(
        Point3D::new(region.min().x, b.min().y, region.min().y),
        Point3D::new(region.max().x, b.max().y, region.max().y),
    );
    let mut out = Scene::new(bounds);
    out.meta = scene.meta.clone();
    for (p, &i) in &scene.blocks {
        if region.contains(crate::geometry::Point2D::new(p.x, p.z)) {
            out.insert(*p, &scene.palette[i as usize]);
        }
    }
    out
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
