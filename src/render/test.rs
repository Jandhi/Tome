#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Instant;

    use crate::geometry::{Point3D, Rect3D};
    use crate::render::colors::block_color;
    use crate::render::elevation::{render_elevation, ElevationSide};
    use crate::render::grid::VoxelGrid;
    use crate::render::iso::{render_iso, IsoCamera};
    use crate::render::topdown::render_topdown;
    use crate::render::{render_all, render_views, RenderOptions, Scene};

    const OX: i32 = 100;
    const OY: i32 = 60;
    const OZ: i32 = 200;
    const N: i32 = 40;

    /// 40×40 test town: gentle hill, a pond, a spruce house with an oak-stairs
    /// roof, door and glass panes, an oak tree and a cobblestone path.
    fn test_scene() -> Scene {
        let mut s = Scene::new(Rect3D { origin: Point3D::new(OX, OY, OZ), size: Point3D::new(N, 30, N) });
        let p = |x: i32, y: i32, z: i32| Point3D::new(OX + x, y, OZ + z);
        let house = |x: i32, z: i32| (18..=26).contains(&x) && (14..=22).contains(&z);
        let pond = |x: i32, z: i32| (4..=12).contains(&x) && (26..=34).contains(&z);

        for x in 0..N {
            for z in 0..N {
                let (dx, dz) = ((x - 32) as f32, (z - 8) as f32);
                let hill = (4.0 * (-(dx * dx + dz * dz) / 80.0).exp()) as i32;
                let ground = if house(x, z) { 64 } else { 64 + hill };
                let floor = if pond(x, z) { 61 } else { ground };
                for y in OY..floor {
                    s.insert(p(x, y, z), if y < floor - 2 { "minecraft:stone" } else { "minecraft:dirt" });
                }
                let top = if pond(x, z) {
                    "minecraft:sand"
                } else if x == 22 && z > 22 {
                    "minecraft:cobblestone"
                } else {
                    "minecraft:grass_block"
                };
                s.insert(p(x, floor, z), top);
                if pond(x, z) {
                    for y in floor + 1..=64 {
                        s.insert(p(x, y, z), "minecraft:water[level=0]");
                    }
                }
            }
        }

        // House: walls y 65..=67, gable roof of oak stairs stepping up to the ridge.
        for x in 18..=26 {
            for z in 14..=22 {
                let edge = x == 18 || x == 26 || z == 14 || z == 22;
                for y in 65..=67 {
                    if edge {
                        s.insert(p(x, y, z), "minecraft:spruce_planks");
                    }
                }
                let step = (z - 14).min(22 - z); // 0 at eaves, 4 at ridge
                let facing = if z < 18 { "south" } else { "north" };
                s.insert(p(x, 68 + step, z), &format!("minecraft:oak_stairs[facing={facing},half=bottom]"));
            }
        }
        s.insert(p(22, 65, 22), "minecraft:oak_door[half=lower,facing=south]");
        s.insert(p(22, 66, 22), "minecraft:oak_door[half=upper,facing=south]");
        for x in [20, 24] {
            s.insert(p(x, 66, 22), "minecraft:glass_pane");
            s.insert(p(x, 66, 14), "minecraft:glass_pane");
        }
        s.insert(p(18, 66, 18), "minecraft:glass_pane");
        s.insert(p(26, 66, 18), "minecraft:glass_pane");

        // Tree on open ground.
        let (tx, tz) = (10, 10);
        for y in 65..=69 {
            s.insert(p(tx, y, tz), "minecraft:oak_log[axis=y]");
        }
        for x in tx - 2..=tx + 2 {
            for z in tz - 2..=tz + 2 {
                for y in 68..=70 {
                    if (x, z) != (tx, tz) || y == 70 {
                        s.insert(p(x, y, z), "minecraft:oak_leaves[persistent=true]");
                    }
                }
            }
        }
        // A cleared cell: air entries must render as empty.
        s.insert(p(0, 64, 0), "minecraft:air");
        s
    }

    fn pixel(pm_path: &Path, x: u32, y: u32) -> [u8; 4] {
        let pm = resvg::tiny_skia::Pixmap::load_png(pm_path).expect("load png");
        let c = pm.pixel(x, y).expect("pixel in range").demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }

    #[test]
    fn render_test_scene_writes_all_views() {
        let scene = test_scene();
        let out = Path::new("output/render_test");
        let paths = render_all(&scene, out).expect("render");

        let expected =
            ["top.png", "iso_se.png", "iso_nw.png", "elev_south.png", "elev_north.png", "elev_east.png", "elev_west.png"];
        assert_eq!(paths.len(), expected.len());
        for name in expected.iter().chain(["index.txt"].iter()) {
            let f = out.join(name);
            let len = std::fs::metadata(&f).map(|m| m.len()).unwrap_or(0);
            assert!(len > 0, "{} missing or empty", f.display());
        }
        let index = std::fs::read_to_string(out.join("index.txt")).unwrap();
        assert!(expected.iter().all(|n| index.contains(n)), "index.txt lists every image:\n{index}");

        let px = RenderOptions::default().px;
        let top = out.join("top.png");
        let pm = resvg::tiny_skia::Pixmap::load_png(&top).unwrap();
        assert_eq!((pm.width(), pm.height()), ((N as u32 + 2) * px, (N as u32 + 2) * px));

        // Centre of block (bx, bz) in the top-down image.
        let at = |bx: u32, bz: u32| ((bx + 1) * px + px / 2, (bz + 1) * px + px / 2);

        // Roof slope (eave row, flat relative to its west neighbour) ≈ oak stairs colour.
        let (rx, ry) = at(22, 15);
        let roof = pixel(&top, rx, ry);
        let stairs = block_color("minecraft:oak_stairs");
        for k in 0..3 {
            let ratio = roof[k] as f32 / stairs[k] as f32;
            assert!((0.75..=1.25).contains(&ratio), "roof pixel {roof:?} vs oak stairs {stairs:?}");
        }

        // Pond is bluish.
        let (wx, wy) = at(8, 30);
        let water = pixel(&top, wx, wy);
        assert!(water[2] as i32 > water[0] as i32 + 30 && water[2] > water[1], "pond pixel {water:?}");
    }

    #[test]
    fn render_region_crops_to_house() {
        let scene = test_scene();
        let opts = RenderOptions {
            region: Some(Rect3D { origin: Point3D::new(OX + 17, OY, OZ + 13), size: Point3D::new(11, 20, 11) }),
            ..Default::default()
        };
        let out = Path::new("output/render_test");
        let paths = render_views(&scene, &opts, out, "house_").expect("render");
        assert_eq!(paths.len(), 7);
        let pm = resvg::tiny_skia::Pixmap::load_png(out.join("house_top.png")).unwrap();
        assert_eq!((pm.width(), pm.height()), (13 * 4, 13 * 4));
        assert!(out.join("house_index.txt").exists());
    }

    #[test]
    fn empty_scene_renders_nothing() {
        let scene = Scene::new(Rect3D { origin: Point3D::new(0, 0, 0), size: Point3D::new(10, 10, 10) });
        let out = std::env::temp_dir().join("tome_render_empty");
        assert!(render_all(&scene, &out).unwrap().is_empty());
        let zero = Scene::new(Rect3D { origin: Point3D::new(0, 0, 0), size: Point3D::new(0, 0, 0) });
        assert!(render_all(&zero, &out).unwrap().is_empty());
    }

    /// ~300k blocks over 200×200×40; prints per-view timings.
    #[test]
    fn render_timing_large_scene() {
        let mut s = Scene::new(Rect3D { origin: Point3D::new(0, 50, 0), size: Point3D::new(200, 40, 200) });
        for x in 0..200 {
            for z in 0..200 {
                let h = 60 + ((x as f32 / 17.0).sin() * 3.0 + (z as f32 / 23.0).cos() * 3.0) as i32;
                for y in h - 6..=h {
                    s.insert(Point3D::new(x, y, z), if y == h { "minecraft:grass_block" } else { "minecraft:dirt" });
                }
                // Scattered 6×6×6 hollow houses.
                if x % 20 < 6 && z % 20 < 6 {
                    let edge = x % 20 == 0 || x % 20 == 5 || z % 20 == 0 || z % 20 == 5;
                    for y in 70..76 {
                        if edge || y == 75 {
                            s.insert(Point3D::new(x, y, z), "minecraft:stone_bricks");
                        }
                    }
                }
            }
        }
        println!("scene blocks: {}", s.blocks.len());

        let t = Instant::now();
        let g = VoxelGrid::from_scene(&s, None).unwrap();
        println!("grid build:   {:?}", t.elapsed());
        let t = Instant::now();
        render_topdown(&g, 4).unwrap();
        println!("top-down:     {:?}", t.elapsed());
        for cam in [IsoCamera::SouthEast, IsoCamera::NorthWest, IsoCamera::NorthEast, IsoCamera::SouthWest] {
            let t = Instant::now();
            render_iso(&g, cam, 8).unwrap();
            println!("iso {}:       {:?}", cam.tag(), t.elapsed());
        }
        for side in ElevationSide::ALL {
            let t = Instant::now();
            render_elevation(&g, side, 4).unwrap();
            println!("elev {:<6}:  {:?}", side.name(), t.elapsed());
        }
        let t = Instant::now();
        let out = std::env::temp_dir().join("tome_render_timing");
        render_all(&s, &out).unwrap();
        println!("render_all incl. PNG encode: {:?}", t.elapsed());
    }
}
