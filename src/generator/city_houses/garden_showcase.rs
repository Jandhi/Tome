//! `--gardens`: house gardens on fixed plots, no town around them.
//!
//! One test street per density [`Ring`]: a road along the north edge, a row
//! of fixed-size houses facing it with that ring's side gap, and the ground
//! behind them as one lot. The real [`plan_gardens`] hands each house its
//! garden and [`furnish_garden`] lays it out, cycling through every
//! [`GardenLayout`] so each one shows up in every street (a desert street
//! gets what a desert town gets: work yards).

use std::path::PathBuf;

use crate::editor::{Editor, World};
use crate::generator::buildings_v2::footprint::merge::outline_from_rects;
use crate::generator::buildings_v2::footprint::{Footprint, SizeClass};
use crate::generator::buildings_v2::showcase::house_context;
use crate::generator::buildings_v2::{build_house, BuildCtx, Culture, HouseSeed};
use crate::generator::data::LoadedData;
use crate::generator::open_space::{furnish_garden, GardenStyle, Theme};
use crate::generator::paths::PathType;
use crate::generator::BuildClaim;
use crate::geometry::{Cardinal, Point2D, Point3D, Rect2D, Rect3D};
use crate::http_mod::GDMCHTTPProvider;
use crate::noise::RNG;
use crate::render::{render_views, RenderOptions};
use crate::util::HashSet;

use super::plots::{garden_owners, plan_gardens, PlacedHouse, Ring};
use super::walk::synthetic_plot_bounds;

const GROUND_Y: i32 = 64;
/// Blocks of open ground around the streets.
const MARGIN: i32 = 4;
const ROAD_WIDTH: i32 = 3;
/// Blocks between one street's lot and the next street's road.
const STREET_GAP: i32 = 4;
const MAX_FRONT: i32 = 10;
/// Roof palettes (`data/palettes/roofs/`) with a contrasting trim, cycled over
/// the medieval houses.
const TRIM_ROOFS: [&str; 5] = ["brick_roof", "oak_wood_roof", "red_wood_roof", "acacia_wood_roof", "blackstone_roof"];

const MAX_DEPTH: i32 = 9;

