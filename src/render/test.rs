#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Instant;

    use crate::geometry::{Point3D, Rect3D};
    use crate::render::colors::block_color;
    use crate::render::elevation::{render_elevation, ElevationSide};
    use crate::render::grid::{VoxelGrid, BACKGROUND};
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

    /// 12×12 scene: a 3×3 plank house (y 0..=1) with a bottom-slab roof at
    /// y 2, and an east-west oak fence line at z 2 ending in a stone block.
    fn shapes_scene() -> Scene {
        let mut s = Scene::new(Rect3D { origin: Point3D::new(0, 0, 0), size: Point3D::new(12, 8, 12) });
        for x in 6..=8 {
            for z in 6..=8 {
                if x != 7 || z != 7 {
                    for y in 0..=1 {
                        s.insert(Point3D::new(x, y, z), "minecraft:oak_planks");
                    }
                }
                s.insert(Point3D::new(x, 2, z), "minecraft:oak_slab[type=bottom,waterlogged=false]");
            }
        }
        for x in 0..=3 {
            let (w, e) = (x > 0, true);
            s.insert(
                Point3D::new(x, 0, 2),
                &format!("minecraft:oak_fence[east={e},north=false,south=false,waterlogged=false,west={w}]"),
            );
        }
        s.insert(Point3D::new(4, 0, 2), "minecraft:stone");
        s
    }

    fn rgba(pm: &resvg::tiny_skia::Pixmap, x: u32, y: u32) -> [u8; 4] {
        let c = pm.pixel(x, y).expect("pixel in range").demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }

    #[test]
    fn bottom_slab_iso_leaves_upper_half_open() {
        let g = VoxelGrid::from_scene(&shapes_scene(), None).unwrap();
        assert_eq!(g.ny, 3);
        let tw = 16;
        let pm = render_iso(&g, IsoCamera::SouthEast, tw as u32).unwrap();
        // Anchor of the front eave slab (8, 2, 8): SE camera, u = x, v = z.
        let (th, bh, margin) = (tw / 2, tw / 2, tw);
        let (u, y, v) = (8, 2, 8);
        let ax = (u - v) * tw / 2 + g.nz * tw / 2 + margin;
        let ay = (u + v) * th / 2 - y * bh + (g.ny - 1) * bh + margin;
        // Sample the +u (east) side a quarter tile right of the front edge: a
        // full cube would show its shaded side in both the upper and lower half.
        let upper = rgba(&pm, (ax + tw / 4) as u32, (ay + 3 * th / 4 + bh / 4) as u32);
        let lower = rgba(&pm, (ax + tw / 4) as u32, (ay + 3 * th / 4 + 3 * bh / 4) as u32);
        let slab = block_color("minecraft:oak_slab");
        let side = crate::render::grid::shade(slab, 0.8);
        assert_eq!(lower, side, "lower half is the slab's east side");
        assert_ne!(upper, side, "upper half must not be drawn as a side face");
        // What shows there is what lies behind: the slab's own (unshaded) top.
        assert_eq!(upper, slab, "upper half shows the slab top behind it");
    }

    #[test]
    fn fence_column_covers_less_than_a_block_in_elevation() {
        let g = VoxelGrid::from_scene(&shapes_scene(), None).unwrap();
        let px = 16u32;
        let pm = render_elevation(&g, ElevationSide::South, px).unwrap();
        // Seen from the south: block column x sits at pixel column (x + 1) * px,
        // block row y at ((ny - 1 - y) + 1) * px.
        let filled = |bx: i32, by: i32| {
            let (x0, y0) = ((bx as u32 + 1) * px, ((g.ny - 1 - by) as u32 + 1) * px);
            let mut n = 0;
            for y in y0..y0 + px {
                for x in x0..x0 + px {
                    if rgba(&pm, x, y) != crate::render::grid::BACKGROUND {
                        n += 1;
                    }
                }
            }
            n
        };
        let (fence, stone) = (filled(1, 0), filled(4, 0));
        assert_eq!(stone, px * px, "a full block fills its square");
        assert!(fence > 0, "the fence is visible");
        assert!(fence < stone * 3 / 4, "fence {fence} px vs block {stone} px");
        // The slab roof covers only the lower half of its row.
        let roof = filled(7, 2);
        assert_eq!(roof, px * px / 2, "bottom slab fills half its square");
    }

    /// Render a crop of a saved scene for manual inspection:
    /// `TOME_SCENE=<path.scene> TOME_REGION="x y z sx sy sz" TOME_RENDER_OUT=<dir> cargo test render_saved_region -- --ignored`
    #[test]
    #[ignore]
    fn render_saved_region() {
        let (Ok(path), Ok(region), Ok(out)) =
            (std::env::var("TOME_SCENE"), std::env::var("TOME_REGION"), std::env::var("TOME_RENDER_OUT"))
        else {
            println!("set TOME_SCENE, TOME_REGION and TOME_RENDER_OUT");
            return;
        };
        let n: Vec<i32> = region.split_whitespace().map(|v| v.parse().expect("region int")).collect();
        assert_eq!(n.len(), 6, "TOME_REGION needs 6 ints");
        let scene = Scene::load(&path).expect("load scene");
        let opts = RenderOptions {
            px: 16,
            iso_tile: 24,
            region: Some(Rect3D { origin: Point3D::new(n[0], n[1], n[2]), size: Point3D::new(n[3], n[4], n[5]) }),
        };
        for p in render_views(&scene, &opts, Path::new(&out), "crop_").expect("render") {
            println!("Rendered {}", p.display());
        }
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

    /// A 9x9 stone floor at y=0 with a 3-high spruce wall along z=8.
    fn room_scene() -> Scene {
        let mut s = Scene::new(Rect3D { origin: Point3D::new(0, 0, 0), size: Point3D::new(9, 6, 9) });
        for x in 0..9 {
            for z in 0..9 {
                s.insert(Point3D::new(x, 0, z), "minecraft:stone");
            }
            for y in 1..=3 {
                s.insert(Point3D::new(x, y, 8), "minecraft:spruce_planks");
            }
        }
        s
    }

    fn rgb(pm: &resvg::tiny_skia::Pixmap, x: u32, y: u32) -> [u8; 3] {
        let p = pm.pixel(x, y).unwrap();
        [p.red(), p.green(), p.blue()]
    }

    #[test]
    fn perspective_sees_wall_floor_and_sky() {
        use crate::render::perspective::{render_perspective, PerspectiveCamera};
        let s = room_scene();
        let g = VoxelGrid::from_scene(&s, None).unwrap();
        // Standing in cell (4, 1, 1), eyes at 2.62, looking level at the wall.
        let cam = PerspectiveCamera { eye: [4.5, 2.62, 1.5], target: [4.5, 2.62, 8.0], fov_deg: 90.0 };
        let pm = render_perspective(&g, &cam, 64, 64).unwrap();

        // Centre: the wall's north face, darker than the raw colour and fogged.
        let wall = block_color("minecraft:spruce_planks");
        let c = rgb(&pm, 32, 32);
        assert!(c[0] < wall[0] && c[1] < wall[1] && c[2] < wall[2], "wall pixel {c:?} vs {wall:?}");
        assert!(c[0] > wall[0] / 2, "wall pixel {c:?} is too dark for a side face");
        // Top row: over the wall, sky.
        assert_eq!(rgb(&pm, 32, 0), [BACKGROUND[0], BACKGROUND[1], BACKGROUND[2]]);
        // Bottom row: the stone floor's top face, nearer and brighter than the wall.
        let stone = block_color("minecraft:stone");
        let f = rgb(&pm, 32, 63);
        assert!(f[0] > c[0], "floor {f:?} should be brighter than the far wall {c:?}");
        assert!(f[0] <= stone[0] && f[0] > stone[0] * 3 / 4, "floor pixel {f:?} vs {stone:?}");
    }

    #[test]
    fn perspective_from_inside_a_block_is_uniform() {
        use crate::render::perspective::{render_perspective, PerspectiveCamera};
        let s = room_scene();
        let g = VoxelGrid::from_scene(&s, None).unwrap();
        let cam = PerspectiveCamera { eye: [4.5, 0.5, 8.5], target: [4.5, 0.5, 0.0], fov_deg: 90.0 };
        let pm = render_perspective(&g, &cam, 16, 16).unwrap();
        let first = rgb(&pm, 0, 0);
        assert!((0..16).all(|x| (0..16).all(|y| rgb(&pm, x, y) == first)));
        assert_ne!(first, [BACKGROUND[0], BACKGROUND[1], BACKGROUND[2]]);
    }

    #[test]
    fn contact_sheet_tiles_in_rows() {
        use crate::render::perspective::contact_sheet;
        let tiles: Vec<_> = (0..5).map(|_| resvg::tiny_skia::Pixmap::new(10, 6).unwrap()).collect();
        let sheet = contact_sheet(&tiles, 3, 2).unwrap();
        assert_eq!((sheet.width(), sheet.height()), (3 * 10 + 4 * 2, 2 * 6 + 3 * 2));
    }
}
