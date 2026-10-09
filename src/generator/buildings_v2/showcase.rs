//! House showcase: builds a grid of houses and renders each one as cropped,
//! enlarged PNGs plus its ASCII floor plan, so the building generator can be
//! judged visually. Offline by default, on a blank flat grass world (no
//! server); `--live` builds into the server's build area instead. `--variants`
//! builds every house once per upper-storey variant, one row per variant.
//! Entry point: `cargo run --profile fast -- --houses [options]`.

use std::path::PathBuf;

use crate::editor::{Editor, World};
use crate::generator::buildings_v2::blueprint::{build_blueprint, render_ascii};
use crate::generator::buildings_v2::footprint::merge::outline_from_rects;
use crate::generator::buildings_v2::footprint::{generate_footprint_biased, Footprint, Plot, SizeClass};
use crate::generator::buildings_v2::{build_house, BuildCtx, BuildingContext, Culture, HouseOutput};
use crate::generator::data::LoadedData;
use crate::geometry::{Point2D, Point3D, Rect2D, Rect3D};
use crate::http_mod::GDMCHTTPProvider;
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

/// Upper-storey variant a house is built with (see `BuildingContext::knee_wall`
/// and `BuildingContext::tall_top_floor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Base,
    Knee,
    Tall,
    Both,
}

impl Variant {
    /// Every variant, in `--variants` row order.
    pub const ALL: [Variant; 4] = [Variant::Base, Variant::Knee, Variant::Tall, Variant::Both];

    /// The variant the single-row flags select.
    pub fn from_flags(knee_wall: bool, tall_top: bool) -> Self {
        match (knee_wall, tall_top) {
            (false, false) => Variant::Base,
            (true, false) => Variant::Knee,
            (false, true) => Variant::Tall,
            (true, true) => Variant::Both,
        }
    }

    pub fn knee_wall(self) -> bool {
        matches!(self, Variant::Knee | Variant::Both)
    }

    pub fn tall_top(self) -> bool {
        matches!(self, Variant::Tall | Variant::Both)
    }

