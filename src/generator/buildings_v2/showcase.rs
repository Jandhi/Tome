//! House showcase: builds a grid of houses and renders each one as cropped,
//! enlarged PNGs (four isometric corners, top, elevations, and a perspective
//! from inside every room) plus its ASCII floor plan, so the building
//! generator can be judged visually inside and out. Offline by default, on a
//! blank flat grass world (no server); `--live` builds into the server's build
//! area instead. `--variants` builds every house once per upper-storey
//! variant, one row per variant; `--rects` pins the footprint; `--light`
//! lights the room views (sky and lamps; the flat views stay unlit).
//! Entry point: `cargo run --profile fast -- --houses [options]`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::editor::{Editor, World};
use crate::util::HashSet;
use crate::generator::buildings_v2::blueprint::{build_blueprint, render_ascii};
use crate::generator::buildings_v2::footprint::merge::outline_from_rects;
use crate::generator::buildings_v2::footprint::{generate_footprint_biased, Footprint, Plot, SizeClass};
use crate::generator::buildings_v2::frame::CELLAR_FLOOR;
use crate::generator::buildings_v2::rooms::{Room, RoomRole};
use crate::generator::buildings_v2::{build_house, BuildCtx, BuildingContext, Culture, HouseOutput};
use crate::generator::data::LoadedData;
use crate::geometry::{Point2D, Point3D, Rect2D, Rect3D};
use crate::http_mod::GDMCHTTPProvider;
use crate::noise::RNG;
use crate::render::elevation::{render_elevation_with, ElevationSide};
use crate::render::grid::VoxelGrid;
use crate::render::iso::{render_iso_with, IsoCamera};
use crate::render::light::{LightMap, Lighting};
use crate::render::lit::Lit;
use crate::render::perspective::{contact_sheet, render_perspective_with, PerspectiveCamera, PerspectiveOptions};
use crate::render::topdown::render_topdown_with;
use crate::render::{render_views, RenderOptions, Scene};

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
/// Size of each room's perspective view.
const ROOM_W: u32 = 480;
const ROOM_H: u32 = 320;
/// Horizontal field of view inside rooms: wide, since most rooms are a few blocks across.
const ROOM_FOV: f32 = 100.0;
/// Player eye height above the cell they stand in.
const EYE_HEIGHT: f32 = 1.62;

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
    /// Light every view (sky light through windows, lanterns and candles,
    /// dark cellars inside; sky and lamps on the exterior crops and the
    /// contact sheet) under this daylight; the output directory gains the
    /// lighting's tag. `None` renders them unlit.
    pub lighting: Option<Lighting>,
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

/// Camera for a player standing in build-area-local cell `stand` on the floor
/// whose walkable level is `floor_y` (absolute), eyes 1.62 up, looking at
/// build-area-local `(x, z)` a little below eye level so the floor and
/// furniture show.
fn standing_camera(stand: Point2D, floor_y: i32, look: (f32, f32), origin: Point3D) -> PerspectiveCamera {
    let eye = [stand.x as f32 + 0.5 + origin.x as f32, floor_y as f32 + EYE_HEIGHT, stand.y as f32 + 0.5 + origin.z as f32];
    let mut target = [look.0 + origin.x as f32, floor_y as f32 + 1.0, look.1 + origin.z as f32];
    // Looking at our own feet (a one-cell room): look east instead.
    if (target[0] - eye[0]).abs() < 0.75 && (target[2] - eye[2]).abs() < 0.75 {
        target = [eye[0] + 2.0, eye[1] - 0.6, eye[2]];
    }
    PerspectiveCamera { eye, target, fov_deg: ROOM_FOV }
}

/// The cell among `cells` that passes `free` with the best view: the
/// longest clear look (`view` gives a cell's best look as `(run, target)`),
/// standing far from `centre` to get the whole room in when looks tie.
/// Ties go to the lowest (x, z), keeping the choice deterministic.
fn pick_stand(
    cells: impl Iterator<Item = (i32, i32)>,
    (cx, cz): (f32, f32),
    free: impl Fn((i32, i32)) -> bool,
    view: impl Fn((i32, i32)) -> (f32, (f32, f32)),
) -> Option<((i32, i32), (f32, f32))> {
    let score = |(x, z): (i32, i32), run: f32| run + 0.15 * ((x as f32 - cx).powi(2) + (z as f32 - cz).powi(2)).sqrt();
    cells
        .filter(|&c| free(c))
        .map(|c| {
            let (run, look) = view(c);
            (c, look, score(c, run))
        })
        .max_by(|a, b| a.2.total_cmp(&b.2).then(b.0.cmp(&a.0)))
        .map(|(c, look, _)| (c, look))
}

