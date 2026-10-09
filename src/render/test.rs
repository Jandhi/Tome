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
        // The same house with full plank blocks in place of the slab roof.
        let mut full = shapes_scene();
        for x in 6..=8 {
            for z in 6..=8 {
                full.insert(Point3D::new(x, 2, z), "minecraft:oak_planks");
            }
        }
        let gf = VoxelGrid::from_scene(&full, None).unwrap();
        let pf = render_iso(&gf, IsoCamera::SouthEast, tw as u32).unwrap();
        // Anchor of the front eave slab (8, 2, 8): SE camera, u = x, v = z.
        let (th, bh, margin) = (tw / 2, tw / 2, tw);
        let (u, y, v) = (8, 2, 8);
        let ax = (u - v) * tw / 2 + g.nz * tw / 2 + margin;
        let ay = (u + v) * th / 2 - y * bh + (g.ny - 1) * bh + margin;
        // Sample the +u (east) side a quarter tile right of the front edge: a
        // full cube would show its shaded side in both the upper and lower half.
        let (sx, upper_y, lower_y) = ((ax + tw / 4) as u32, (ay + 3 * th / 4 + bh / 4) as u32, (ay + 3 * th / 4 + 3 * bh / 4) as u32);
        let upper = rgba(&pm, sx, upper_y);
        let lower = rgba(&pm, sx, lower_y);
        // The lower half is the slab's east side: the same pixel (flat colour
        // or texel) as the full block's side there.
        assert_eq!(lower, rgba(&pf, sx, lower_y), "lower half is the slab's east side");
        // The upper half shows what lies behind: the slab's own top, which
        // sits half a block lower on screen than the full block's top.
        assert_eq!(upper, rgba(&pf, sx, upper_y - (bh / 2) as u32), "upper half shows the slab top behind it");
        if crate::render::assets::assets().is_none() {
            let slab = block_color("minecraft:oak_slab");
            let side = crate::render::grid::shade(slab, 0.8);
            assert_eq!(lower, side, "lower half is the slab's east side");
            assert_ne!(upper, side, "upper half must not be drawn as a side face");
            assert_eq!(upper, slab, "upper half shows the slab top behind it");
        }
    }

    /// One block at the origin of a 1x1x1 scene.
    fn lone_block(block: &str) -> VoxelGrid {
        let mut s = Scene::new(Rect3D { origin: Point3D::new(0, 0, 0), size: Point3D::new(1, 1, 1) });
        s.insert(Point3D::new(0, 0, 0), block);
        VoxelGrid::from_scene(&s, None).unwrap()
    }

    /// Non-background pixels of a pixmap and their distinct colours.
    fn painted(pm: &resvg::tiny_skia::Pixmap) -> (usize, usize) {
        let mut colours: Vec<[u8; 4]> = Vec::new();
        let mut n = 0;
        for y in 0..pm.height() {
            for x in 0..pm.width() {
                let c = rgba(pm, x, y);
                if c != BACKGROUND {
                    n += 1;
                    if !colours.contains(&c) {
                        colours.push(c);
                    }
                }
            }
        }
        (n, colours.len())
    }

    #[test]
    fn textured_planks_iso_shows_texels_over_the_same_silhouette() {
        if crate::render::assets::assets().is_none() {
            println!("no Minecraft client jar found; skipping");
            return;
        }
        let planks = lone_block("minecraft:oak_planks");
        assert!(planks.has_textures());
        let pm = render_iso(&planks, IsoCamera::SouthEast, 32).unwrap();
        let (n, colours) = painted(&pm);
        assert!(colours > 3, "textured planks show more than one colour per face, got {colours}");
        // A block the jar has no model for keeps the flat path; its full-cube
        // silhouette covers exactly the same pixels, so the two never leave seams.
        let flat = lone_block("minecraft:not_a_block");
        assert!(!flat.has_textures());
        let pf = render_iso(&flat, IsoCamera::SouthEast, 32).unwrap();
        let (nf, cf) = painted(&pf);
        assert_eq!(cf, 3, "flat cube: one colour per face");
        assert_eq!(n, nf, "textured and flat cubes cover the same pixels");
    }

    #[test]
    fn glass_pane_cutout_texels_do_not_paint() {
        if crate::render::assets::assets().is_none() {
            println!("no Minecraft client jar found; skipping");
            return;
        }
        use crate::render::assets::Face6;
        // A pane running north-south: its south piece (z 9..16) shows its east
        // face to the SE camera, textured with the see-through pane texture.
        let g = lone_block("minecraft:glass_pane[east=false,north=true,south=true,waterlogged=false,west=false]");
        let geo = g.geometry(g.get(0, 0, 0)).expect("pane model");
        let piece = geo.elements.iter().find(|e| e.to[2] > 15.0 && e.to[0] < 10.0).expect("south piece");
        let east = crate::render::texel::face(piece, Face6::East).expect("east face");
        let tw = 32;
        let pm = render_iso(&g, IsoCamera::SouthEast, tw as u32).unwrap();
        // Anchor of the only block: off_x = nv * tw / 2 + margin, off_y = margin.
        let (ax, ay) = (tw / 2 + tw, tw);
        let (mut holes, mut frame) = (0, 0);
        for y in 0..(tw + tw / 2) {
            for x in 0..tw {
                // Invert the projection on the east face's plane (u = 9).
                let (sx, sy) = (x as f32 + 0.5, y as f32 + 0.5);
                let d = (sx - tw as f32 / 2.0) / (tw as f32 / 32.0);
                let v = piece.to[0] - d;
                let yy = 16.0 - (sy - (piece.to[0] + v) * (tw as f32 / 64.0)) * 16.0 / (tw as f32 / 2.0);
                if v < piece.from[2] || v >= piece.to[2] || yy < 0.0 || yy >= 16.0 {
                    continue;
                }
                let texel = east.sample_at([piece.to[0], yy, v]);
                let pixel = rgba(&pm, (ax + x - tw / 2) as u32, (ay + y) as u32);
                if texel[3] == 0 {
                    holes += 1;
                    assert_eq!(pixel, BACKGROUND, "cut-out texel at ({x}, {y}) must not paint");
                } else {
                    frame += 1;
                    assert_ne!(pixel, BACKGROUND, "frame texel at ({x}, {y}) paints");
                }
            }
        }
        assert!(holes > 0 && frame > 0, "pane face has both holes ({holes}) and frame ({frame}) texels");
    }

    #[test]
    fn textured_topdown_grass_is_green_and_glass_shows_through() {
        if crate::render::assets::assets().is_none() {
            println!("no Minecraft client jar found; skipping");
            return;
        }
        let px = 16u32;
        let grass = render_topdown(&lone_block("minecraft:grass_block[snowy=false]"), px).unwrap();
        let (n, colours) = painted(&grass);
        assert_eq!(n, (px * px) as usize);
        assert!(colours > 1, "grass top shows its texture");
        for y in px..2 * px {
            for x in px..2 * px {
                let c = rgba(&grass, x, y);
                assert!(c[1] > c[0] + 15 && c[1] > c[2], "grass pixel {c:?} is green");
            }
        }
        // Glass over grass: the pane texture's clear middle lets the grass show.
        let mut s = Scene::new(Rect3D { origin: Point3D::new(0, 0, 0), size: Point3D::new(1, 2, 1) });
        s.insert(Point3D::new(0, 0, 0), "minecraft:grass_block[snowy=false]");
        s.insert(Point3D::new(0, 1, 0), "minecraft:glass");
        let g = VoxelGrid::from_scene(&s, None).unwrap();
        let pm = render_topdown(&g, px).unwrap();
        let geo = g.geometry(g.get(0, 1, 0)).expect("glass model");
        let up = crate::render::texel::face(&geo.elements[0], crate::render::assets::Face6::Up).expect("up face");
        let (mut through, mut frame) = (0, 0);
        for y in 0..px {
            for x in 0..px {
                let texel = up.sample_at([x as f32 + 0.5, 16.0, y as f32 + 0.5]);
                let (c, below) = (rgba(&pm, px + x, px + y), rgba(&grass, px + x, px + y));
                if texel[3] == 0 {
                    through += 1;
                    // The grass sits a block below the surface cell, so it
                    // gets the full "low surface" darkening (0.8).
                    assert_eq!(c, crate::render::grid::shade(below, 0.8), "clear glass texel shows the grass under it");
                } else {
                    frame += 1;
                }
            }
        }
        assert!(through > 0 && frame > 0, "glass top has clear ({through}) and frame ({frame}) texels");
    }

    #[test]
    fn textured_elevation_shows_facade_texels() {
        if crate::render::assets::assets().is_none() {
            println!("no Minecraft client jar found; skipping");
            return;
        }
        let g = lone_block("minecraft:stone_bricks");
        let px = 16u32;
        let pm = render_elevation(&g, ElevationSide::South, px).unwrap();
        let (n, colours) = painted(&pm);
        assert_eq!(n, (px * px) as usize, "a full block fills its square");
        assert!(colours > 3, "the facade shows its texture, got {colours} colours");
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

    /// Render a crop of a saved scene for manual inspection (`TOME_PX` / `TOME_TILE`
    /// override the 16 px per block and 24 px iso tile):
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
        let env_or = |key: &str, default: u32| std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default);
        let opts = RenderOptions {
            px: env_or("TOME_PX", 16),
            iso_tile: env_or("TOME_TILE", 24),
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
        time_views(&s);
        let t = Instant::now();
        let out = std::env::temp_dir().join("tome_render_timing");
        render_all(&s, &out).unwrap();
        println!("render_all incl. PNG encode: {:?}", t.elapsed());
    }

    /// Build the grid and render every standard view once, printing timings.
    fn time_views(s: &Scene) {
        let t = Instant::now();
        let g = VoxelGrid::from_scene(s, None).unwrap();
        println!("grid build:   {:?} (textures: {})", t.elapsed(), g.has_textures());
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
    }

    /// Per-view timings of a saved town (set `TOME_NO_TEXTURES=1` to compare the flat path):
    /// `TOME_SCENE=<path.scene> cargo test --release time_saved_scene -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn time_saved_scene() {
        let Ok(path) = std::env::var("TOME_SCENE") else {
            println!("set TOME_SCENE");
            return;
        };
        let t = Instant::now();
        let scene = Scene::load(&path).expect("load scene");
        println!("scene load:   {:?} ({} blocks)", t.elapsed(), scene.blocks.len());
        time_views(&scene);
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
    fn blockstate_variants_and_multipart_pick_models() {
        use crate::render::assets::{parse_block, pick_models, when_matches};
        let (id, props) = parse_block("minecraft:oak_stairs[facing=east,half=top,shape=straight]");
        assert_eq!(id, "oak_stairs");
        assert_eq!(props.get("half").map(String::as_str), Some("top"));

        let variants = serde_json::json!({ "variants": {
            "facing=east,half=bottom": { "model": "minecraft:block/a" },
            "facing=east,half=top": [{ "model": "minecraft:block/b", "x": 180, "y": 90, "uvlock": true }, { "model": "block/c" }],
        }});
        let picks = pick_models(&variants, &props);
        assert_eq!(picks.len(), 1);
        assert_eq!((picks[0].model.as_str(), picks[0].x, picks[0].y, picks[0].uvlock), ("block/b", 180, 90, true));

        let (_, fence) = parse_block("oak_fence[north=true,east=false,south=true,west=false]");
        let multipart = serde_json::json!({ "multipart": [
            { "apply": { "model": "block/post" } },
            { "when": { "north": "true" }, "apply": { "model": "block/side" } },
            { "when": { "east": "true" }, "apply": { "model": "block/side", "y": 90 } },
            { "when": { "OR": [{ "south": "true" }, { "west": "true" }] }, "apply": { "model": "block/or" } },
            { "when": { "AND": [{ "north": "true" }, { "west": "true|false" }] }, "apply": { "model": "block/and" } },
        ]});
        let names: Vec<String> = pick_models(&multipart, &fence).into_iter().map(|p| p.model).collect();
        assert_eq!(names, ["block/post", "block/side", "block/or", "block/and"]);
        assert!(!when_matches(&serde_json::json!({ "up": "true" }), &fence), "a missing property never matches");
    }

    #[test]
    fn jar_models_resolve_when_a_client_jar_is_present() {
        use crate::render::assets::{assets, Face6};
        let Some(a) = assets() else {
            println!("no Minecraft client jar found; skipping");
            return;
        };
        // A straight stair is a slab plus a step.
        let stairs = a.geometry("minecraft:oak_stairs[facing=east,half=bottom,shape=straight]").expect("oak stairs");
        assert_eq!(stairs.elements.len(), 2);
        // A fence with one link: post plus the rail's two bars.
        let fence = a.geometry("oak_fence[north=true,east=false,south=false,west=false]").expect("oak fence");
        assert_eq!(fence.elements.len(), 3);
        // Grass: tinted green top, dirt-brown sides; a plain cube samples its own texture.
        let grass = a.geometry("grass_block[snowy=false]").expect("grass block");
        assert!(grass.top[1] > grass.top[0] + 15 && grass.side[0] > grass.side[1], "grass top {:?} side {:?}", grass.top, grass.side);
        let planks = a.geometry("minecraft:oak_planks").expect("oak planks");
        assert_eq!(planks.elements.len(), 1);
        let (_, up) = planks.elements[0].faces.iter().find(|(f, _)| *f == Face6::Up).expect("up face");
        assert_eq!(up.sample_at(up.origin), up.texture.sample(0.0, 0.0));
        assert_eq!(up.sample_at([15.5, 16.0, 15.5]), up.texture.sample(15.5, 15.5));
        // Entity-rendered blocks and bare ids the jar lacks fall back.
        assert!(a.geometry("minecraft:chest[facing=north]").is_none());
        assert!(a.geometry("minecraft:not_a_block").is_none());
        // Bare generator states resolve through the defaults.
        assert!(a.geometry("minecraft:lantern").is_some());
        assert!(a.geometry("minecraft:oak_trapdoor[facing=north,half=bottom,open=true]").is_some());
    }

    #[test]
    fn contact_sheet_tiles_in_rows() {
        use crate::render::perspective::contact_sheet;
        let tiles: Vec<_> = (0..5).map(|_| resvg::tiny_skia::Pixmap::new(10, 6).unwrap()).collect();
        let sheet = contact_sheet(&tiles, 3, 2).unwrap();
        assert_eq!((sheet.width(), sheet.height()), (3 * 10 + 4 * 2, 2 * 6 + 3 * 2));
    }

    // ---- viewer textures (jd/viewer-textures): textured export of the HTML viewer ----

    /// 2x2 grass, a stair, a poppy, glass and a chest: every export path at once.
    fn viewer_texture_scene() -> Scene {
        let mut s = Scene::new(Rect3D { origin: Point3D::new(0, 60, 0), size: Point3D::new(8, 8, 8) });
        for (x, z) in [(1, 1), (2, 1), (1, 2), (2, 2)] {
            s.insert(Point3D::new(x, 61, z), "minecraft:grass_block[snowy=false]");
        }
        s.insert(Point3D::new(4, 61, 1), "minecraft:oak_stairs[facing=east,half=bottom,shape=straight]");
        s.insert(Point3D::new(1, 62, 1), "minecraft:poppy");
        s.insert(Point3D::new(4, 61, 4), "minecraft:glass");
        s.insert(Point3D::new(6, 61, 6), "minecraft:chest[facing=north]");
        s
    }

    #[test]
    fn viewer_exports_textured_faces_when_a_jar_is_present() {
        use crate::render::assets::assets;
        use crate::render::viewer::{export, RECORD_BYTES, ROTATED_RECORD_BYTES};
        if assets().is_none() {
            println!("no Minecraft client jar found; skipping");
            return;
        }
        let g = VoxelGrid::from_scene(&viewer_texture_scene(), None).unwrap();
        let e = export(&g).unwrap();
        let t = e.textured.expect("models give a textured buffer");
        assert_eq!(t.faces.len(), t.a_count * RECORD_BYTES + t.r_count * ROTATED_RECORD_BYTES);
        assert_eq!(e.stats.faces, e.flat.len() / RECORD_BYTES + t.a_count + t.r_count);
        // The chest has no element model: it stays on the flat path with its hand colour.
        assert_eq!(e.flat.len() / RECORD_BYTES, 6, "chest faces");
        // The poppy's cross model is two rotated quads.
        assert_eq!(t.r_count, 2, "rotated faces");
        assert_eq!(e.stats.rotated_faces, 2);
        // The 2x2 grass top merges into one 32x32 tiled quad: the grid's y range
        // starts at the grass, so its top is the plane y = 16, at x = z = 16.
        let a_recs: Vec<[i16; 5]> = t.faces[..t.a_count * RECORD_BYTES]
            .chunks(RECORD_BYTES)
            .map(|r| {
                let v = |i: usize| i16::from_le_bytes([r[i * 2], r[i * 2 + 1]]);
                [v(0), v(1), v(2), v(3), v(4)]
            })
            .collect();
        let dirs: Vec<u8> = t.faces[..t.a_count * RECORD_BYTES].chunks(RECORD_BYTES).map(|r| r[10]).collect();
        let grass_top = a_recs.iter().zip(&dirs).filter(|(r, &d)| d == 2 && r[1] == 16 && r[3] == 32).map(|(r, _)| *r).collect::<Vec<_>>();
        assert_eq!(grass_top, vec![[16, 16, 16, 32, 32]], "merged grass top");
        // Grass sides bake the tinted overlay onto dirt: one face per exposed side, no coincident pair.
        let west_sides = a_recs.iter().zip(&dirs).filter(|(r, &d)| d == 1 && r[0] == 16).count();
        assert_eq!(west_sides, 1, "west side of the 2x2 grass merges into one quad");
        // Each texture and composite is in the atlas once; the key table is compact.
        assert!(e.stats.atlas_textures >= 6, "atlas textures {}", e.stats.atlas_textures);
        assert!(e.stats.face_keys >= 8 && e.stats.face_keys <= 64, "face keys {}", e.stats.face_keys);
        assert_eq!(e.stats.atlas_size.0 % 16, 0);
        assert!(t.atlas_png.starts_with(&[0x89, b'P', b'N', b'G']));
        // Keys: the glass is translucent per the hand colour (flags bit 1), grass tops are tinted.
        let rows: Vec<Vec<f64>> = t.keys.iter().map(|k| k.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect()).collect();
        assert!(rows.iter().all(|r| r.len() == 14));
        assert!(rows.iter().any(|r| r[13] as u8 & 2 != 0), "a translucent key for glass");
        assert!(rows.iter().any(|r| r[10] < 200.0 && r[11] > r[10]), "a green-tinted key for the grass top");
    }

    #[test]
    fn viewer_page_embeds_atlas_only_with_textures() {
        use crate::render::assets::assets;
        use crate::render::viewer::write_viewer_html;
        let path = std::env::temp_dir().join("tome_viewer_textures_test").join("viewer.html");
        let stats = write_viewer_html(&viewer_texture_scene(), None, &path).unwrap();
        let html = std::fs::read_to_string(&path).unwrap();
        assert_eq!(stats.html_bytes, html.len());
        let textured = assets().is_some();
        assert_eq!(html.contains(r#"<script id="tex""#), textured);
        assert_eq!(html.contains(r#"<script id="atlas""#), textured);
        assert_eq!(html.contains(r#""tex":{"#), textured);
        assert_eq!(html.contains("fract(vUv)"), textured);
        assert_eq!(stats.atlas_textures > 0, textured);
        assert!(!html.contains("<!--@"), "template markers never reach the page");
    }
}

/// Lighting (`render::light`): the emitter table, light maps, the brightness
/// curve, smooth corners and lit perspective views.
#[cfg(test)]
mod lighting_tests {
    use std::time::Instant;

    use resvg::tiny_skia::Pixmap;

    use crate::geometry::{Point3D, Rect3D};
    use crate::render::grid::VoxelGrid;
    use crate::render::light::{emission, LightMap, Lighting, AMBIENT};
    use crate::render::perspective::{render_perspective, render_perspective_with, PerspectiveCamera, PerspectiveOptions};
    use crate::render::Scene;

    fn scene(sx: i32, sy: i32, sz: i32) -> Scene {
        Scene::new(Rect3D { origin: Point3D::new(0, 0, 0), size: Point3D::new(sx, sy, sz) })
    }

    /// Stone shell of the box `lo..=hi` (one block thick), interior left empty.
    fn shell(s: &mut Scene, lo: Point3D, hi: Point3D) {
        for x in lo.x..=hi.x {
            for y in lo.y..=hi.y {
                for z in lo.z..=hi.z {
                    if x == lo.x || x == hi.x || y == lo.y || y == hi.y || z == lo.z || z == hi.z {
                        s.insert(Point3D::new(x, y, z), "minecraft:stone");
                    }
                }
            }
        }
    }

    fn floor(s: &mut Scene, sx: i32, sz: i32, y: i32) {
        for x in 0..sx {
            for z in 0..sz {
                s.insert(Point3D::new(x, y, z), "minecraft:stone");
            }
        }
    }

    fn grid(s: &Scene) -> VoxelGrid {
        VoxelGrid::from_scene(s, None).expect("grid")
    }

    fn mean_brightness(pm: &Pixmap) -> f32 {
        let d = pm.data();
        let n = (pm.width() * pm.height()) as f32;
        d.chunks(4).map(|p| (p[0] as f32 + p[1] as f32 + p[2] as f32) / 3.0).sum::<f32>() / n
    }

    #[test]
    fn emitter_table_reads_block_state() {
        assert_eq!(emission("minecraft:lantern[hanging=true]"), 15);
        assert_eq!(emission("minecraft:wall_torch[facing=west]"), 14);
        assert_eq!(emission("torch"), 14);
        assert_eq!(emission("minecraft:soul_lantern"), 10);
        assert_eq!(emission("minecraft:campfire[lit=true]"), 15);
        assert_eq!(emission("minecraft:campfire[lit=false]"), 0);
        assert_eq!(emission("minecraft:campfire"), 15, "campfires are lit by default");
        assert_eq!(emission("minecraft:candle[candles=3,lit=true]"), 9);
        assert_eq!(emission("minecraft:red_candle[candles=4,lit=true]"), 12);
        assert_eq!(emission("minecraft:candle[candles=3]"), 0, "candles are unlit by default");
        assert_eq!(emission("minecraft:redstone_torch"), 7);
        assert_eq!(emission("minecraft:redstone_torch[lit=false]"), 0);
        assert_eq!(emission("minecraft:redstone_lamp"), 0);
        assert_eq!(emission("minecraft:redstone_lamp[lit=true]"), 15);
        assert_eq!(emission("minecraft:waxed_exposed_copper_bulb[lit=true]"), 12);
        assert_eq!(emission("minecraft:copper_bulb[lit=true]"), 15);
        assert_eq!(emission("minecraft:small_amethyst_bud"), 1);
        assert_eq!(emission("minecraft:magma_block"), 3);
        assert_eq!(emission("minecraft:ochre_froglight[axis=y]"), 15);
        assert_eq!(emission("minecraft:stone"), 0);
        assert_eq!(emission("minecraft:oak_planks"), 0);
    }

    #[test]
    fn lighting_parses_modes_and_factors() {
        assert_eq!(Lighting::parse("day").unwrap().daylight, Lighting::DAY);
        assert_eq!(Lighting::parse("night").unwrap().daylight, Lighting::NIGHT);
        assert_eq!(Lighting::parse("dusk").unwrap().tag(), "dusk");
        assert_eq!(Lighting::parse("0.3").unwrap().tag(), "light0.30");
        assert!(Lighting::parse("noon").is_err());
        assert!(Lighting::parse("1.5").is_err());
    }

    #[test]
    fn sealed_room_without_emitter_is_dark() {
        // Bounds one block wider than the box, so the columns beside it see the sky.
        let mut s = scene(11, 6, 11);
        shell(&mut s, Point3D::new(1, 0, 1), Point3D::new(9, 5, 9));
        let lm = LightMap::new(&grid(&s));
        assert_eq!(lm.sky_light(0, 3, 3), 15, "open column beside the box");
        for (x, y, z) in [(5, 3, 5), (2, 1, 2), (8, 4, 8)] {
            assert_eq!((lm.sky_light(x, y, z), lm.block_light(x, y, z)), (0, 0), "inside at {x},{y},{z}");
            assert!((lm.brightness(x, y, z, 1.0) - AMBIENT).abs() < 1e-6);
        }
        assert!(lm.brightness(0, 3, 3, 1.0) > 0.99, "open sky at noon is full brightness");
        assert!(lm.is_solid(1, 2, 5) && !lm.is_solid(5, 3, 5));
    }

    #[test]
    fn torch_lights_neighbourhood_falling_off_by_one_per_block() {
        let mut s = scene(31, 3, 31);
        floor(&mut s, 31, 31, 0);
        s.insert(Point3D::new(15, 1, 15), "minecraft:torch");
        let lm = LightMap::new(&grid(&s));
        assert_eq!(lm.emitters, 1);
        assert_eq!(lm.block_light(15, 1, 15), 14);
        for d in 0..=14 {
            let want = 14u8.saturating_sub(d as u8);
            assert_eq!(lm.block_light(15 + d, 1, 15), want, "east {d}");
            assert_eq!(lm.block_light(15, 1, 15 - d), want, "north {d}");
        }
        // Light walks the grid: Manhattan distance, not Euclidean.
        assert_eq!(lm.block_light(18, 1, 18), 8);
        assert_eq!(lm.block_light(0, 1, 0), 0);
        // Nothing stops sky light here, and the stone floor holds none.
        assert_eq!(lm.sky_light(15, 1, 15), 15);
        assert_eq!(lm.sky_light(15, 0, 15), 0);
    }

    #[test]
    fn sky_light_reaches_open_columns_and_fades_under_roofs() {
        // A 5x5 roof at y 4 over a floor; the roof block varies.
        for roof in ["minecraft:stone", "minecraft:oak_slab[type=bottom]", "minecraft:oak_stairs[facing=north,half=bottom]"] {
            let mut s = scene(11, 6, 11);
            floor(&mut s, 11, 11, 0);
            for x in 3..=7 {
                for z in 3..=7 {
                    s.insert(Point3D::new(x, 4, z), roof);
                }
            }
            let lm = LightMap::new(&grid(&s));
            assert_eq!(lm.sky_light(0, 2, 0), 15, "{roof}: open column");
            assert_eq!(lm.sky_light(2, 3, 5), 15, "{roof}: beside the roof");
            // Under the roof, light only comes in from the sides: 14 at the
            // edge, 13 one in, 12 at the centre.
            assert_eq!(lm.sky_light(3, 3, 5), 14, "{roof}: under the eave");
            assert_eq!(lm.sky_light(4, 3, 5), 13, "{roof}");
            assert_eq!(lm.sky_light(5, 3, 5), 12, "{roof}: under the centre");
            assert_eq!(lm.sky_light(5, 1, 5), 12, "{roof}: under the centre, at the floor");
            assert!(lm.brightness(5, 2, 5, 1.0) < lm.brightness(0, 2, 0, 1.0), "{roof}: shade is darker");
        }
        // A fence lets light straight through.
        let mut s = scene(11, 6, 11);
        floor(&mut s, 11, 11, 0);
        for x in 3..=7 {
            for z in 3..=7 {
                s.insert(Point3D::new(x, 4, z), "minecraft:oak_fence");
            }
        }
        let lm = LightMap::new(&grid(&s));
        assert_eq!(lm.sky_light(5, 3, 5), 15);
    }

    #[test]
    fn water_and_leaves_dim_sky_light() {
        // A single column (the grid's sides are closed), so only the light
        // falling straight down counts.
        let mut s = scene(1, 8, 1);
        s.insert(Point3D::new(0, 0, 0), "minecraft:stone");
        for y in 1..=3 {
            s.insert(Point3D::new(0, y, 0), "minecraft:water[level=0]");
        }
        s.insert(Point3D::new(0, 6, 0), "minecraft:oak_leaves[persistent=true]");
        let lm = LightMap::new(&grid(&s));
        // Through one leaf block: 14, and from there 1 less per block, air or water.
        assert_eq!(lm.sky_light(0, 6, 0), 14);
        assert_eq!(lm.sky_light(0, 5, 0), 13);
        assert_eq!(lm.sky_light(0, 4, 0), 12);
        assert_eq!(lm.sky_light(0, 3, 0), 11);
        assert_eq!(lm.sky_light(0, 2, 0), 10);
        assert_eq!(lm.sky_light(0, 1, 0), 9);
        // Water alone under open sky dims the straight-down light air would
        // pass untouched: 14, 13, 12.
        let mut s = scene(1, 5, 1);
        s.insert(Point3D::new(0, 0, 0), "minecraft:stone");
        for y in 1..=3 {
            s.insert(Point3D::new(0, y, 0), "minecraft:water[level=0]");
        }
        let lm = LightMap::new(&grid(&s));
        assert_eq!((lm.sky_light(0, 3, 0), lm.sky_light(0, 2, 0), lm.sky_light(0, 1, 0)), (14, 13, 12));
    }

    #[test]
    fn night_lit_room_is_brighter_than_an_unlit_one() {
        // Two sealed rooms; the second has a lantern under its ceiling. The
        // column at x 19..20 is open sky.
        let mut s = scene(21, 7, 9);
        shell(&mut s, Point3D::new(0, 0, 0), Point3D::new(8, 6, 8));
        shell(&mut s, Point3D::new(10, 0, 0), Point3D::new(18, 6, 8));
        s.insert(Point3D::new(14, 5, 4), "minecraft:lantern[hanging=true]");
        let lm = LightMap::new(&grid(&s));
        assert_eq!(lm.block_light(14, 5, 4), 15);
        assert_eq!(lm.block_light(14, 3, 4), 13);
        assert_eq!(lm.sky_light(14, 3, 4), 0);
        assert_eq!(lm.sky_light(19, 3, 4), 15);
        let night = Lighting::NIGHT;
        let (unlit, lit, sky) = (lm.brightness(4, 3, 4, night), lm.brightness(14, 3, 4, night), lm.brightness(19, 3, 4, night));
        assert!((unlit - AMBIENT).abs() < 1e-6, "unlit room at night {unlit}");
        assert!(lit > 0.7, "lantern room at night {lit}");
        assert!(sky > unlit && sky < 0.3, "moonlit sky {sky}");
        assert!(lit > sky, "at night the lamp-lit room ({lit}) beats the open sky ({sky})");
        // By day the open sky beats everything indoors.
        assert!(lm.brightness(19, 3, 4, 1.0) > lm.brightness(14, 3, 4, 1.0));
    }

    #[test]
    fn brightness_curve_is_monotonic_and_bounded() {
        let mut last = -1.0;
        for level in 0..=15 {
            let b = LightMap::curve(level as f32);
            assert!(b > last, "level {level}: {b} <= {last}");
            last = b;
        }
        assert!((LightMap::curve(0.0) - AMBIENT).abs() < 1e-6);
        assert!((LightMap::curve(15.0) - 1.0).abs() < 1e-6);
        assert!(LightMap::curve(7.0) > 0.3 && LightMap::curve(7.0) < 0.6);
    }

    #[test]
    fn smooth_corners_darken_against_walls() {
        // Floor with a 3-high wall along x = 0.
        let mut s = scene(7, 5, 7);
        floor(&mut s, 7, 7, 0);
        for y in 1..=3 {
            for z in 0..7 {
                s.insert(Point3D::new(0, y, z), "minecraft:stone");
            }
        }
        let lm = LightMap::new(&grid(&s));
        // Top face of the floor cell (1, 0, 3): the corners against the wall
        // (low x) are darker than the open ones (high x).
        let c = lm.face_corners([1, 1, 3], 1, 1.0);
        assert!(c[0] < c[1] && c[2] < c[3], "corners {c:?}");
        assert!((c[1] - 1.0).abs() < 1e-6 && (c[3] - 1.0).abs() < 1e-6, "open corners are fully lit: {c:?}");
        // Interpolation runs from the wall side to the open side.
        let near = lm.face_light_at([1, 1, 3], 1, 0.0, 0.5, 1.0);
        let far = lm.face_light_at([1, 1, 3], 1, 1.0, 0.5, 1.0);
        assert!(near < far);
        // Far from the wall every corner is open.
        assert_eq!(lm.face_corners([4, 1, 3], 1, 1.0), [1.0; 4]);
    }

    /// 9x6x9 sealed room; the camera stands in one corner looking across it.
    fn room_view(lantern: bool, lighting: Option<Lighting>) -> Pixmap {
        let mut s = scene(9, 6, 9);
        shell(&mut s, Point3D::new(0, 0, 0), Point3D::new(8, 5, 8));
        if lantern {
            s.insert(Point3D::new(4, 4, 4), "minecraft:lantern[hanging=true]");
        }
        let g = grid(&s);
        let map = LightMap::new(&g);
        let cam = PerspectiveCamera { eye: [1.5, 2.62, 1.5], target: [7.5, 2.0, 7.5], fov_deg: 90.0 };
        let opts = PerspectiveOptions { lighting: lighting.map(|l| (&map, l)) };
        render_perspective_with(&g, &cam, 96, 64, &opts).unwrap()
    }

    #[test]
    fn perspective_lighting_changes_mean_brightness_the_right_way() {
        let unlit = room_view(false, None);
        let day_sealed = room_view(false, Some(Lighting::day()));
        let night_sealed = room_view(false, Some(Lighting::night()));
        let night_lantern = room_view(true, Some(Lighting::night()));
        let (u, d, n, l) = (mean_brightness(&unlit), mean_brightness(&day_sealed), mean_brightness(&night_sealed), mean_brightness(&night_lantern));
        assert!(d < u * 0.3, "a sealed room lit by nothing is far darker than unlit ({d} vs {u})");
        assert!((n - d).abs() < 1.0, "no sky inside: night and day alike ({n} vs {d})");
        assert!(l > n * 2.0, "a lantern lights the room at night ({l} vs {n})");
        assert!(l < u, "even lit, the lamp-lit room is not brighter than the unlit render ({l} vs {u})");
        // Lighting off is the plain renderer, pixel for pixel.
        let mut s = scene(9, 6, 9);
        shell(&mut s, Point3D::new(0, 0, 0), Point3D::new(8, 5, 8));
        let g = grid(&s);
        let cam = PerspectiveCamera { eye: [1.5, 2.62, 1.5], target: [7.5, 2.0, 7.5], fov_deg: 90.0 };
        let plain = render_perspective(&g, &cam, 96, 64).unwrap();
        let with = render_perspective_with(&g, &cam, 96, 64, &PerspectiveOptions::default()).unwrap();
        assert_eq!(plain.data(), with.data());
    }

    /// Town-sized grid: 256x256 columns, ~4M cells; prints the light-map time.
    #[test]
    fn light_map_timing_town_sized_grid() {
        let mut s = scene(256, 128, 256);
        for x in 0..256 {
            for z in 0..256 {
                let h = 60 + ((x as f32 / 23.0).sin() * 2.0 + (z as f32 / 19.0).cos() * 2.0) as i32;
                for y in h - 3..=h {
                    s.insert(Point3D::new(x, y, z), if y == h { "minecraft:grass_block" } else { "minecraft:dirt" });
                }
                // Hollow 8x8 houses every 16 blocks, a lantern in each, a few torches outside.
                if x % 16 < 8 && z % 16 < 8 {
                    let edge = x % 16 == 0 || x % 16 == 7 || z % 16 == 0 || z % 16 == 7;
                    for y in h + 1..=h + 6 {
                        if edge || y == h + 6 {
                            s.insert(Point3D::new(x, y, z), "minecraft:oak_planks");
                        }
                    }
                    if x % 16 == 4 && z % 16 == 4 {
                        s.insert(Point3D::new(x, h + 5, z), "minecraft:lantern[hanging=true]");
                    }
                } else if x % 16 == 12 && z % 16 == 12 {
                    s.insert(Point3D::new(x, h + 1, z), "minecraft:torch");
                }
            }
        }
        // A spire so the grid is a full 64 high, like a town with its towers.
        for y in 57..121 {
            s.insert(Point3D::new(128, y, 128), "minecraft:stone_bricks");
        }
        let t = Instant::now();
        let g = grid(&s);
        println!("grid build: {:?} ({}x{}x{} = {} cells)", t.elapsed(), g.nx, g.ny, g.nz, g.cells().len());
        let t = Instant::now();
        let lm = LightMap::new(&g);
        let took = t.elapsed();
        let (sky, block) = lm.histogram();
        println!("light map: {took:?}; {} emitters; sky-lit {} cells, block-lit {} cells", lm.emitters, sky[15], block[1..].iter().sum::<usize>());
        assert_eq!(lm.emitters, 256 + 256);
        assert!(lm.len() > 4_000_000);
        // Inside a house: no sky, lantern light; outside: full sky. The map
        // is in grid-local coordinates, whose y starts at the lowest block.
        let h = |x: i32, z: i32| 60 + ((x as f32 / 23.0).sin() * 2.0 + (z as f32 / 19.0).cos() * 2.0) as i32 - g.origin.y;
        assert_eq!(lm.sky_light(4, h(4, 4) + 2, 4), 0);
        assert_eq!(lm.block_light(4, h(4, 4) + 5, 4), 15);
        assert_eq!(lm.block_light(4, h(4, 4) + 2, 4), 12);
        assert_eq!(lm.sky_light(12, h(12, 12) + 2, 12), 15);
    }
}
