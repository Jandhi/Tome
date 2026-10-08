//! Software voxel renderer: turns a [`Scene`] (terrain + placed blocks) into
//! PNG images that can be inspected without opening Minecraft.
//!
//! Views: top-down (height-shaded), isometric, and the four side elevations.
//! Output goes under an `out_dir`; see [`render_all`] and [`render_views`].
//! Each call also writes an `index.txt` describing every image.

pub mod colors;
pub mod elevation;
pub mod grid;
pub mod iso;
pub mod scene;
pub mod shape;
pub mod topdown;

#[cfg(test)]
mod test;

pub use scene::Scene;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use resvg::tiny_skia::Pixmap;

use crate::geometry::Rect3D;
use elevation::ElevationSide;
use grid::VoxelGrid;
use iso::IsoCamera;

#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    /// Pixels per block for the top-down and elevation views.
    pub px: u32,
    /// Isometric tile width in pixels (snapped to a multiple of 4).
    pub iso_tile: u32,
    /// Absolute-coordinate sub-box to render (e.g. one building). `None` = whole scene.
    pub region: Option<Rect3D>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions { px: 4, iso_tile: 8, region: None }
    }
}

/// Render every standard view of `scene` into `out_dir` and return the paths
/// written. Never panics on an empty scene; returns `Ok(vec![])` instead.
pub fn render_all(scene: &Scene, out_dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    render_views(scene, &RenderOptions::default(), out_dir, "")
}

/// Render `<prefix>top.png`, `<prefix>iso_se.png`, `<prefix>iso_nw.png` and
/// `<prefix>elev_{south,north,east,west}.png` plus `<prefix>index.txt` into
/// `out_dir`. Returns the PNG paths (empty when nothing visible is in range).
pub fn render_views(scene: &Scene, opts: &RenderOptions, out_dir: &Path, prefix: &str) -> anyhow::Result<Vec<PathBuf>> {
    let Some(g) = VoxelGrid::from_scene(scene, opts.region) else {
        log::info!("render: nothing visible to render into {}", out_dir.display());
        return Ok(Vec::new());
    };
    log_unknown_blocks(scene);
    std::fs::create_dir_all(out_dir)?;

    let lo = g.origin;
    let hi = (lo.x + g.nx - 1, lo.y + g.ny - 1, lo.z + g.nz - 1);
    let mut index = format!(
        "Renders of x {}..{}, y {}..{}, z {}..{} (absolute world coords, {}x{}x{} blocks).\n\
         Coordinates: +X east, +Y up, +Z south.\n\n",
        lo.x, hi.0, lo.y, hi.1, lo.z, hi.2, g.nx, g.ny, g.nz
    );
    let mut paths = Vec::new();
    let mut save = |name: String, desc: String, pm: Pixmap| -> anyhow::Result<()> {
        let path = out_dir.join(&name);
        pm.save_png(&path)?;
        writeln!(index, "{name} ({}x{} px): {desc}", pm.width(), pm.height())?;
        paths.push(path);
        Ok(())
    };

    save(
        format!("{prefix}top.png"),
        format!(
            "top-down orthographic, north up, east right, {} px/block, 1-block margin, hill-shaded (light from NW), water bluer with depth",
            opts.px.max(1)
        ),
        topdown::render_topdown(&g, opts.px)?,
    )?;
    for (cam, desc) in [
        (IsoCamera::SouthEast, "camera at the south-east corner; south and east facades visible, north-west corner at the top"),
        (IsoCamera::NorthWest, "camera at the north-west corner; north and west facades visible, south-east corner at the top"),
    ] {
        save(
            format!("{prefix}iso_{}.png", cam.tag()),
            format!("2:1 isometric, {desc}, tile width {} px", iso::tile_width(opts.iso_tile)),
            iso::render_iso(&g, cam, opts.iso_tile)?,
        )?;
    }
    for side in ElevationSide::ALL {
        save(
            format!("{prefix}elev_{}.png", side.name()),
            format!(
                "orthographic elevation {}, {} px/block, nearer blocks brighter (depth fog)",
                side.describe(),
                opts.px.max(1)
            ),
            elevation::render_elevation(&g, side, opts.px)?,
        )?;
    }

    std::fs::write(out_dir.join(format!("{prefix}index.txt")), index)?;
    Ok(paths)
}

/// Warn once per render about block ids this scene uses that fell through to
/// the hash-colour fallback.
fn log_unknown_blocks(scene: &Scene) {
    let unknown = colors::unknown_ids();
    let mut used: Vec<&str> = scene
        .palette
        .iter()
        .map(|s| Scene::strip_state(s))
        .map(|id| id.strip_prefix("minecraft:").unwrap_or(id))
        .filter(|id| unknown.iter().any(|u| u == id))
        .collect();
    used.sort_unstable();
    used.dedup();
    if !used.is_empty() {
        log::warn!("render: {} block id(s) have no colour, using fallback: {}", used.len(), used.join(", "));
    }
}