/// Camera inside `room`: stands on the walkable, non-stair interior cell
/// farthest from the room's centre that `free` confirms is clear (the room
/// plan does not know about roof slopes or tall items). `None` when no cell
/// qualifies.
fn room_camera(
    room: &Room,
    floor_y: i32,
    origin: Point3D,
    free: impl Fn((i32, i32)) -> bool,
    view: impl Fn((i32, i32)) -> (f32, (f32, f32)),
) -> Option<PerspectiveCamera> {
    let it = room.interior;
    let centre = ((it.min().x + it.max().x) as f32 / 2.0, (it.min().y + it.max().y) as f32 / 2.0);
    let cells = room
        .constraints
        .iter_ground()
        .map(|(cell, _)| cell)
        .filter(|&cell| room.constraints.is_walkable(cell) && !room.constraints.is_stair(cell));
    let (stand, look) = pick_stand(cells, centre, free, view)?;
    Some(standing_camera(Point2D::new(stand.0, stand.1), floor_y, look, origin))
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
    if let Some(l) = &opts.lighting {
        dir_name.push_str(&format!("-{}", l.tag()));
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
    let contact_opts = RenderOptions { px: 4, iso_tile: contact_tile, region: contact_region, lighting: opts.lighting };
    render_views(&scene, &contact_opts, &out_dir, "")?;

    // Light-map cost over every house's interior grid, reported at the end.
    let (mut light_time, mut light_grids, mut light_cells) = (Duration::ZERO, 0usize, 0usize);
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
        // Exterior views, lit (sky and lamps) like the room views when asked.
        let exterior_map = opts.lighting.map(|_| {
            let t = Instant::now();
            let map = LightMap::new(&g);
            light_time += t.elapsed();
            light_grids += 1;
            light_cells += map.len();
            map
        });
        let exterior_lit = exterior_map.as_ref().zip(opts.lighting).map(|(map, l)| Lit::new(&g, map, l));
        let exterior_lit = exterior_lit.as_ref();
        for cam in [IsoCamera::SouthEast, IsoCamera::NorthWest, IsoCamera::SouthWest, IsoCamera::NorthEast] {
            save(format!("{prefix}_iso_{}.png", cam.tag()), render_iso_with(&g, cam, 2 * px, exterior_lit)?)?;
        }
        save(format!("{prefix}_top.png"), render_topdown_with(&g, px, exterior_lit)?)?;
        for side in ElevationSide::ALL {
            save(format!("{prefix}_elev_{}.png", side.name()), render_elevation_with(&g, side, px, exterior_lit)?)?;
        }

        let blueprint = build_blueprint(&house.frame, &house.wall_segs, &house.floor_plan, &house.room_plan, house.has_attic);
        let plan_file = format!("{prefix}_plan.txt");
        std::fs::write(out_dir.join(&plan_file), render_ascii(&blueprint))?;

        // Interior views: a perspective from inside every room (and the cellar
        // from its landing), each saved on its own and tiled into one sheet.
        // The grid reaches down to the cellar, which the exterior crop leaves out.
        let mut rooms_json = Vec::new();
        let interior_lo_y = if house.has_cellar { house.frame.floor_y(CELLAR_FLOOR) + origin.y - 2 } else { region.min().y };
        let interior_region = Rect3D::from_points(Point3D::new(region.min().x, interior_lo_y, region.min().z), region.max());
        let Some(gi) = VoxelGrid::from_scene(&scene, Some(interior_region)) else { continue };
        let light_map = opts.lighting.map(|_| {
            let t = Instant::now();
            let map = LightMap::new(&gi);
            light_time += t.elapsed();
            light_grids += 1;
            light_cells += map.len();
            map
        });
        let room_opts = PerspectiveOptions { lighting: light_map.as_ref().zip(opts.lighting), fog: true };
        // A player can stand in build-area-local `cell` on walkable level
        // `floor_y` (absolute) when nothing fills the feet cell and the head
        // cell is clear; `roomy` also wants the cell above the head clear, so
        // the camera keeps away from attic eaves and low beams when it can.
        let clear = |cell: (i32, i32), floor_y: i32, roomy: bool| {
            let (gx, gz) = (cell.0 + origin.x - gi.origin.x, cell.1 + origin.z - gi.origin.z);
            let at = |dy: i32| gi.get(gx, floor_y + dy - gi.origin.y, gz);
            !gi.is_full(at(0)) && at(1) == 0 && (!roomy || at(2) == 0)
        };
        // Clear distance (blocks) of a look from `cell` along `(dx, dz)` at
        // eye and knee height, up to 16.
        let run = |cell: (i32, i32), floor_y: i32, room: Rect2D, (dx, dz): (f32, f32)| -> f32 {
            let (mut x, mut z, mut d) = (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5, 0.0);
            while d < 16.0 {
                x += dx * 0.25;
                z += dz * 0.25;
                d += 0.25;
                // Stay inside the room (its walls included): a look down a
                // doorway into the next room doesn't count.
                let (ix, iz) = (x.floor() as i32, z.floor() as i32);
                if ix < room.min().x - 1 || ix > room.max().x + 1 || iz < room.min().y - 1 || iz > room.max().y + 1 {
                    break;
                }
                let (gx, gz) = (ix + origin.x - gi.origin.x, iz + origin.z - gi.origin.z);
                if gi.get(gx, floor_y + 1 - gi.origin.y, gz) != 0 || gi.is_full(gi.get(gx, floor_y - gi.origin.y, gz)) {
                    break;
                }
            }
            d
        };
        // The best look from `cell` in `room`: the longest clear run among
        // the direction to the room's middle (preferred when it ties) and
        // sixteen compass directions, so a camera hemmed in by furniture
        // turns to face the open part of the room. Returns the run and the
        // build-area-local point to look at.
        let view = |cell: (i32, i32), floor_y: i32, room: Rect2D| -> (f32, (f32, f32)) {
            let (ex, ez) = (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5);
            let (cx, cz) = ((room.min().x + room.max().x) as f32 / 2.0 + 0.5, (room.min().y + room.max().y) as f32 / 2.0 + 0.5);
            let mut dirs: Vec<((f32, f32), f32)> = (0..16)
                .map(|i| {
                    let a = i as f32 * std::f32::consts::TAU / 16.0;
                    ((a.cos(), a.sin()), 0.0)
                })
                .collect();
            let (dx, dz) = (cx - ex, cz - ez);
            let len = (dx * dx + dz * dz).sqrt();
            if len >= 0.5 {
                dirs.push(((dx / len, dz / len), 0.5));
            }
            let mut best = (0.0f32, (cx, cz));
            for (dir, bonus) in dirs {
                let r = run(cell, floor_y, room, dir);
                if r + bonus > best.0 {
                    best = (r + bonus, (ex + dir.0 * r.max(2.0), ez + dir.1 * r.max(2.0)));
                }
            }
            best
        };
        let mut views: Vec<(String, serde_json::Value, PerspectiveCamera)> = Vec::new();
        for (ri, room) in house.room_plan.rooms.iter().enumerate() {
            let floor_y = house.frame.floor_y(room.floor) + origin.y;
            let look = |c: (i32, i32)| view(c, floor_y, room.interior);
            let cam = room_camera(room, floor_y, origin, |c| clear(c, floor_y, true), look)
                .or_else(|| room_camera(room, floor_y, origin, |c| clear(c, floor_y, false), look));
            let Some(cam) = cam else { continue };
            let level = match room.role {
                RoomRole::Attic => "attic".to_string(),
                _ => format!("floor{}", room.floor),
            };
            let name = format!("{prefix}_room_{ri:02}_{level}_{}.png", label(room.room_type));
            let info = serde_json::json!({
                "index": ri, "level": level, "type": format!("{:?}", room.room_type), "role": format!("{:?}", room.role),
                "render": name, "eye": cam.eye,
            });
            views.push((name, info, cam));
        }
        if house.has_cellar {
            // The cellar is not a room-plan room: stand anywhere free under the core.
            let floor_y = house.frame.floor_y(CELLAR_FLOOR) + origin.y;
            let core = house.footprint.rects()[0];
            let inner = core.shrink(1);
            let centre = ((inner.min().x + inner.max().x) as f32 / 2.0, (inner.min().y + inner.max().y) as f32 / 2.0);
            let look = |c: (i32, i32)| view(c, floor_y, inner);
            let stand = pick_stand(inner.iter().map(|p| (p.x, p.y)), centre, |c| clear(c, floor_y, true), look)
                .or_else(|| pick_stand(inner.iter().map(|p| (p.x, p.y)), centre, |c| clear(c, floor_y, false), look));
            if let Some((stand, look)) = stand {
                let cam = standing_camera(Point2D::new(stand.0, stand.1), floor_y, look, origin);
                let name = format!("{prefix}_room_cellar.png");
                let info = serde_json::json!({ "level": "cellar", "type": "Cellar", "role": "Cellar", "render": name, "eye": cam.eye });
                views.push((name, info, cam));
            }
        }
        if !views.is_empty() {
            let mut tiles = Vec::new();
            for (name, info, cam) in &views {
                let pm = render_perspective_with(&gi, cam, ROOM_W, ROOM_H, &room_opts)?;
                pm.save_png(out_dir.join(name))?;
                tiles.push(pm);
                rooms_json.push(info.clone());
            }
            let cols = (tiles.len() as f64).sqrt().ceil() as usize;
            let sheet_name = format!("{prefix}_rooms.png");
            contact_sheet(&tiles, cols, 8)?.save_png(out_dir.join(&sheet_name))?;
            renders.push(sheet_name);
        }

        // Cutaways: a top-down view of each storey clipped at its ceiling, so
        // the walls, doors and furniture of that floor show as placed. One
        // image per level (cellar, floors, attic) and a sheet of them all.
        let mut cutaways = Vec::new();
        // Per level: its name and the slices it is made of, each a rect
        // (build-area local) with the absolute y band to keep: from the floor
        // slab up to the storey's top air row, so the ceiling and everything
        // above are cut off. The roof's own blocks are left out everywhere.
        // The ground floor keeps its whole surroundings (`None`); upper levels
        // keep only their own rooms plus one cell of wall, or lower wings'
        // ceilings would cover the side rooms. Attic rooms over lower wings
        // sit at their own height, so each attic room is its own slice.
        let wall_rows = house.frame.wall_height() as i32;
        let band = |floor_y: i32, rows: i32| (floor_y - 1 + origin.y, floor_y + rows - 1 + origin.y);
        let mut levels: Vec<(String, Option<Vec<(Rect2D, i32, i32)>>, i32)> = Vec::new();
        if house.has_cellar {
            let (lo, hi) = band(house.frame.floor_y(CELLAR_FLOOR), wall_rows);
            levels.push(("cellar".to_string(), Some(vec![(house.footprint.rects()[0], lo, hi)]), 1));
        }
        for f in house.frame.floors() {
            let (lo, hi) = band(house.frame.floor_y(f), wall_rows);
            let slices: Vec<(Rect2D, i32, i32)> =
                house.frame.active_rects(f).iter().filter_map(|&i| house.frame.rect_at(i, f)).map(|r| (r, lo, hi)).collect();
            levels.push((format!("floor{f}"), (f > 0).then_some(slices), 1));
        }
        if house.has_attic {
            let slices: Vec<(Rect2D, i32, i32)> = house
                .room_plan
                .rooms
                .iter()
                .filter(|r| r.role == RoomRole::Attic)
                .map(|r| {
                    let (lo, hi) = band(house.frame.floor_y(r.floor), wall_rows);
                    (r.rect, lo, hi)
                })
                .collect();
            levels.push(("attic".to_string(), Some(slices), 1));
        }
        for (level, slices, margin) in &levels {
            let (lo_y, hi_y) = match slices {
                None => band(house.frame.floor_y(0), wall_rows),
                Some(sl) => (sl.iter().map(|s| s.1).min().unwrap_or(0), sl.iter().map(|s| s.2).max().unwrap_or(0)),
            };
            if slices.as_ref().is_some_and(|sl| sl.is_empty()) {
                continue;
            }
            let cut = Rect3D::from_points(Point3D::new(region.min().x, lo_y, region.min().z), Point3D::new(region.max().x, hi_y, region.max().z));
            let roof: HashSet<Point3D> = house.roof_cells.iter().map(|p| *p + origin).collect();
            let masked;
            let source = match slices {
                None if roof.is_empty() => &scene,
                None => {
                    let mut sub = Scene::new(scene.bounds);
                    for (p, &i) in &scene.blocks {
                        if cut.contains(*p) && !roof.contains(p) {
                            sub.insert(*p, &scene.palette[i as usize]);
                        }
                    }
                    masked = sub;
                    &masked
                }
                Some(sl) => {
                    let inside = |p: &Point3D| {
                        let local = Point2D::new(p.x - origin.x, p.z - origin.z);
                        sl.iter().any(|(r, lo, hi)| {
                            let (a, b) = (r.min(), r.max());
                            p.y >= *lo
                                && p.y <= *hi
                                && local.x >= a.x - margin
                                && local.x <= b.x + margin
                                && local.y >= a.y - margin
                                && local.y <= b.y + margin
                        })
                    };
                    let mut sub = Scene::new(scene.bounds);
                    for (p, &i) in &scene.blocks {
                        if inside(p) && !roof.contains(p) {
                            sub.insert(*p, &scene.palette[i as usize]);
                        }
                    }
                    masked = sub;
                    &masked
                }
            };
            let Some(gc) = VoxelGrid::from_scene(source, Some(cut)) else { continue };
            // Unlit: a light map of the clipped grid would see sky through the missing roof.
            // A near-top-down perspective: high above, tilted a little toward
            // the north, narrow field of view (close to orthographic) and no
            // fog, so walls and furniture keep some depth while the plan stays
            // readable. The top-down keeps the plan exact.
            let iso = {
                let (lo, hi) = (gc.origin, Point3D::new(gc.origin.x + gc.nx, gc.origin.y + gc.ny, gc.origin.z + gc.nz));
                let centre = [(lo.x + hi.x) as f32 / 2.0, (lo.y + hi.y) as f32 / 2.0, (lo.z + hi.z) as f32 / 2.0];
                let extent = (gc.nx.max(gc.nz) as f32 + 4.0) * 1.15;
                let fov: f32 = 25.0;
                let dist = extent / 2.0 / (fov.to_radians() / 2.0).tan();
                let tilt: f32 = 22.0f32.to_radians();
                let eye = [centre[0], centre[1] + dist * tilt.cos(), centre[2] + dist * tilt.sin()];
                let cam = PerspectiveCamera { eye, target: centre, fov_deg: fov };
                let w = (extent * px as f32) as u32;
                let opts = PerspectiveOptions { lighting: None, fog: false };
                render_perspective_with(&gc, &cam, w, w * 3 / 4, &opts)?
            };
            let top = render_topdown_with(&gc, px, None)?;
            for (suffix, pm) in [("", &iso), ("_top", &top)] {
                let name = format!("{prefix}_cutaway_{level}{suffix}.png");
                pm.save_png(out_dir.join(&name))?;
                renders.push(name);
            }
            cutaways.push((iso, top));
        }
        if !cutaways.is_empty() {
            // Sheet: one column per level, iso above its top-down.
            let n = cutaways.len();
            let (isos, tops): (Vec<_>, Vec<_>) = cutaways.into_iter().unzip();
            let tiles: Vec<_> = isos.into_iter().chain(tops).collect();
            let name = format!("{prefix}_cutaways.png");
            contact_sheet(&tiles, n, 8)?.save_png(out_dir.join(&name))?;
            renders.push(name);
        }

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
            "rooms": rooms_json,
        }));
    }

    let mut summary = serde_json::json!({
        "culture": format!("{:?}", opts.culture),
        "seed": opts.seed,
        "live": opts.live,
        "variants": variants.iter().map(|v| v.tag()).collect::<Vec<_>>(),
        "grid_size": [grid_w, grid_d],
        "requested": count * variants.len(),
        "built": built.len(),
        "houses": records,
    });
    if let Some(l) = &opts.lighting {
        summary["lighting"] = serde_json::json!({ "daylight": l.daylight, "smooth": l.smooth, "tag": l.tag() });
        println!(
            "room views lit ({}, daylight {:.2}): light maps for {light_grids} grids, {light_cells} cells, in {:.1?} total",
            l.tag(), l.daylight, light_time,
        );
    }
    std::fs::write(out_dir.join("houses.json"), serde_json::to_string_pretty(&summary)?)?;
    println!(
        "{} of {} houses built on a {grid_w}x{grid_d} grid; output in {}",
        built.len(), count * variants.len(), out_dir.display(),
    );
    Ok(out_dir)
}
