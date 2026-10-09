//! Offline house showcase: builds a grid of houses on a blank flat grass world
//! (no server) and renders each one as cropped, enlarged PNGs plus its ASCII
//! floor plan, so the building generator can be judged visually.
//! Entry point: `cargo run --profile fast -- --houses [options]`.

use std::path::PathBuf;

use crate::editor::World;
use crate::generator::buildings_v2::blueprint::{build_blueprint, render_ascii};
use crate::generator::buildings_v2::footprint::{generate_footprint_biased, Plot, SizeClass};
use crate::generator::buildings_v2::{build_house, BuildCtx, BuildingContext, Culture};
use crate::generator::data::LoadedData;
use crate::geometry::{Point2D, Point3D, Rect2D, Rect3D};
use crate::noise::RNG;
use crate::render::elevation::{render_elevation, ElevationSide};
use crate::render::grid::VoxelGrid;
use crate::render::iso::{render_iso, IsoCamera};
use crate::render::topdown::render_topdown;
use crate::render::{render_views, RenderOptions};

/// Flat ground: the grass surface sits at y = 63, the first air at 64.
const GROUND_Y: i32 = 64;
/// Cells between a grid cell's edge and its plot, so overhangs, door ramps and
/// cellars never reach a neighbour or the build-area edge.
const CELL_MARGIN: i32 = 6;
/// Horizontal padding (blocks of bare ground) around everything a house placed,
/// in its render crop.
const CROP_PAD: i32 = 2;
/// Widest the whole-grid contact sheet iso may get, in pixels; its tile shrinks to fit.
const CONTACT_MAX_W: i32 = 2400;

#[derive(Debug, Clone, Copy)]
pub enum SizeSpec {
    /// Cycle Cottage, House, Hall, Manor.
    Mixed,
    One(SizeClass),
}

#[derive(Debug, Clone)]
pub struct ShowcaseOptions {
    pub culture: Culture,
    pub seed: i64,
    pub count: usize,
    pub size: SizeSpec,
    pub jetty: bool,
    /// Knee wall between the top floor's windows and the eaves (see
    /// `BuildingContext::knee_wall`).
    pub knee_wall: bool,
    /// Taller top floor on Halls and Manors (see `BuildingContext::tall_top_floor`).
    pub tall_top: bool,
    /// Pixels per block for top/elevation views; iso tiles are twice this.
    /// Default 16.
    pub px: u32,
}

/// Lowercase, filename-safe label: `Gable(Slab)` -> `gable_slab`.
fn label(debug: impl std::fmt::Debug) -> String {
    let s: String = format!("{debug:?}")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    s.split('_').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("_")
}

/// Absolute x/z bounds of grid cell (`col`, `row`).
fn cell_bounds(col: i32, row: i32, cell: i32) -> Rect2D {
    Rect2D::from_points(Point2D::new(col * cell, row * cell), Point2D::new((col + 1) * cell - 1, (row + 1) * cell - 1))
}