    /// Short name used in file names, houses.json and the console.
    pub fn tag(self) -> &'static str {
        match self {
            Variant::Base => "base",
            Variant::Knee => "knee",
            Variant::Tall => "tall",
            Variant::Both => "both",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShowcaseOptions {
    pub culture: Culture,
    pub seed: i64,
    pub count: usize,
    pub size: SizeSpec,
    pub jetty: bool,
    /// Knee wall between the top floor's windows and the eaves (see
    /// `BuildingContext::knee_wall`). Single-row mode only.
    pub knee_wall: bool,
    /// Taller top floor on Halls and Manors (see `BuildingContext::tall_top_floor`).
    /// Single-row mode only.
    pub tall_top: bool,
    /// Build every house once per `Variant::ALL`, one row of `count` houses per
    /// variant, so the variants of house `i` line up in a column.
    pub variants: bool,
    /// Build into the live server's build area (from its minimum corner) instead
    /// of a synthetic flat world; the renders are still written afterwards.
    pub live: bool,
    /// Pixels per block for top/elevation views; iso tiles are twice this.
    /// Default 16.
    pub px: u32,
    /// Fixed footprint for every house instead of a generated one: rects
    /// relative to the plot's minimum corner, core first (see [`parse_rects`]).
    /// Size class and roof style still cycle, so `--count 3` on a medieval
    /// culture shows the same footprint under all three gable pitches.
    pub rects: Option<Vec<Rect2D>>,
}

/// Parse a `--rects` spec: `x,z,w,h;x,z,w,h;...`, one rect per `;`, core first.
pub fn parse_rects(spec: &str) -> anyhow::Result<Vec<Rect2D>> {
    let mut rects = Vec::new();
    for part in spec.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        let nums: Vec<i32> = part
            .split(',')
            .map(|n| n.trim().parse::<i32>().map_err(|e| anyhow::anyhow!("bad number {n:?} in rect {part:?}: {e}")))
            .collect::<Result<_, _>>()?;
        let [x, z, w, h] = nums[..] else {
            anyhow::bail!("rect {part:?} must be x,z,w,h");
        };
        if w < 1 || h < 1 {
            anyhow::bail!("rect {part:?} must have a positive size");
        }
        rects.push(Rect2D::new(Point2D::new(x, z), Point2D::new(w, h)));
    }
    if rects.is_empty() {
        anyhow::bail!("--rects needs at least one rect: x,z,w,h[;x,z,w,h...]");
    }
    Ok(rects)
}

/// Lowercase, filename-safe label: `Gable(Slab)` -> `gable_slab`.
fn label(debug: impl std::fmt::Debug) -> String {
    let s: String = format!("{debug:?}")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    s.split('_').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("_")
}

/// Build-area-local x/z bounds of grid cell (`col`, `row`).
fn cell_bounds(col: i32, row: i32, cell: i32) -> Rect2D {
    Rect2D::from_points(Point2D::new(col * cell, row * cell), Point2D::new((col + 1) * cell - 1, (row + 1) * cell - 1))
}

/// Grid layout: cell size in blocks and (columns, rows) in cells.
pub fn grid_layout(opts: &ShowcaseOptions) -> (i32, usize, usize) {
    let manor = matches!(opts.size, SizeSpec::Mixed | SizeSpec::One(SizeClass::Manor));
    let cell = if manor { 48 } else { 36 };
    let count = opts.count.max(1);
    if opts.variants {
        // One row per variant; house i sits in column i of every row.
        (cell, count, Variant::ALL.len())
    } else {
        let cols = (count as f64).sqrt().ceil() as usize;
        (cell, cols, count.div_ceil(cols))
    }
}

/// One house as built, with where it stands.
struct Built {
    i: usize,
    variant: Variant,
    /// Build-area-local grid cell.
    cell: Rect2D,
    /// Lowest terrain under the cell before anything was built (local y).
    ground_y: i32,
    house: HouseOutput,
}

/// Build the houses, render each one and the whole grid, and return the output
/// directory.
pub async fn run_showcase(opts: &ShowcaseOptions) -> anyhow::Result<PathBuf> {
    if opts.variants && (opts.knee_wall || opts.tall_top) {
        anyhow::bail!("--variants already builds every variant; drop --knee-wall / --tall-top");
    }
    let sizes: Vec<SizeClass> = match opts.size {
        SizeSpec::Mixed => vec![SizeClass::Cottage, SizeClass::House, SizeClass::Hall, SizeClass::Manor],
        SizeSpec::One(s) => vec![s],
    };
    let count = opts.count.max(1);
    let (cell, cols, rows) = grid_layout(opts);
    let (grid_w, grid_d) = (cols as i32 * cell, rows as i32 * cell);
    let variants: Vec<Variant> = if opts.variants {
        Variant::ALL.to_vec()
    } else {
        vec![Variant::from_flags(opts.knee_wall, opts.tall_top)]
    };

    // The world: the server's, with the grid laid out from its build area's
    // minimum corner, or a synthetic flat one exactly the size of the grid.
    let world = if opts.live {
        let world = World::new(&GDMCHTTPProvider::new()).await?;
        let size = world.build_area.size;
        if size.x < grid_w || size.z < grid_d {
            anyhow::bail!(
                "build area is {}x{} (x by z) but the grid needs {grid_w}x{grid_d} \
                 ({cols} x {rows} cells of {cell} blocks); set a larger build area",
                size.x, size.z,
            );
        }
        world
    } else {
        let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(grid_w - 1, 159, grid_d - 1));
        World::synthetic(build_area, GROUND_Y)
    };
    // Generator coordinates are build-area local; scene and printed coordinates
    // are absolute (local + origin).
    let origin = world.build_area.origin;
    let to_abs = |p: Point2D| Point2D::new(p.x + origin.x, p.y + origin.z);
    let mut editor: Editor = if opts.live { world.get_editor() } else { world.get_offline_editor() };

    let data = LoadedData::load()?;
    let palette_id = opts.culture.palette_id();
    let palette = data
        .palettes
        .get(&palette_id)
        .ok_or_else(|| anyhow::anyhow!("palette {palette_id:?} not found"))?
        .clone();
    let roof_styles = opts.culture.roof_styles();

    let mut dir_name = format!("{}-{}", label(opts.culture), opts.seed);
    if opts.live {
        dir_name.insert_str(0, "live-");
    }
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
    if opts.variants {
        dir_name.push_str("-variants");
    }
    if opts.rects.is_some() {
        dir_name.push_str("-rects");
    }
    let out_dir = PathBuf::from("output/houses").join(dir_name);
    std::fs::create_dir_all(&out_dir)?;

    // One seed per house, drawn up front (each is exactly what `rng.derive()`
    // would seed), so house i gets the same footprint, roof and engawa roll
    // whatever variant or row it is built in.
    let mut rng = RNG::new(opts.seed);
    let house_seeds: Vec<i64> = (0..count).map(|_| rng.next_i64()).collect();