/// Build `count` houses per ring street for each of `cultures` (one block of
/// three streets per culture, stacked north to south), garden them, and render
/// the lot into `output/gardens/<culture|all>-<seed>[-live]/` (whole grid, then
/// one close-up per street). `live` builds into the server's build area from
/// its minimum corner instead of a synthetic flat world, then teleports the
/// players to look over the first street. `at` shifts the whole grid from the
/// build area's minimum corner (live only), to build clear of what is there.
/// Returns the output directory.
pub async fn run_garden_showcase(cultures: &[Culture], seed: i64, count: usize, live: bool, at: Point2D) -> anyhow::Result<PathBuf> {
    let data = LoadedData::load()?;
    let rings = [Ring::Core, Ring::Middle, Ring::Outer];
    let count = count.max(1) as i32;

    // Each street: road, then a lot deep enough for the deepest house plus the
    // ring's garden and a little more, so a capped garden shows where it stops.
    let lot_depth = |r: Ring| MAX_DEPTH + r.garden_depth() + 2;
    let street_depth = |r: Ring| ROAD_WIDTH + lot_depth(r) + STREET_GAP;
    let width = 2 * MARGIN + count * (MAX_FRONT + Ring::Outer.side_gap());
    let block_depth: i32 = rings.iter().map(|&r| street_depth(r)).sum();
    let depth = 2 * MARGIN + cultures.len() as i32 * block_depth;
    let provider = GDMCHTTPProvider::new();
    let world = if live {
        let world = World::new(&provider).await?;
        let size = world.build_area.size;
        if size.x < at.x + width || size.z < at.y + depth {
            anyhow::bail!(
                "build area is {}x{} (x by z) but {} culture(s) of {count} houses need {width}x{depth} from {},{}; set a larger build area, move --at or pass a smaller --count",
                size.x, size.z, cultures.len(), at.x, at.y,
            );
        }
        world
    } else {
        let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(width - 1, 159, depth - 1));
        World::synthetic(build_area, GROUND_Y)
    };
    let at = if live { at } else { Point2D::new(0, 0) };
    // West and east edges of the streets.
    let (x_lo, x_hi) = (at.x + MARGIN, at.x + width - 1 - MARGIN);
    let mut editor: Editor = if live { world.get_editor() } else { world.get_offline_editor() };
    let mut rng = RNG::new(seed);

    let mut houses: Vec<PlacedHouse> = Vec::new();
    let mut lots: Vec<HashSet<Point2D>> = Vec::new();
    // Per street (= per lot): its culture, ring, rect and ground height.
    let mut streets: Vec<(Culture, Ring, Rect2D, i32)> = Vec::new();
    let mut z0 = at.y + MARGIN;
    for &culture in cultures {
        let palette = data
            .palettes
            .get(&culture.palette_id())
            .ok_or_else(|| anyhow::anyhow!("palette for {culture:?} not found"))?
            .clone();
        for &ring in &rings {
            let street = lots.len();
            // Road along the north edge of the street.
            let road = Rect2D::from_points(Point2D::new(x_lo, z0), Point2D::new(x_hi, z0 + ROAD_WIDTH - 1));
            let ground = editor.world().get_height_at(road.min()).unwrap_or(GROUND_Y);
            for c in road.iter() {
                let h = editor.world().get_height_at(c).unwrap_or(ground);
                editor.place_block_forced(&"minecraft:stone_bricks".into(), Point3D::new(c.x, h - 1, c.y)).await;
                editor.world_mut().claim(c, BuildClaim::Path(PathType::Pavement));
            }
            let front_z = z0 + ROAD_WIDTH;
            let lot_rect = Rect2D::from_points(
                Point2D::new(x_lo, front_z),
                Point2D::new(x_hi, front_z + lot_depth(ring) - 1),
            );
            lots.push(lot_rect.iter().collect());
            streets.push((culture, ring, Rect2D::from_points(road.min(), lot_rect.max()), ground));

            // Houses along the road, each its ring's gap from the next.
            let roofs = culture.roof_styles_for(SizeClass::House);
            let mut x = x_lo;
            for i in 0..count {
                let fw = rng.rand_i32_range(7, MAX_FRONT + 1);
                let d = rng.rand_i32_range(7, MAX_DEPTH + 1);
                if x + fw > x_hi + 1 {
                    break;
                }
                let rect = Rect2D::from_points(Point2D::new(x, front_z), Point2D::new(x + fw - 1, front_z + d - 1));
                let house_seed = HouseSeed(rng.next_i64());
                let roof = roofs[(i as usize) % roofs.len()];
                let mut bctx = house_context(culture, SizeClass::House, roof, house_seed);
                bctx.back_door = true;
                let footprint = Footprint::new(outline_from_rects(&[rect]), vec![rect]);
                let mut caller_rng = RNG::new(house_seed.0);
                // Medieval roofs cycle through the town roof palettes whose
                // trim (secondary roof) stands out from the roof, so the roof
                // trim shows; the base palette's blackstone trim barely does.
                let house_palette = match culture {
                    Culture::Medieval => {
                        let id = TRIM_ROOFS[(street * count as usize + i as usize) % TRIM_ROOFS.len()];
                        match data.palettes.get(&id.into()) {
                            Some(roof) => palette.clone().merged_with(roof),
                            None => palette.clone(),
                        }
                    }
                    _ => palette.clone(),
                };
                let mut ctx = BuildCtx::new(&mut editor, &data, &house_palette, &mut caller_rng);
                match build_house(&mut ctx, footprint, &bctx, synthetic_plot_bounds(&rect, Cardinal::North)).await {
                    Ok(out) => {
                        let (ramp_cells, entrances) = super::plots::door_cells(&out);
                        let gap = ring.side_gap();
                        let front: Vec<Point2D> = (x..(x + fw + gap).min(x_hi + 1)).map(|cx| Point2D::new(cx, front_z)).collect();
                        houses.push(PlacedHouse {
                            front,
                            width: fw as usize,
                            depth: d,
                            outward: Cardinal::North,
                            building: out.building_id,
                            ring,
                            lot: street,
                            ramp_cells,
                            entrances,
                        });
                    }
                    Err(e) => println!("{culture:?} {ring:?} house {i}: build failed, skipped: {e}"),
                }
                x += fw + ring.side_gap();
            }
            z0 += street_depth(ring);
        }
    }

    // Gardens, exactly as a town plans and furnishes them, layouts cycled per street.
    let gardens = plan_gardens(editor.world(), &houses, &lots);
    for g in &gardens {
        for &c in &g.cells {
            editor.world_mut().claim(c, BuildClaim::Building(g.building));
        }
    }
    let owners = garden_owners(&gardens);
    let fences = super::plots::plan_fences(&editor, &gardens, &owners, &houses);
    let mut garden_rng = RNG::new(seed).derive();
    let mut per_street = vec![0usize; streets.len()];
    for (i, g) in gardens.iter().enumerate() {
        let street = houses[g.house].lot;
        let theme = Theme::for_culture(streets[street].0);
        let style = GardenStyle::for_culture(streets[street].0)?;
        let layouts = style.layout_list();
        let layout = layouts[per_street[street] % layouts.len()];
        per_street[street] += 1;
        // Every third garden walled, so both boundaries show in each street.
        let walled = per_street[street] % 3 == 0;
        let (built, walled) = furnish_garden(
            &editor, i, &g.cells, g.building, g.into, &g.gates, &g.doors, &owners, &fences, &theme, &style, Some(layout), 0, Some(walled), &mut garden_rng,
        )
        .await;
        let wall = if walled { ", walled" } else { "" };
        println!("{:?} {:?} street, house {}: {built:?}{wall}, {} cells", streets[street].0, g.ring, g.house, g.cells.len());
    }
    editor.flush_buffer().await;

    let origin = editor.world().build_area.origin;
    // Where each street is in the world, and a spot south of it, above the
    // gardens, looking back north over them at the houses.
    for &(culture, ring, r, ground) in &streets {
        let x = r.min().x + (r.max().x - r.min().x) / 3 + origin.x;
        let z = r.max().y + 6 + origin.z;
        println!(
            "{culture:?} {ring:?} street: x {}..{}, z {}..{}  /tp @s {x} {} {z} 180 35",
            r.min().x + origin.x, r.max().x + origin.x, r.min().y + origin.z, r.max().y + origin.z, ground + origin.y + 9,
        );
    }
    if live {
        if let Some(&(_, _, r, ground)) = streets.first() {
            let (x, z) = (r.min().x + 12 + origin.x, r.max().y + 6 + origin.z);
            let _ = provider.command(vec![format!("tp @a {x} {} {z} 160 35", ground + origin.y + 9)]).await;
        }
    }

    let tag = if cultures.len() == 1 { format!("{:?}", cultures[0]) } else { "all".into() };
    let out_dir = PathBuf::from("output/gardens").join(format!("{tag}-{seed}{}", if live { "-live" } else { "" }).to_lowercase());
    std::fs::create_dir_all(&out_dir)?;
    let scene = editor.capture_scene();
    scene.save(out_dir.join("gardens.scene"))?;
    let lo = streets.iter().map(|s| s.3).min().unwrap_or(GROUND_Y);
    let hi = streets.iter().map(|s| s.3).max().unwrap_or(GROUND_Y);
    let grid = Rect3D::from_points(
        Point3D::new(origin.x + at.x, lo + origin.y - 3, origin.z + at.y),
        Point3D::new(origin.x + at.x + width - 1, hi + origin.y + 40, origin.z + at.y + depth - 1),
    );
    render_views(&scene, &RenderOptions { px: 8, iso_tile: 16, region: Some(grid), ..RenderOptions::default() }, &out_dir, "")?;
    for &(culture, ring, r, ground) in &streets {
        let region = Rect3D::from_points(
            Point3D::new(r.min().x + origin.x, ground + origin.y - 3, r.min().y + origin.z),
            Point3D::new(r.max().x + origin.x, ground + origin.y + 40, r.max().y + origin.z),
        );
        let opts = RenderOptions { px: 12, iso_tile: 24, region: Some(region), ..RenderOptions::default() };
        let prefix = if cultures.len() == 1 { format!("{ring:?}_") } else { format!("{culture:?}_{ring:?}_") };
        render_views(&scene, &opts, &out_dir, &prefix.to_lowercase())?;
    }
    println!("Gardens: {} of {} houses; output in {}", gardens.len(), houses.len(), out_dir.display());
    Ok(out_dir)
}