/// Build `opts.count` houses on a flat synthetic world, render each one and the
/// whole grid, and return the output directory.
pub async fn run_showcase(opts: &ShowcaseOptions) -> anyhow::Result<PathBuf> {
    let sizes: Vec<SizeClass> = match opts.size {
        SizeSpec::Mixed => vec![SizeClass::Cottage, SizeClass::House, SizeClass::Hall, SizeClass::Manor],
        SizeSpec::One(s) => vec![s],
    };
    let cell = if sizes.contains(&SizeClass::Manor) { 48 } else { 36 };
    let count = opts.count.max(1);
    let cols = (count as f64).sqrt().ceil() as usize;
    let rows = count.div_ceil(cols);

    let build_area = Rect3D::from_points(
        Point3D::new(0, 0, 0),
        Point3D::new(cols as i32 * cell - 1, 159, rows as i32 * cell - 1),
    );
    let world = World::synthetic(build_area, GROUND_Y);
    let mut editor = world.get_offline_editor();

    let data = LoadedData::load()?;
    let palette_id = opts.culture.palette_id();
    let palette = data
        .palettes
        .get(&palette_id)
        .ok_or_else(|| anyhow::anyhow!("palette {palette_id:?} not found"))?
        .clone();
    let roof_styles = opts.culture.roof_styles();

    let mut dir_name = format!("{}-{}", label(opts.culture), opts.seed);
    if let SizeSpec::One(s) = opts.size {
        dir_name.push_str(&format!("-{}", label(s)));
    }
    if opts.jetty {
        dir_name.push_str("-jetty");
    }
    if opts.knee_wall {
        dir_name.push_str("-knee");
    }
    if opts.tall_top {
        dir_name.push_str("-tall");
    }
    let out_dir = PathBuf::from("output/houses").join(dir_name);
    std::fs::create_dir_all(&out_dir)?;

    let mut rng = RNG::new(opts.seed);
    let mut built = Vec::new();
    for i in 0..count {
        let size_class = sizes[i % sizes.len()];
        let roof_style = roof_styles[i % roof_styles.len()];
        let (col, row) = ((i % cols) as i32, (i / cols) as i32);
        let plot_bounds = Rect2D::from_points(
            Point2D::new(col * cell + CELL_MARGIN, row * cell + CELL_MARGIN),
            Point2D::new((col + 1) * cell - 1 - CELL_MARGIN, (row + 1) * cell - 1 - CELL_MARGIN),
        );
        let plot = Plot::fully_usable(plot_bounds);
        let Some(footprint) = generate_footprint_biased(&mut rng, &plot, &size_class, opts.culture.square_bias()) else {
            println!("house {i:02}: {size_class:?} {roof_style:?}: no footprint fits the plot, skipped");
            continue;
        };
        let mut bctx = BuildingContext::new(opts.culture, size_class, roof_style);
        bctx.jetty = opts.jetty;
        bctx.knee_wall = opts.knee_wall;
        bctx.tall_top_floor = opts.tall_top;
        // Same roll as the settlement: engawa only for the culture/size that takes it.
        let (en, ed) = opts.culture.engawa_chance(size_class);
        bctx.engawa = ed > 0 && rng.rand_i32_range(0, ed as i32) < en as i32;
        let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
        match build_house(&mut ctx, footprint, &bctx, plot_bounds).await {
            Ok(house) => built.push((i, cell_bounds(col, row, cell), house)),
            Err(msg) => println!("house {i:02}: {size_class:?} {roof_style:?}: build failed, skipped: {msg}"),
        }
    }
    editor.flush_buffer().await;

    let scene = editor.capture_scene();
    let px = opts.px.max(1);

    // Contact sheet: every standard view of the whole grid, iso tile shrunk so
    // the iso stays under CONTACT_MAX_W pixels wide.
    let span = build_area.size.x + build_area.size.z;
    let contact_tile = (2 * px as i32).min(2 * CONTACT_MAX_W / span).max(4) as u32;
    render_views(&scene, &RenderOptions { px: 4, iso_tile: contact_tile, region: None }, &out_dir, "")?;

    let mut records = Vec::new();
    for (i, cell_rect, house) in &built {
        // Tight crop: everything solid the house placed above the ground (walls,
        // roof overhangs, door ramps, decks) inside its own grid cell, plus a
        // CROP_PAD ring of ground. The cell holds nothing else.
        let (cmin, cmax) = (cell_rect.min(), cell_rect.max());
        let placed = scene.blocks.keys().filter(|p| {
            p.y >= GROUND_Y && p.x >= cmin.x && p.x <= cmax.x && p.z >= cmin.y && p.z <= cmax.y && scene.is_solid(**p)
        });
        let (mut lo, mut hi) = (Point3D::new(i32::MAX, GROUND_Y - 1, i32::MAX), Point3D::new(i32::MIN, GROUND_Y, i32::MIN));
        for p in placed {
            lo = Point3D::new(lo.x.min(p.x), lo.y, lo.z.min(p.z));
            hi = Point3D::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        if lo.x > hi.x {
            println!("house {i:02}: nothing placed above ground, not rendered");
            continue;
        }
        let pad = Point3D::new(CROP_PAD, 0, CROP_PAD);
        let region = Rect3D::from_points(lo - pad, hi + pad + Point3D::new(0, 2, 0));
        let Some(g) = VoxelGrid::from_scene(&scene, Some(region)) else { continue };

        let mut renders = Vec::new();
        let mut save = |name: String, pm: resvg::tiny_skia::Pixmap| -> anyhow::Result<()> {
            pm.save_png(out_dir.join(&name))?;
            renders.push(name);
            Ok(())
        };
        for cam in [IsoCamera::SouthEast, IsoCamera::NorthWest, IsoCamera::SouthWest, IsoCamera::NorthEast] {
            save(format!("house_{i:02}_iso_{}.png", cam.tag()), render_iso(&g, cam, 2 * px)?)?;
        }
        save(format!("house_{i:02}_top.png"), render_topdown(&g, px)?)?;
        for side in ElevationSide::ALL {
            save(format!("house_{i:02}_elev_{}.png", side.name()), render_elevation(&g, side, px)?)?;
        }

        let blueprint = build_blueprint(&house.frame, &house.wall_segs, &house.floor_plan, &house.room_plan, house.has_attic);
        let plan_file = format!("house_{i:02}_plan.txt");
        std::fs::write(out_dir.join(&plan_file), render_ascii(&blueprint))?;

        let fp = house.footprint.bounds();
        let floors = house.frame.max_floors();
        let rooms = house.room_plan.rooms.len();
        let windows = house.wall_segs.windows().count();
        println!(
            "house {i:02}: {:?}, roof={:?}, floors={floors}, rooms={rooms}, timber={:?}, windows={windows}, attic={}, cellar={}, rects={}, footprint={}x{}",
            house.size_class, house.roof_style, house.timber_pattern, house.has_attic, house.has_cellar,
            house.footprint.rects().len(), fp.size.x, fp.size.y,
        );
        records.push(serde_json::json!({
            "index": i,
            "size_class": format!("{:?}", house.size_class),
            "roof_style": format!("{:?}", house.roof_style),
            "floors": floors,
            "rooms": rooms,
            "timber_pattern": format!("{:?}", house.timber_pattern),
            "windows": windows,
            "attic": house.has_attic,
            "cellar": house.has_cellar,
            "footprint_rects": house.footprint.rects().len(),
            "region": region,
            "plan": plan_file,
            "renders": renders,
        }));
    }

    let summary = serde_json::json!({
        "culture": format!("{:?}", opts.culture),
        "seed": opts.seed,
        "requested": count,
        "built": built.len(),
        "houses": records,
    });
    std::fs::write(out_dir.join("houses.json"), serde_json::to_string_pretty(&summary)?)?;
    println!("{} of {count} houses built; output in {}", built.len(), out_dir.display());
    Ok(out_dir)
}