    let mut built: Vec<Built> = Vec::new();
    for (row_block, &variant) in variants.iter().enumerate() {
        let tag = variant.tag();
        for i in 0..count {
            let size_class = sizes[i % sizes.len()];
            let roof_style = roof_styles[i % roof_styles.len()];
            let (col, row) = if opts.variants {
                (i as i32, row_block as i32)
            } else {
                ((i % cols) as i32, (i / cols) as i32)
            };
            let cell_rect = cell_bounds(col, row, cell);
            let ground_y = cell_rect
                .iter()
                .filter_map(|p| editor.world().get_height_at(p))
                .min()
                .unwrap_or(GROUND_Y);
            let plot_bounds = Rect2D::from_points(
                Point2D::new(col * cell + CELL_MARGIN, row * cell + CELL_MARGIN),
                Point2D::new((col + 1) * cell - 1 - CELL_MARGIN, (row + 1) * cell - 1 - CELL_MARGIN),
            );
            let mut house_rng = RNG::new(house_seeds[i]);
            let footprint = match &opts.rects {
                Some(rects) => {
                    // Fixed footprint, shifted from plot-relative to build-area-local.
                    let shift = plot_bounds.min();
                    let rects: Vec<Rect2D> = rects.iter().map(|r| Rect2D::new(r.min() + shift, r.size)).collect();
                    if let Some(r) = rects.iter().find(|r| !plot_bounds.contains_rect(r)) {
                        anyhow::bail!(
                            "--rects: rect at {},{} size {}x{} does not fit the {}x{} plot",
                            r.min().x - shift.x, r.min().y - shift.y, r.size.x, r.size.y,
                            plot_bounds.size.x, plot_bounds.size.y,
                        );
                    }
                    Footprint::new(outline_from_rects(&rects), rects)
                }
                None => {
                    let plot = Plot::fully_usable(plot_bounds);
                    match generate_footprint_biased(&mut house_rng, &plot, &size_class, opts.culture.square_bias()) {
                        Some(f) => f,
                        None => {
                            println!("house {tag} {i:02}: {size_class:?} {roof_style:?}: no footprint fits the plot, skipped");
                            continue;
                        }
                    }
                }
            };
            let mut bctx = BuildingContext::new(opts.culture, size_class, roof_style);
            bctx.jetty = opts.jetty;
            bctx.knee_wall = variant.knee_wall();
            bctx.tall_top_floor = variant.tall_top();
            // Same roll as the settlement: engawa only for the culture/size that takes it.
            let (en, ed) = opts.culture.engawa_chance(size_class);
            bctx.engawa = ed > 0 && house_rng.rand_i32_range(0, ed as i32) < en as i32;
            let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut house_rng);
            match build_house(&mut ctx, footprint, &bctx, plot_bounds).await {
                Ok(house) => built.push(Built { i, variant, cell: cell_rect, ground_y, house }),
                Err(msg) => println!("house {tag} {i:02}: {size_class:?} {roof_style:?}: build failed, skipped: {msg}"),
            }
        }
    }
    editor.flush_buffer().await;

    // Where everything stands, in absolute coordinates, so it can be found in game.
    for (row_block, &variant) in variants.iter().enumerate() {
        let row_houses: Vec<&Built> = built.iter().filter(|b| b.variant == variant).collect();
        println!("row {row_block} ({}): {} houses", variant.tag(), row_houses.len());
        for b in &row_houses {
            let (lo, hi) = (to_abs(b.cell.min()), to_abs(b.cell.max()));
            println!(
                "  house {} {:02}: {:?} at x {}..{}, z {}..{}",
                variant.tag(), b.i, b.house.size_class, lo.x, hi.x, lo.y, hi.y,
            );
        }
        if let Some(first) = row_houses.first() {
            // Stand on untouched ground just inside the cell's north-west corner,
            // looking south-east at the house.
            let corner = first.cell.min() + Point2D::new(2, 2);
            let y = editor.world().get_height_at(corner).unwrap_or(first.ground_y) + origin.y;
            let abs = to_abs(corner);
            println!("  /tp @s {} {} {} -45 20", abs.x, y, abs.y);
        }
    }

    let scene = editor.capture_scene();
    let px = opts.px.max(1);

    // Contact sheet: every standard view of the whole grid, iso tile shrunk so
    // the iso stays under CONTACT_MAX_W pixels wide. Live, the build area can be
    // larger than the grid and full of terrain, so crop to the grid.
    let span = grid_w + grid_d;
    let contact_tile = (2 * px as i32).min(2 * CONTACT_MAX_W / span).max(4) as u32;
    let contact_region = if opts.live {
        let ground = built.iter().map(|b| b.ground_y).min().unwrap_or(GROUND_Y) + origin.y;
        let top = origin.y + editor.world().build_area.size.y - 1;
        Some(Rect3D::from_points(
            Point3D::new(origin.x, ground - 4, origin.z),
            Point3D::new(origin.x + grid_w - 1, top, origin.z + grid_d - 1),
        ))
    } else {
        None
    };
    render_views(&scene, &RenderOptions { px: 4, iso_tile: contact_tile, region: contact_region }, &out_dir, "")?;

    let mut records = Vec::new();
    for b in &built {
        let (i, house, tag) = (b.i, &b.house, b.variant.tag());
        // Tight crop: everything solid the house placed above the ground (walls,
        // roof overhangs, door ramps, decks) inside its own grid cell, plus a
        // CROP_PAD ring of ground. The cell holds nothing else.
        let (cmin, cmax) = (to_abs(b.cell.min()), to_abs(b.cell.max()));
        let ground_y = b.ground_y + origin.y;
        let placed = scene.blocks.keys().filter(|p| {
            p.y >= ground_y && p.x >= cmin.x && p.x <= cmax.x && p.z >= cmin.y && p.z <= cmax.y && scene.is_solid(**p)
        });
        let (mut lo, mut hi) = (Point3D::new(i32::MAX, ground_y - 1, i32::MAX), Point3D::new(i32::MIN, ground_y, i32::MIN));
        for p in placed {
            lo = Point3D::new(lo.x.min(p.x), lo.y, lo.z.min(p.z));
            hi = Point3D::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        if lo.x > hi.x {
            println!("house {tag} {i:02}: nothing placed above ground, not rendered");
            continue;
        }
        let pad = Point3D::new(CROP_PAD, 0, CROP_PAD);
        let region = Rect3D::from_points(lo - pad, hi + pad + Point3D::new(0, 2, 0));
        let Some(g) = VoxelGrid::from_scene(&scene, Some(region)) else { continue };

        let prefix = format!("house_{tag}_{i:02}");
        let mut renders = Vec::new();
        let mut save = |name: String, pm: resvg::tiny_skia::Pixmap| -> anyhow::Result<()> {
            pm.save_png(out_dir.join(&name))?;
            renders.push(name);
            Ok(())
        };
        for cam in [IsoCamera::SouthEast, IsoCamera::NorthWest, IsoCamera::SouthWest, IsoCamera::NorthEast] {
            save(format!("{prefix}_iso_{}.png", cam.tag()), render_iso(&g, cam, 2 * px)?)?;
        }
        save(format!("{prefix}_top.png"), render_topdown(&g, px)?)?;
        for side in ElevationSide::ALL {
            save(format!("{prefix}_elev_{}.png", side.name()), render_elevation(&g, side, px)?)?;
        }

        let blueprint = build_blueprint(&house.frame, &house.wall_segs, &house.floor_plan, &house.room_plan, house.has_attic);
        let plan_file = format!("{prefix}_plan.txt");
        std::fs::write(out_dir.join(&plan_file), render_ascii(&blueprint))?;

        let fp = house.footprint.bounds();
        let floors = house.frame.max_floors();
        let rooms = house.room_plan.rooms.len();
        let windows = house.wall_segs.windows().count();
        println!(
            "house {tag} {i:02}: {:?}, roof={:?}, floors={floors}, rooms={rooms}, timber={:?}, windows={windows}, attic={}, cellar={}, rects={}, footprint={}x{}",
            house.size_class, house.roof_style, house.timber_pattern, house.has_attic, house.has_cellar,
            house.footprint.rects().len(), fp.size.x, fp.size.y,
        );
        records.push(serde_json::json!({
            "index": i,
            "variant": tag,
            "size_class": format!("{:?}", house.size_class),
            "roof_style": format!("{:?}", house.roof_style),
            "floors": floors,
            "rooms": rooms,
            "timber_pattern": format!("{:?}", house.timber_pattern),
            "windows": windows,
            "attic": house.has_attic,
            "cellar": house.has_cellar,
            "footprint_rects": house.footprint.rects().len(),
            "footprint_size": [fp.size.x, fp.size.y],
            "cell": [cmin.x, cmin.y, cmax.x, cmax.y],
            "region": region,
            "plan": plan_file,
            "renders": renders,
        }));
    }

    let summary = serde_json::json!({
        "culture": format!("{:?}", opts.culture),
        "seed": opts.seed,
        "live": opts.live,
        "variants": variants.iter().map(|v| v.tag()).collect::<Vec<_>>(),
        "grid_size": [grid_w, grid_d],
        "requested": count * variants.len(),
        "built": built.len(),
        "houses": records,
    });
    std::fs::write(out_dir.join("houses.json"), serde_json::to_string_pretty(&summary)?)?;
    println!(
        "{} of {} houses built on a {grid_w}x{grid_d} grid; output in {}",
        built.len(), count * variants.len(), out_dir.display(),
    );
    Ok(out_dir)
}
