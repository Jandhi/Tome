//! Furniture previews: one piece at a time in a small walled corner, rendered
//! from two three-quarter angles, so a new piece can be judged on its own
//! before it is let loose in rooms. `--furniture <name,...>` writes
//! `output/furniture/<name>.png` per piece and a `sheet.png` of them all.
//!
//! The piece is placed exactly as the furnisher would against a north wall
//! (offsets and facings unrotated), or freestanding two cells out from it
//! when it has no wall constraint. Palette swaps are left as authored.

use std::path::{Path, PathBuf};

use resvg::tiny_skia::Pixmap;

use crate::editor::block_string;
use crate::geometry::{Cardinal, Point3D, Rect3D};
use crate::render::grid::VoxelGrid;
use crate::render::light::{LightMap, Lighting};
use crate::render::perspective::{contact_sheet, render_perspective_with, PerspectiveCamera, PerspectiveOptions};
use crate::render::Scene;

use super::block::{apply_facing, parse_block, resolve_facing, rotate_block};
use super::data::{Furniture, FurnitureData};
use super::placement::needs_wall;

/// Minimum interior size of the preview corner (x along the back wall, z away
/// from it); larger pieces grow the room to fit with `MARGIN` cells to spare.
const MIN_ROOM_W: i32 = 9;
const MIN_ROOM_D: i32 = 7;
/// Minimum height of the back and side walls above the floor.
const MIN_WALL_H: i32 = 4;
const MARGIN: i32 = 2;
const VIEW_W: u32 = 640;
const VIEW_H: u32 = 480;

/// The preview scene for one piece, and the centre of the piece's blocks
/// (absolute coordinates) for the cameras to aim at.
pub fn preview_scene(item: &Furniture) -> (Scene, [f32; 3]) {
    let extent = |axis: usize| {
        let values = item.blocks.iter().map(|b| b.offset[axis]);
        (values.clone().min().unwrap_or(0), values.max().unwrap_or(0))
    };
    let ((a0, a1), (h0, h1), (w0, w1)) = (extent(0), extent(1), extent(2));
    // Against the back wall, or two cells out.
    let gap = if needs_wall(item) { 0 } else { 2 };
    let room_w = MIN_ROOM_W.max(a1 - a0 + 1 + 2 * MARGIN);
    let room_d = MIN_ROOM_D.max(gap + w1 - w0 + 1 + MARGIN);
    let wall_h = MIN_WALL_H.max(h1 - h0 + 2);

    let mut scene = Scene::new(Rect3D {
        origin: Point3D::new(-1, 0, -1),
        size: Point3D::new(room_w + 1, wall_h + 2, room_d + 1),
    });
    for x in -1..room_w {
        for z in -1..room_d {
            scene.insert(Point3D::new(x, 0, z), "minecraft:spruce_planks");
        }
    }
    for y in 1..=wall_h {
        for x in -1..room_w {
            scene.insert(Point3D::new(x, y, -1), "minecraft:stone_bricks");
        }
        for z in 0..room_d {
            scene.insert(Point3D::new(-1, y, z), "minecraft:stone_bricks");
        }
    }

    // Centre the piece along the wall.
    let ox = (room_w - (a1 - a0 + 1)) / 2 - a0;
    let oz = gap - w0;

    let dir = Cardinal::North;
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for pb in item.blocks.iter().filter(|b| b.place) {
        let facing = item.constraints.iter()
            .find(|c| c.offset == [pb.offset[0], pb.offset[2]])
            .and_then(|c| resolve_facing(c.facing, dir));
        let block = apply_facing(&rotate_block(&parse_block(&pb.block), dir), facing);
        let p = Point3D::new(ox + pb.offset[0], 1 + pb.offset[1], oz + pb.offset[2]);
        scene.insert(p, &block_string(&block));
        for (k, v) in [p.x, p.y, p.z].into_iter().enumerate() {
            lo[k] = lo[k].min(v as f32);
            hi[k] = hi[k].max(v as f32 + 1.0);
        }
    }
    let centre = if lo[0] > hi[0] {
        [room_w as f32 / 2.0, 1.0, 1.0]
    } else {
        [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0]
    };
    (scene, centre)
}

/// Two lit three-quarter views of `item`, side by side.
pub fn render_preview(item: &Furniture) -> anyhow::Result<Pixmap> {
    let (scene, c) = preview_scene(item);
    // Back the cameras off for pieces bigger than the default room.
    let bounds = scene.bounds.size;
    let scale = (bounds.x as f32 / (MIN_ROOM_W + 1) as f32).max(bounds.y as f32 / (MIN_WALL_H + 2) as f32).max(1.0);
    let g = VoxelGrid::from_scene(&scene, None).ok_or_else(|| anyhow::anyhow!("empty preview scene"))?;
    let map = LightMap::new(&g);
    let opts = PerspectiveOptions { lighting: Some((&map, Lighting::parse("day")?)), fog: false };
    let views = [[4.5, 3.6, 6.5], [-3.2, 3.6, 6.5]]
        .into_iter()
        .map(|d| {
            let cam = PerspectiveCamera { eye: [c[0] + d[0] * scale, c[1] + d[1] * scale, c[2] + d[2] * scale], target: c, fov_deg: 50.0 };
            render_perspective_with(&g, &cam, VIEW_W, VIEW_H, &opts)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    contact_sheet(&views, 2, 4)
}

/// Render each named piece into `out_dir/<name>.png` plus `out_dir/sheet.png`
/// (two pieces per row, in the order given). Returns the paths written.
pub fn render_previews(data: &FurnitureData, names: &[String], out_dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(out_dir)?;
    let mut paths = Vec::new();
    let mut tiles = Vec::new();
    for name in names {
        let item = data.items.get(name).ok_or_else(|| anyhow::anyhow!("no furniture item named '{name}'"))?;
        let pm = render_preview(item)?;
        let path = out_dir.join(format!("{name}.png"));
        pm.save_png(&path)?;
        paths.push(path);
        tiles.push(pm);
    }
    if tiles.len() > 1 {
        let path = out_dir.join("sheet.png");
        contact_sheet(&tiles, 2, 12)?.save_png(&path)?;
        paths.push(path);
    }
    Ok(paths)
}
