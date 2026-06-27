//! Ship tests. The offline build uses a synthetic
//! water world + offline editor, no server. The live `build_ship` places keels
//! into the running server's build area for the screenshot loop.

use crate::generator::ships::keel;

/// Scan small hulls for watertightness leaks (the stern-hole bug). Pure geometry — no
/// server. Flood-fills the below-waterline band from outside and reports any interior
/// cell it reaches (a side/bottom hole). Run: `cargo test hull_watertight_scan -- --nocapture`
#[test]
fn hull_watertight_scan() {
    use crate::generator::ships::hull::{build_hull_model, HullShape};
    use crate::generator::ships::keel::build_keel_model;
    use std::collections::HashSet;

    for length in 14..=22 {
        for shape in [HullShape::Teardrop, HullShape::Oval] {
            let keel = build_keel_model(length);
            let hull = build_hull_model(length, keel.depth, 2.7, shape, &keel.top_profile());
            let mut solid: HashSet<(i32, i32, i32)> = HashSet::new();
            for c in &hull.cells { solid.insert((c.x, c.y, c.z)); }
            for b in &hull.bevel { solid.insert((b.local.x, b.local.y, b.local.z)); }
            for c in &keel.cells { solid.insert((c.local.x, c.local.y, c.local.z)); }
            let interior: HashSet<(i32, i32, i32)> = hull.interior.iter().map(|c| (c.x, c.y, c.z)).collect();

            let (xlo, xhi) = (-2, length + 1);
            let mb = hull.max_beam + 2;
            let (ylo, yhi) = (1, keel.depth - 1); // strictly below the waterline rim
            if yhi < ylo { continue; }
            let air = |p: (i32, i32, i32)| !solid.contains(&p);
            let mut seen: HashSet<(i32, i32, i32)> = HashSet::new();
            let mut stack: Vec<(i32, i32, i32)> = Vec::new();
            // Seed from the bounding-box shell (exterior), in the underwater band.
            for y in ylo..=yhi {
                for x in xlo..=xhi {
                    for &z in &[-mb, mb] { if air((x, y, z)) { stack.push((x, y, z)); } }
                }
            }
            let mut leaks: Vec<(i32, i32, i32)> = Vec::new();
            while let Some(p) = stack.pop() {
                if !seen.insert(p) { continue; }
                if interior.contains(&p) { leaks.push(p); }
                let (x, y, z) = p;
                for d in [(-1,0,0),(1,0,0),(0,-1,0),(0,1,0),(0,0,-1),(0,0,1)] {
                    let n = (x + d.0, y + d.1, z + d.2);
                    if n.0 < xlo || n.0 > xhi || n.1 < ylo || n.1 > yhi || n.2 < -mb || n.2 > mb { continue; }
                    if air(n) && !seen.contains(&n) { stack.push(n); }
                }
            }
            leaks.sort();
            assert!(
                leaks.is_empty(),
                "hull leak len={length} {shape:?} depth={}: {} interior cells reachable from outside below the waterline, e.g. {:?}",
                keel.depth, leaks.len(), &leaks[..leaks.len().min(8)],
            );
        }
    }
}

/// One-off analysis of the user's hand-fixed bowsprit/prow structure blocks, to
/// reverse-engineer the pattern. Run with:
/// `cargo test analyze_bowsprit_nbt -- --nocapture --ignored`
#[test]
#[ignore]
fn analyze_bowsprit_nbt() {
    use fastnbt::Value;
    use std::collections::HashMap;
    use std::io::Read;

    fn read_nbt(path: &str) -> Value {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
        let mut buf = Vec::new();
        flate2::read::GzDecoder::new(bytes.as_slice())
            .read_to_end(&mut buf)
            .expect("gunzip");
        fastnbt::from_bytes(&buf).expect("parse nbt")
    }
    fn as_i32(v: &Value) -> i32 {
        match v {
            Value::Byte(b) => *b as i32,
            Value::Short(s) => *s as i32,
            Value::Int(i) => *i,
            Value::Long(l) => *l as i32,
            _ => panic!("not int: {v:?}"),
        }
    }
    fn comp(v: &Value) -> &HashMap<String, Value> {
        match v {
            Value::Compound(c) => c,
            _ => panic!("not compound"),
        }
    }
    fn list(v: &Value) -> &Vec<Value> {
        match v {
            Value::List(l) => l,
            _ => panic!("not list"),
        }
    }
    fn s(v: &Value) -> &str {
        match v {
            Value::String(s) => s,
            _ => panic!("not string"),
        }
    }

    fn dump(path: &str) {
        let root = read_nbt(path);
        let c = comp(&root);
        let size = list(&c["size"]);
        let (sx, sy, sz) = (as_i32(&size[0]), as_i32(&size[1]), as_i32(&size[2]));
        let palette = if let Some(p) = c.get("palette") {
            list(p).clone()
        } else {
            list(&list(&c["palettes"])[0]).clone()
        };
        let legend: Vec<(String, String)> = palette
            .iter()
            .map(|e| {
                let ec = comp(e);
                let nm = s(&ec["Name"]).replace("minecraft:", "");
                let props = ec
                    .get("Properties")
                    .map(|p| {
                        let mut kv: Vec<String> =
                            comp(p).iter().map(|(k, v)| format!("{k}={}", s(v))).collect();
                        kv.sort();
                        kv.join(",")
                    })
                    .unwrap_or_default();
                (nm, props)
            })
            .collect();
        // form char per palette index
        let charof = |i: usize| -> char {
            let nm = &legend[i].0;
            if nm == "air" || nm == "structure_void" || nm == "cave_air" {
                ' '
            } else if nm.contains("stairs") {
                '/'
            } else if nm.contains("slab") {
                '-'
            } else if nm.contains("fence") || nm.contains("wall") {
                '|'
            } else if nm.contains("log") {
                'O'
            } else {
                '#'
            }
        };
        let mut occ: HashMap<(i32, i32, i32), usize> = HashMap::new();
        for b in list(&c["blocks"]) {
            let bc = comp(b);
            let st = as_i32(&bc["state"]) as usize;
            let pos = list(&bc["pos"]);
            occ.insert((as_i32(&pos[0]), as_i32(&pos[1]), as_i32(&pos[2])), st);
        }
        let is_solid = |st: usize| charof(st) != ' ';

        println!("\n================ {path}");
        println!("size = {sx} x {sy} x {sz}  (placed blocks = {})", occ.len());
        for (i, (nm, pr)) in legend.iter().enumerate() {
            println!("  [{i:2}] {} {nm}{}", charof(i), if pr.is_empty() { String::new() } else { format!("  [{pr}]") });
        }

        // Top-down (collapse Y): X across, Z down.
        println!("-- top-down (X→, Z↓), any block --");
        for z in 0..sz {
            let mut row = String::new();
            for x in 0..sx {
                let any = (0..sy).any(|y| occ.get(&(x, y, z)).map_or(false, |&st| is_solid(st)));
                row.push(if any { '#' } else { '.' });
            }
            println!("  z{z:2} {row}");
        }

        // Side profile per Z slice: X across, Y up (row 0 = top).
        for z in 0..sz {
            let has = (0..sx).any(|x| (0..sy).any(|y| occ.get(&(x, y, z)).map_or(false, |&st| is_solid(st))));
            if !has {
                continue;
            }
            println!("-- side z={z} (X→, Y↑) --");
            for y in (0..sy).rev() {
                let mut row = String::new();
                for x in 0..sx {
                    row.push(occ.get(&(x, y, z)).map_or('.', |&st| charof(st)));
                }
                println!("  y{y:2} {row}");
            }
        }
    }

    let dir = "C:/Users/timdo/AppData/Roaming/ModrinthApp/profiles/GDMC 2026/saves/Test flatworld/generated/minecraft/structures";
    dump(&format!("{dir}/small_ship_bowsprit_before.nbt"));
    dump(&format!("{dir}/small_ship_bowsprit_after.nbt"));
    dump(&format!("{dir}/large_ship_bowsprit_before.nbt"));
    dump(&format!("{dir}/large_ship_bowsprit_after.nbt"));
}

/// Every masthead-flag block must be connected: it always has another flag block within
/// a 3×3×3 neighbourhood (Chebyshev distance 1) — no floating wool. Swept across mast
/// counts, lengths and ripple phases.
#[test]
fn masthead_flags_have_no_floating_blocks() {
    use crate::generator::ships::additions::masts::build_masts_model;
    use std::collections::HashSet;

    let mut rng = crate::noise::RNG::new(11);
    for length in [14, 20, 26, 32, 40, 46] {
        for count in [1, 2, 3] {
            for _ in 0..12 {
                let phase = (rng.rand_i32_range(0, 360) as f32).to_radians();
                let seed = rng.rand_i32_range(0, 1000);
                let sign = if rng.rand_i32_range(0, 2) == 0 { -1 } else { 1 }; // aft / forward
                let model = build_masts_model(
                    length, count, 0.0, 0, length / 4, false, false, phase, seed, sign, 4,
                );
                for mast in &model.masts {
                    let occ: HashSet<(i32, i32, i32)> =
                        mast.flag.cells.iter().map(|c| (c.x, c.y, c.z)).collect();
                    assert!(!occ.is_empty(), "len={length} count={count}: empty flag");
                    for &(x, y, z) in &occ {
                        let connected = (-1..=1).any(|dx| {
                            (-1..=1).any(|dy| {
                                (-1..=1).any(|dz| {
                                    (dx, dy, dz) != (0, 0, 0)
                                        && occ.contains(&(x + dx, y + dy, z + dz))
                                })
                            })
                        });
                        assert!(
                            connected,
                            "floating flag block at ({x},{y},{z}) len={length} count={count} phase={phase} seed={seed}",
                        );
                    }
                }
            }
        }
    }
}

/// Diagnostic: print, per ship length, the weather-deck Y, railing cap top, lowest-sail
/// foot, and the resulting clearance — so the 2–3-block rule can be verified across sizes.
/// Run: `cargo test sail_clearance_diagnostic -- --nocapture --ignored`
#[tokio::test]
#[ignore]
async fn sail_clearance_diagnostic() {
    use crate::editor::World;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships::{build_ship, ShipCtx, ShipSpec};
    use crate::geometry::{Cardinal, Point2D, Point3D, Rect3D};
    use crate::noise::RNG;

    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
    let data = LoadedData::load().expect("data");
    let palette = data.palettes.get(&PaletteId::from("ship_oak")).expect("ship_oak").clone();

    for length in [14, 20, 26, 32, 38, 44] {
        let world = World::synthetic_water(build_area, 50, 64);
        let mut editor = world.get_offline_editor();
        let mut rng = RNG::new(3);
        let spec = ShipSpec::new(Cardinal::North, length);
        let mut ctx = ShipCtx::new(&mut editor, &data, &palette, &mut rng);
        let ship = build_ship(&mut ctx, &spec, Point2D::new(96, 96)).await;
        let wd = ship.weather_deck_y;
        let rail_top = ship
            .railing
            .as_ref()
            .and_then(|r| r.cap.iter().map(|c| c.y).max());
        let masts = ship.masts.as_ref().unwrap();
        let m = &masts.masts[0];
        let bottom = m.yards.last().unwrap();
        let foot = bottom.y - bottom.sail_height;
        println!(
            "len={length:2} tier={:?} deck_y={} weather_y={wd} rail_top={:?} | lowest yard_y={} sail_h={} foot={foot} clearance={} | yards={}",
            ship.tier, ship.deck.deck_y, rail_top, bottom.y, bottom.sail_height,
            foot - wd, m.yards.len(),
        );
    }
}

/// On every mast the **lowest sail is the largest** — its yard carries the tallest sail
/// height (and widest yard) of the stack, shrinking going up. Swept across sizes.
#[test]
fn square_sails_largest_at_bottom() {
    use crate::generator::ships::additions::masts::build_masts_model;

    for length in [20, 26, 32, 40, 46] {
        for count in [1, 2, 3] {
            let model = build_masts_model(length, count, 0.0, 0, 5, false, false, 0.0, 0, 1, 4);
            for mast in &model.masts {
                if mast.yards.len() < 2 {
                    continue;
                }
                // Yards are ordered top→bottom, so the last is the bottom (course).
                let bottom = mast.yards.last().unwrap();
                for y in &mast.yards {
                    assert!(
                        bottom.sail_height >= y.sail_height,
                        "len={length} count={count}: bottom sail ({}) shorter than an upper sail ({}) at y={}",
                        bottom.sail_height, y.sail_height, y.y,
                    );
                    assert!(
                        bottom.half_width >= y.half_width,
                        "len={length} count={count}: bottom yard ({}) narrower than an upper yard ({})",
                        bottom.half_width, y.half_width,
                    );
                }
            }
        }
    }
}

/// A deployed square sail's billow surface has **no holes**: the bulge field never steps
/// more than 1 between neighbouring cells, so every placed block has a grid neighbour within
/// its 3×3 (no see-through gaps). Swept across width, drop and wind strength.
#[test]
fn square_sail_surface_has_no_holes() {
    use crate::generator::ships::additions::masts::billow_field;
    use crate::generator::ships::SailBillow;

    for shape in [SailBillow::Domed, SailBillow::Curtain, SailBillow::Combined] {
        for hw in 2..=9 {
            for drop in 2..=16 {
                for wind10 in [0, 15, 20, 30, 40, 60] {
                    let wind = wind10 as f32 / 10.0;
                    let (b, ny) = billow_field(hw, 0, drop, wind, shape);
                    let nz = (2 * hw + 1) as usize;
                    let at = |zi: usize, yi: usize| zi * ny + yi;
                    for zi in 0..nz {
                        for yi in 0..ny {
                            if zi + 1 < nz {
                                assert!(
                                    (b[at(zi, yi)] - b[at(zi + 1, yi)]).abs() <= 1,
                                    "z-step >1 at {shape:?} hw={hw} drop={drop} wind={wind}",
                                );
                            }
                            if yi + 1 < ny {
                                assert!(
                                    (b[at(zi, yi)] - b[at(zi, yi + 1)]).abs() <= 1,
                                    "y-step >1 at {shape:?} hw={hw} drop={drop} wind={wind}",
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The deployed spanker (aft fore-and-aft sail) is bent to the boom + gaff and bellies
/// sideways with **no floating blocks** — every placed canvas block has a neighbour within
/// its 3×3×3. Swept across sizes (spanker forced on).
#[test]
fn spanker_sail_has_no_holes() {
    use crate::generator::ships::additions::masts::{build_masts_model, spanker_billow};
    use std::collections::HashSet;

    for length in [26, 32, 38, 44] {
        for count in [2, 3] {
            // spanker = true, so the aftmost mast carries one.
            let model = build_masts_model(length, count, 0.0, 0, 5, true, false, 0.0, 0, 1, 4);
            for mast in &model.masts {
                let Some(sp) = &mast.spanker else { continue };
                assert!(!sp.sail.is_empty(), "len={length}: empty spanker sail");
                let spars: HashSet<(i32, i32)> = sp
                    .boom
                    .iter()
                    .chain(sp.gaff.iter())
                    .map(|c| (c.local.x, c.local.y))
                    .collect();
                let depth = spanker_billow(&sp.sail, 3.0, &spars);
                let placed: HashSet<(i32, i32, i32)> = sp
                    .sail
                    .iter()
                    .filter(|c| !spars.contains(&(c.x, c.y)))
                    .map(|c| (c.x, c.y, depth[&(c.x, c.y)]))
                    .collect();
                for &(x, y, z) in &placed {
                    let connected = (-1..=1).any(|dx| {
                        (-1..=1).any(|dy| {
                            (-1..=1).any(|dz| {
                                (dx, dy, dz) != (0, 0, 0) && placed.contains(&(x + dx, y + dy, z + dz))
                            })
                        })
                    });
                    assert!(
                        connected,
                        "floating spanker cell at ({x},{y},{z}) len={length} count={count}",
                    );
                }
            }
        }
    }
}

/// End-to-end: a Huge ship (jib chance 100%) actually **places the jib** — its rigging line
/// (forestay + hangers) and white-wool (sail) blocks land in the world. Runs once with the
/// rigging **forced to chain** and once **forced to fence**, asserting the chosen material
/// shows up (and the other doesn't), so the `RiggingMaterial` option is honoured.
#[tokio::test]
async fn jib_places_rigging_and_wool() {
    use crate::editor::World;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships::{build_ship, RiggingMaterial, ShipCtx, ShipSpec};
    use crate::geometry::{Cardinal, Point2D, Point3D, Rect3D};
    use crate::noise::RNG;

    // Scan the build volume for rigging (chain / fence) + white_wool, building a Huge ship
    // with the rigging material forced.
    async fn scan(rigging: RiggingMaterial) -> (i32, i32, i32) {
        let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
        let data = LoadedData::load().expect("data");
        let palette = data.palettes.get(&PaletteId::from("ship_oak")).expect("ship_oak").clone();

        let world = World::synthetic_water(build_area, 50, 64);
        let mut editor = world.get_offline_editor();
        let mut rng = RNG::new(3);
        let spec = ShipSpec::new(Cardinal::North, 46).with_rigging(rigging); // Huge → jib always
        let mut ctx = ShipCtx::new(&mut editor, &data, &palette, &mut rng);
        let _ship = build_ship(&mut ctx, &spec, Point2D::new(96, 96)).await;
        editor.flush_buffer().await;

        let (mut chains, mut fences, mut wool) = (0, 0, 0);
        for x in 0..180 {
            for y in 40..110 {
                for z in 0..180 {
                    if let Some(b) = editor.try_get_block(Point3D::new(x, y, z)) {
                        let id = b.id.as_str();
                        if id.contains("chain") {
                            chains += 1;
                        } else if id.contains("fence") {
                            fences += 1;
                        } else if id.contains("white_wool") {
                            wool += 1;
                        }
                    }
                }
            }
        }
        (chains, fences, wool)
    }

    let (chains, _fences, wool) = scan(RiggingMaterial::Chain).await;
    println!("jib scan (chain): chains={chains}, white_wool={wool}");
    assert!(chains > 0, "chain rigging: expected forestay/hanger chain blocks");
    assert!(wool > 0, "chain rigging: expected jib/sail wool blocks");

    // With fence rigging the jib's lines become fences. (Other fences exist — railing, mast
    // finials — so we only assert wool is present and the forestay didn't fall back to chain
    // for the *jib*; a clean count of jib-only fences isn't separable here, so we assert no
    // chain blocks anywhere, proving the jib used fence not chain.)
    let (chains_f, fences_f, wool_f) = scan(RiggingMaterial::Fence).await;
    println!("jib scan (fence): chains={chains_f}, fences={fences_f}, white_wool={wool_f}");
    assert_eq!(chains_f, 0, "fence rigging: no chain blocks should be placed");
    assert!(fences_f > 0, "fence rigging: expected fence blocks (rigging + railing)");
    assert!(wool_f > 0, "fence rigging: expected jib/sail wool blocks");
}

/// Under a **furled / struck** sail state the jib shows **only its rigging** (the forestay stay) —
/// no canvas. Builds the same Huge ship `Full` vs `Furled` (chain rigging forced) and asserts the
/// stay rigging is still placed while the canvas wool collapses to near nothing.
#[tokio::test]
async fn jib_furled_is_rigging_only() {
    use crate::editor::World;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships::{build_ship, RiggingMaterial, SailState, ShipCtx, ShipSpec};
    use crate::geometry::{Cardinal, Point2D, Point3D, Rect3D};
    use crate::noise::RNG;

    async fn scan(state: SailState) -> (i32, i32) {
        let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
        let data = LoadedData::load().expect("data");
        let palette = data.palettes.get(&PaletteId::from("ship_oak")).expect("ship_oak").clone();
        let world = World::synthetic_water(build_area, 50, 64);
        let mut editor = world.get_offline_editor();
        let mut rng = RNG::new(3);
        let spec = ShipSpec::new(Cardinal::North, 46)
            .with_rigging(RiggingMaterial::Chain)
            .with_sail_state(state);
        let mut ctx = ShipCtx::new(&mut editor, &data, &palette, &mut rng);
        let _ship = build_ship(&mut ctx, &spec, Point2D::new(96, 96)).await;
        editor.flush_buffer().await;
        let (mut chains, mut wool) = (0, 0);
        for x in 0..180 {
            for y in 40..110 {
                for z in 0..180 {
                    if let Some(b) = editor.try_get_block(Point3D::new(x, y, z)) {
                        let id = b.id.as_str();
                        if id.contains("chain") {
                            chains += 1;
                        } else if id.contains("white_wool") {
                            wool += 1;
                        }
                    }
                }
            }
        }
        (chains, wool)
    }

    let (full_ch, full_wool) = scan(SailState::Full).await;
    let (furl_ch, furl_wool) = scan(SailState::Furled).await;
    println!("jib full: chains={full_ch}, wool={full_wool} | furled: chains={furl_ch}, wool={furl_wool}");
    assert!(furl_ch > 0, "furled jib should still place its forestay stay rigging");
    // Canvas is gone under furled — only a stray white flag could remain, far below the set sail.
    assert!(
        furl_wool * 5 < full_wool,
        "furled should have far less wool than full (no jib canvas): full={full_wool} furled={furl_wool}",
    );
}

/// On a 2+ mast ship, a **mast-to-mast stay** (rigging) connects the mastheads — assert a chain
/// lands near the midpoint between the two forward mastheads (well above the deck).
#[tokio::test]
async fn mast_stays_connect_tops() {
    use crate::editor::World;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships::{build_ship, RiggingMaterial, ShipCtx, ShipSpec};
    use crate::geometry::{Cardinal, Point2D, Point3D, Rect3D};
    use crate::noise::RNG;

    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
    let data = LoadedData::load().expect("data");
    let palette = data.palettes.get(&PaletteId::from("ship_oak")).expect("ship_oak").clone();
    let world = World::synthetic_water(build_area, 50, 64);
    let mut editor = world.get_offline_editor();
    let mut rng = RNG::new(7);
    let spec = ShipSpec::new(Cardinal::North, 36).with_rigging(RiggingMaterial::Chain); // Large → 3 masts
    let mut ctx = ShipCtx::new(&mut editor, &data, &palette, &mut rng);
    let ship = build_ship(&mut ctx, &spec, Point2D::new(96, 96)).await;
    editor.flush_buffer().await;

    let masts = ship.masts.as_ref().expect("masts");
    assert!(masts.masts.len() >= 2, "need 2+ masts for a stay");
    let mut tops: Vec<Point3D> = masts
        .masts
        .iter()
        .map(|m| m.cells.iter().max_by_key(|p| p.y).copied().unwrap())
        .collect();
    tops.sort_by_key(|p| p.x);
    let (m1, m2) = (tops[0], tops[1]);
    let (midx, midy) = ((m1.x + m2.x) / 2, (m1.y + m2.y) / 2);
    // The cache is keyed by the local point passed to `place_block` (= `placement.to_world(local)`),
    // so read it back the same way.
    let mut found = false;
    for dx in -1..=1 {
        for dy in -2..=2 {
            let local = Point3D::new(midx + dx, midy + dy, 0);
            if let Some(b) = editor.get_cached_block(ship.placement.to_world(local)) {
                if b.id.as_str().contains("chain") {
                    found = true;
                }
            }
        }
    }
    assert!(found, "expected a mast-to-mast stay chain near the midpoint between the two mastheads");
}

/// The helm (ship's wheel) places its lectern base + trapdoor wheel on the deck.
#[tokio::test]
async fn helm_places_wheel() {
    use crate::editor::World;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships::{build_ship, ShipCtx, ShipSpec};
    use crate::geometry::{Cardinal, Point2D, Point3D, Rect3D};
    use crate::noise::RNG;

    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
    let data = LoadedData::load().expect("data");
    let palette = data.palettes.get(&PaletteId::from("ship_oak")).expect("ship_oak").clone();
    let world = World::synthetic_water(build_area, 50, 64);
    let mut editor = world.get_offline_editor();
    let mut rng = RNG::new(5);
    let spec = ShipSpec::new(Cardinal::North, 32);
    let mut ctx = ShipCtx::new(&mut editor, &data, &palette, &mut rng);
    let _ship = build_ship(&mut ctx, &spec, Point2D::new(96, 96)).await;
    editor.flush_buffer().await;

    let (mut lecterns, mut trapdoors) = (0, 0);
    for x in 0..180 {
        for y in 40..127 {
            for z in 0..180 {
                if let Some(b) = editor.try_get_block(Point3D::new(x, y, z)) {
                    let id = b.id.as_str();
                    if id.contains("lectern") {
                        lecterns += 1;
                    } else if id.contains("trapdoor") {
                        trapdoors += 1;
                    }
                }
            }
        }
    }
    println!("helm scan: lecterns={lecterns}, trapdoors={trapdoors}");
    assert!(lecterns > 0, "expected a lectern (helm base)");
    assert!(trapdoors > 0, "expected a trapdoor (helm wheel)");
}

/// The jib's billowed triangle has **no holes**: the depth field never steps more than 1
/// Stage 3: companionways connect the levels — a Medium+ ship records hatch/stair cells and cuts
/// at least one hole (air) in a deck layer.
#[tokio::test]
async fn companionways_connect_levels() {
    use crate::editor::World;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships::{build_ship, ShipCtx, ShipSpec};
    use crate::geometry::{Cardinal, Point2D, Point3D, Rect3D};
    use crate::noise::RNG;

    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
    let data = LoadedData::load().expect("data");
    let palette = data.palettes.get(&PaletteId::from("ship_oak")).expect("ship_oak").clone();
    let world = World::synthetic_water(build_area, 50, 64);
    let mut editor = world.get_offline_editor();
    let mut rng = RNG::new(4);
    let spec = ShipSpec::new(Cardinal::North, 36); // Large → hold + gun deck
    let mut ctx = ShipCtx::new(&mut editor, &data, &palette, &mut rng);
    let ship = build_ship(&mut ctx, &spec, Point2D::new(96, 96)).await;
    editor.flush_buffer().await;

    assert!(!ship.hatch_cells.is_empty(), "companionways should record hatch/stair cells");
    // A below-decks hatch is an open hole (air); the weather-deck hatch is an openable trapdoor lid.
    // (Recorded cells are the x/z footprint at `floor_y`; the ceiling cells carry the hole/lid.)
    let (mut holes, mut trapdoors) = (0, 0);
    for cell in &ship.hatch_cells {
        if let Some(b) = editor.try_get_block(ship.placement.to_world(*cell)) {
            let id = b.id.as_str();
            if id.contains("air") {
                holes += 1;
            } else if id.contains("trapdoor") {
                trapdoors += 1;
            }
        }
    }
    assert!(holes > 0, "expected an open below-decks hatch (air)");
    assert!(trapdoors > 0, "expected an openable trapdoor lid over the weather-deck hatch");
}

/// Stage 3: the interior levels get **furnished** — a Large ship places furniture (barrels/chests/
/// etc.) below decks, reusing the buildings_v2 furnishing engine.
#[tokio::test]
async fn ship_interior_furnished() {
    use crate::editor::World;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships::{build_ship, ShipCtx, ShipSpec};
    use crate::geometry::{Cardinal, Point2D, Point3D, Rect3D};
    use crate::noise::RNG;

    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
    let data = LoadedData::load().expect("data");
    let palette = data.palettes.get(&PaletteId::from("ship_oak")).expect("ship_oak").clone();
    let world = World::synthetic_water(build_area, 50, 64);
    let mut editor = world.get_offline_editor();
    let mut rng = RNG::new(8);
    let spec = ShipSpec::new(Cardinal::North, 38); // Large → hold + gun deck
    let mut ctx = ShipCtx::new(&mut editor, &data, &palette, &mut rng);
    let _ship = build_ship(&mut ctx, &spec, Point2D::new(96, 96)).await;
    editor.flush_buffer().await;

    let (mut cargo, mut beds) = (0, 0);
    for x in 0..180 {
        for y in 40..90 {
            for z in 0..180 {
                if let Some(b) = editor.try_get_block(Point3D::new(x, y, z)) {
                    let id = b.id.as_str();
                    if id.contains("barrel") || id.ends_with("chest") || id.contains("hay") {
                        cargo += 1;
                    } else if id.contains("bed") {
                        beds += 1;
                    }
                }
            }
        }
    }
    println!("ship interior: cargo={cargo}, beds={beds}");
    assert!(cargo > 0, "expected cargo (barrels/chests) in the holds");
    // The gun deck is bulkheaded into cabins — the captain/crew cabins place beds.
    assert!(beds > 0, "expected a bed from the bulkheaded gun-deck cabins");
}

/// For fun: a **giant** ship — keel length 100 — builds end to end without panicking, and everything
/// scales off the length (hull/masts/levels/etc. are all length-derived). Just a smoke test.
#[tokio::test]
async fn giant_ship_length_100() {
    use crate::editor::World;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships::{build_ship, ShipCtx, ShipSpec};
    use crate::geometry::{Cardinal, Point2D, Point3D, Rect3D};
    use crate::noise::RNG;

    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(383, 255, 383));
    let data = LoadedData::load().expect("data");
    let palette = data.palettes.get(&PaletteId::from("ship_oak")).expect("ship_oak").clone();
    let world = World::synthetic_water(build_area, 60, 120);
    let mut editor = world.get_offline_editor();
    let mut rng = RNG::new(100);
    let spec = ShipSpec::new(Cardinal::North, 100);
    let mut ctx = ShipCtx::new(&mut editor, &data, &palette, &mut rng);
    let ship = build_ship(&mut ctx, &spec, Point2D::new(190, 190)).await;
    editor.flush_buffer().await;

    // Everything scaled off the length: a deep hull, tall masts, multiple levels.
    assert!(ship.keel.depth >= 12, "len-100 keel should be deep, got {}", ship.keel.depth);
    assert!(ship.hull.max_beam >= 25, "len-100 beam should be wide, got {}", ship.hull.max_beam);
    let masts = ship.masts.as_ref().expect("masts");
    let tallest = masts.masts.iter().map(|m| m.height).max().unwrap_or(0);
    assert!(tallest >= 80, "len-100 mainmast should be tall, got {tallest}");
    assert!(ship.levels.levels.iter().any(|l| l.name == "hold"), "len-100 should have a hold");
    // Deep hull → stacked lower holds reached by mast ladders.
    let lower = ship.levels.levels.iter().filter(|l| l.name == "lower_hold").count();
    assert!(lower >= 1, "len-100 deep hull should stack lower holds, got {lower}");
    let mut ladders = 0;
    for x in 0..256 {
        for y in 60..130 {
            for z in 0..256 {
                if let Some(b) = editor.try_get_block(Point3D::new(x, y, z)) {
                    if b.id.as_str().contains("ladder") {
                        ladders += 1;
                    }
                }
            }
        }
    }
    assert!(ladders > 0, "len-100 should place mast ladders to the lower holds");
    println!(
        "giant ship: depth={}, beam={}, mast={}, levels={} (lower_holds={lower}), ladders={ladders}",
        ship.keel.depth, ship.hull.max_beam, tallest, ship.levels.levels.len()
    );
}

/// Stage 3: the interior **levels** model is well-formed across sizes — every level has positive
/// headroom and a non-empty footprint; Medium+ ships (with a raised additional deck) get a gun deck
/// on top of the hold. Writes a per-level ASCII dump to `output/ships/levels.txt`.
#[tokio::test]
async fn ship_levels_offline() {
    use crate::editor::World;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships::levels::render_levels_ascii;
    use crate::generator::ships::{build_ship, ShipCtx, ShipSpec};
    use crate::geometry::{Cardinal, Point2D, Point3D, Rect3D};
    use crate::noise::RNG;

    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
    let data = LoadedData::load().expect("data");
    let palette = data.palettes.get(&PaletteId::from("ship_oak")).expect("ship_oak").clone();

    let mut dump = String::new();
    for length in [16, 26, 36, 44] {
        let world = World::synthetic_water(build_area, 50, 64);
        let mut editor = world.get_offline_editor();
        let mut rng = RNG::new(7);
        let spec = ShipSpec::new(Cardinal::North, length);
        let mut ctx = ShipCtx::new(&mut editor, &data, &palette, &mut rng);
        let ship = build_ship(&mut ctx, &spec, Point2D::new(96, 96)).await;

        // Every level: positive headroom, non-empty footprint, floor below ceiling.
        for lvl in &ship.levels.levels {
            assert!(lvl.headroom() >= 2, "len={length} {} headroom {}", lvl.name, lvl.headroom());
            assert!(lvl.floor_y < lvl.ceiling_y, "len={length} {} floor>=ceiling", lvl.name);
            assert!(lvl.outline.iter().any(|&h| h >= 1), "len={length} {} empty footprint", lvl.name);
        }
        // A hold appears once the hull is deep enough for headroom below the deck; Medium+ (raised
        // additional deck) also gets a gun deck.
        let has_hold = ship.levels.levels.iter().any(|l| l.name == "hold");
        if ship.deck.deck_y - 2 >= 2 {
            assert!(has_hold, "len={length} deep enough but no hold (deck_y={})", ship.deck.deck_y);
        }
        if ship.weather_deck_y > ship.deck.deck_y {
            assert!(ship.levels.levels.iter().any(|l| l.name == "gun_deck"), "len={length} no gun deck");
        }

        dump.push_str(&format!("==== length {length} ====\n"));
        dump.push_str(&render_levels_ascii(&ship.levels));
    }

    std::fs::create_dir_all("output/ships").ok();
    std::fs::write("output/ships/levels.txt", &dump).ok();
    println!("{dump}");
}

/// between neighbouring cells. Swept across a range of triangle shapes and winds.
#[test]
fn jib_sail_has_no_holes() {
    use crate::generator::ships::additions::masts::{curved_sail_xy, jib_billow, line_xy};
    use crate::geometry::Point3D;
    use std::collections::HashSet;

    // A few representative jib triangles (bowsprit start A, tip B, foremast top C).
    let cases = [
        (Point3D::new(30, 6, 0), Point3D::new(42, 7, 0), Point3D::new(26, 30, 0)),
        (Point3D::new(20, 5, 0), Point3D::new(28, 5, 0), Point3D::new(17, 20, 0)),
        (Point3D::new(40, 8, 0), Point3D::new(58, 9, 0), Point3D::new(35, 40, 0)),
    ];
    for (a, b, c) in cases {
        // The real path: a roached (curved foot/leech) outline, not a bare triangle.
        let tri = curved_sail_xy(a, b, c, 2.0, 2.0);
        let cellset: HashSet<(i32, i32)> = tri.iter().copied().collect();
        // No **interior holes**: a cell with region neighbours on all four sides must itself be in
        // the region (guards the inward-curve self-intersection bug that dropped sail sections).
        let (x0, x1) = (a.x.min(b.x).min(c.x), a.x.max(b.x).max(c.x));
        let (y0, y1) = (a.y.min(b.y).min(c.y), a.y.max(b.y).max(c.y));
        for x in x0..=x1 {
            for y in y0..=y1 {
                if !cellset.contains(&(x, y))
                    && [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
                        .iter()
                        .all(|n| cellset.contains(n))
                {
                    panic!("curved jib interior hole at ({x},{y}) for corners {a:?} {b:?} {c:?}");
                }
            }
        }
        // Pin the foot (A→B) and luff (B→C), as the build does.
        let mut pinned: HashSet<(i32, i32)> = line_xy(a, b).into_iter().collect();
        pinned.extend(line_xy(b, c));
        for wind10 in [10, 20, 30, 40] {
            let depth = jib_billow(&tri, wind10 as f32 / 10.0, &pinned);
            for &(x, y) in &tri {
                let d = depth[&(x, y)];
                for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                    if cellset.contains(&(nx, ny)) {
                        assert!(
                            (d - depth[&(nx, ny)]).abs() <= 1,
                            "jib step >1 at ({x},{y}) wind={wind10}",
                        );
                    }
                }
            }
        }
    }
}

/// The keel geometry is well-formed across a range of (no-rowboat) lengths,
/// including several random sizes.
#[test]
fn keel_model_property_test() {
    // Ships have a minimum size — no rowboat. Smallest keel ~14, largest ~46.
    let mut rng = crate::noise::RNG::new(99);
    let mut lengths = vec![14, 20, 30, 38, 46];
    for _ in 0..8 {
        lengths.push(rng.rand_i32_range(14, 47));
    }

    for length in lengths {
        let model = keel::build_keel_model(length);

        assert!(!model.cells.is_empty(), "length {length}: empty keel");
        assert_eq!(model.waterline_y, model.depth, "length {length}: waterline == depth");
        assert!(model.depth >= 1, "length {length}: keel must have depth");

        // Depth scales with length: ~30 → 5–6, smallest → 1–2.
        if length == 30 {
            assert!((5..=6).contains(&model.depth), "length 30 depth was {}", model.depth);
        }

        // Sternpost is a full vertical column at x=0 reaching the waterline.
        let post_top = model
            .cells
            .iter()
            .filter(|c| c.local.x == 0)
            .map(|c| c.local.y)
            .max()
            .unwrap();
        assert_eq!(post_top, model.depth, "length {length}: sternpost should reach the waterline");

        // The bow rake reaches the bow tip and the waterline.
        let max_x = model.cells.iter().map(|c| c.local.x).max().unwrap();
        assert_eq!(max_x, length - 1, "length {length}: keel should span to the bow tip");
        let bow_top = model.cells.iter().map(|c| c.local.y).max().unwrap();
        assert_eq!(bow_top, model.depth, "length {length}: rake should rise to the waterline");
    }
}

/// Full offline pipeline: build a keel on a water flatworld, verify the sternpost,
/// flat run and bow rake landed, and write the ASCII side profile.
#[tokio::test]
async fn build_ship_offline() {
    use crate::editor::World;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships::additions::bowsprit::{build_bowsprit_model, BowspritRake};
    use crate::generator::ships::blueprint::{
        render_hull_plan, render_hull_section, render_keel_ascii, render_spar_profile,
    };
    use crate::generator::ships::additions::{self, DeckAddition};
    use crate::generator::ships::{build_ship, ShipSpec, ShipCtx};
    use crate::geometry::{Cardinal, Point2D, Point3D, Rect3D};
    use crate::minecraft::Block;
    use crate::noise::RNG;
    use crate::util::init_logger;

    init_logger();

    // Water flatworld: seabed at y=50, sea surface at y=64.
    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
    let world = World::synthetic_water(build_area, 50, 64);
    let mut editor = world.get_offline_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette = data
        .palettes
        .get(&PaletteId::from("ship_oak"))
        .expect("ship_oak palette (data/palettes/ships/)")
        .clone();

    let mut rng = RNG::new(7);
    let spec = ShipSpec::new(Cardinal::North, 30);
    let anchor = Point2D::new(96, 96);

    let mut ctx = ShipCtx::new(&mut editor, &data, &palette, &mut rng);
    let ship = build_ship(&mut ctx, &spec, anchor).await;
    editor.flush_buffer().await;

    assert!(ship.on_water, "anchor over a water flatworld should be detected as water");

    let keel = &ship.keel;
    let place = &ship.placement;
    let is_air = |b: &Block| b.id == "air".into() || b.id == "minecraft:air".into();
    let solid_at = |p: Point3D| {
        editor.try_get_block(p).as_ref().map_or(false, |b| !is_air(b))
    };

    // Sternpost base is solid.
    assert!(solid_at(place.to_world(Point3D::new(0, 0, 0))), "sternpost base missing");
    // Sternpost rises to the waterline.
    assert!(
        solid_at(place.to_world(Point3D::new(0, keel.depth, 0))),
        "sternpost should reach the waterline"
    );
    // A flat-run slab amidships is present.
    let mid_x = keel.length / 2;
    assert!(solid_at(place.to_world(Point3D::new(mid_x, 0, 0))), "flat run missing amidships");
    // The bow tip is present at the waterline.
    assert!(
        solid_at(place.to_world(Point3D::new(keel.length - 1, keel.depth, 0))),
        "bow rake should reach the waterline at the stem"
    );

    // Hull shell: a side cell at the widest layer (waterline) is placed.
    let hull = &ship.hull;
    assert!(!hull.cells.is_empty(), "hull shell should have cells");

    // The hull respects the keel: every hull cell sits strictly above the keel's
    // crest at its station, so the keel stays the outermost, water-touching part.
    let top = keel.top_profile();
    for c in &hull.cells {
        let kt = top[c.x as usize];
        assert!(
            kt == i32::MIN || c.y > kt,
            "hull cell {c:?} is not above the keel crest ({kt}) at x={}",
            c.x,
        );
    }
    let side = hull
        .cells
        .iter()
        .find(|c| c.y == hull.depth && c.z != 0)
        .expect("hull should have a side cell at the waterline");
    assert!(
        solid_at(place.to_world(*side)),
        "expected hull shell block at {side:?}",
    );

    // On water, the hollow interior is cleared to air (dry hull). Pick a cell near the **top** of
    // the hull, hard against the **side** — above the laid hold floor, clear of the centreline masts
    // and the off-centre companionway — so it stays air after the Stage-3 interior passes.
    let air_probe = hull
        .interior
        .iter()
        .filter(|c| c.y < hull.depth) // below the deck slabs (which fill the y == depth row)
        .max_by_key(|c| (c.y, c.z.abs()))
        .copied();
    if let Some(cell) = air_probe {
        let inside = place.to_world(cell);
        assert!(
            editor.try_get_block(inside).as_ref().map_or(false, |b| is_air(b)),
            "hull interior should be cleared to air at {inside:?} (local {cell:?}), got {:?}",
            editor.try_get_block(inside),
        );
    }

    // Rudder: a raked blade block and a fence attachment are placed aft of the stern.
    let rud = &ship.rudder;
    assert!(!rud.blade.is_empty(), "rudder should have a blade");
    assert!(!rud.fences.is_empty(), "rudder should have fence attachments");
    assert!(
        solid_at(place.to_world(rud.blade[0].local)),
        "expected rudder blade block at {:?}",
        place.to_world(rud.blade[0].local),
    );
    let fence_world = place.to_world(rud.fences[0]);
    assert!(
        editor.try_get_block(fence_world).map_or(false, |b| b.id.as_str().contains("fence")),
        "expected a fence at {fence_world:?}, got {:?}",
        editor.try_get_block(fence_world).map(|b| b.id),
    );

    // Additional deck: blocks are placed above the main deck (topsides + floor).
    let mid = keel.length / 2;
    let beam_hw = ship.hull.max_beam / 2;
    let mut above_deck = false;
    for dy in 1..=6 {
        for z in -beam_hw..=beam_hw {
            if solid_at(place.to_world(Point3D::new(mid, ship.deck.deck_y + dy, z))) {
                above_deck = true;
            }
        }
    }
    assert!(above_deck, "additional deck should place blocks above the main deck");

    // Main railing: a fence rail caps the top weather deck (above the additional deck).
    // Skipped when DEBUG_SKIP excludes it.
    if !additions::DEBUG_SKIP.contains(&DeckAddition::MainRailing) {
        let railing = ship.railing.as_ref().expect("railing should be built");
        assert!(!railing.cap.is_empty(), "railing should have a fence rail cap");
        let cap_world = place.to_world(railing.cap[0]);
        assert!(
            editor.try_get_block(cap_world).map_or(false, |b| b.id.as_str().contains("fence")),
            "expected a fence rail at {cap_world:?}, got {:?}",
            editor.try_get_block(cap_world).map(|b| b.id),
        );
    }

    // Bowsprit: a solid tapered prow extends the bow, carrying a smoothed spar that
    // projects forward past the bow tip. Skipped when DEBUG_SKIP excludes it.
    if !additions::DEBUG_SKIP.contains(&DeckAddition::Bowsprit) {
        let bowsprit = ship.bowsprit.as_ref().expect("bowsprit should be built");
        assert!(!bowsprit.spar.is_empty(), "bowsprit should have a spar");
        assert!(!bowsprit.prow.is_empty(), "bowsprit should have a solid prow");
        let prow_world = place.to_world(bowsprit.prow[0]);
        assert!(
            editor.try_get_block(prow_world).as_ref().map_or(false, |b| !is_air(b)),
            "expected a solid prow block at {prow_world:?}, got {:?}",
            editor.try_get_block(prow_world).map(|b| b.id),
        );
        assert!(
            bowsprit.tip.x > keel.length - 1,
            "bowsprit tip ({}) should project past the bow tip ({})",
            bowsprit.tip.x,
            keel.length - 1,
        );
        // The spar is a block/slab beam (no stairs) — its mid cell is a solid plank or slab.
        let spar_world = place.to_world(bowsprit.spar[bowsprit.spar.len() / 2].local);
        assert!(
            editor.try_get_block(spar_world).map_or(false, |b| {
                let id = b.id.as_str();
                id.contains("slab") || id.contains("plank") || id.contains("log")
            }),
            "expected a block/slab spar at {spar_world:?}, got {:?}",
            editor.try_get_block(spar_world).map(|b| b.id),
        );
    }

    // Deck: a top slab caps the hull's open top.
    let deck = &ship.deck;
    assert!(!deck.cells.is_empty(), "deck should have slabs");
    let deck_world = place.to_world(deck.cells[0]);
    assert!(
        editor.try_get_block(deck_world).map_or(false, |b| b.id.as_str().contains("slab")),
        "expected a deck slab at {deck_world:?}, got {:?}",
        editor.try_get_block(deck_world).map(|b| b.id),
    );

    // Masts: keel-stepped log poles rising above the deck.
    if !additions::DEBUG_SKIP.contains(&DeckAddition::Masts) {
        let masts = ship.masts.as_ref().expect("masts should be built");
        assert_eq!(
            masts.masts.len() as i32,
            ship.tier.mast_count(),
            "mast count should match the size tier",
        );
        let main = &masts.masts[0];
        let log_world = place.to_world(main.cells[main.cells.len() / 2]);
        assert!(
            editor.try_get_block(log_world).map_or(false, |b| b.id.as_str().contains("log")),
            "expected a mast log at {log_world:?}, got {:?}",
            editor.try_get_block(log_world).map(|b| b.id),
        );
    }

    let ascii = render_keel_ascii(keel);
    let plan = render_hull_plan(hull);
    // Oval variant plan for comparison.
    let oval = crate::generator::ships::hull::build_hull_model(
        keel.length,
        keel.depth,
        spec.beam_ratio,
        crate::generator::ships::HullShape::Oval,
        &keel.top_profile(),
    );
    let oval_plan = render_hull_plan(&oval);
    let section = render_hull_section(hull);
    // Spar step-pattern (block/slab) profiles for each rake — the bow shape pre-check.
    let keel_top = keel.top_profile();
    let mut spar_dump = String::new();
    for rake in [
        BowspritRake::Straight,
        BowspritRake::Gentle,
        BowspritRake::Steep,
        BowspritRake::Tiered,
    ] {
        let m = build_bowsprit_model(
            keel.length,
            ship.deck.deck_y,
            ship.deck.deck_y,
            &keel_top,
            &hull.top_half,
            true,
            rake,
        );
        spar_dump.push_str(&render_spar_profile(&m));
        spar_dump.push('\n');
    }
    std::fs::create_dir_all("output/ships").ok();
    std::fs::write("output/ships/keel.txt", &ascii).expect("write ASCII");
    std::fs::write("output/ships/hull.txt", &plan).expect("write hull plan");
    std::fs::write("output/ships/hull_oval.txt", &oval_plan).expect("write oval plan");
    std::fs::write("output/ships/hull_section.txt", &section).expect("write hull section");
    std::fs::write("output/ships/bowsprit_spar.txt", &spar_dump).expect("write spar profiles");

    println!(
        "Keel OK: length={}, depth={}, bow_rake={}, cells={}",
        keel.length,
        keel.depth,
        keel.bow_rake_len,
        keel.cells.len()
    );
    println!("{ascii}");
    println!(
        "Hull OK: max_beam={}, shell_cells={}",
        hull.max_beam,
        hull.cells.len()
    );
    println!("{plan}");
}

/// On land, the keel is built resting on the ground (everything above it), not
/// buried: the flat bottom sits at the ground surface.
#[tokio::test]
async fn build_ship_offline_land() {
    use crate::editor::World;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships::{build_ship, ShipSpec, ShipCtx};
    use crate::geometry::{Cardinal, Point2D, Point3D, Rect3D};
    use crate::minecraft::Block;
    use crate::noise::RNG;
    use crate::util::init_logger;

    init_logger();

    // Dry-land flatworld: solid ground at y=64.
    let ground_y = 64;
    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
    let world = World::synthetic(build_area, ground_y);
    let mut editor = world.get_offline_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette = data
        .palettes
        .get(&PaletteId::from("ship_oak"))
        .expect("ship_oak palette")
        .clone();

    let mut rng = RNG::new(7);
    let spec = ShipSpec::new(Cardinal::North, 30);
    let anchor = Point2D::new(96, 96);

    let mut ctx = ShipCtx::new(&mut editor, &data, &palette, &mut rng);
    let ship = build_ship(&mut ctx, &spec, anchor).await;
    editor.flush_buffer().await;

    assert!(!ship.on_water, "anchor on a land flatworld should be detected as land");

    let place = &ship.placement;
    let is_air = |b: &Block| b.id == "air".into() || b.id == "minecraft:air".into();

    // The keel's flat bottom (local y=0) rests at the ground surface.
    let bottom_world = place.to_world(Point3D::new(ship.keel.length / 2, 0, 0));
    assert_eq!(bottom_world.y, ground_y, "land keel bottom should rest on the ground");
    assert!(
        editor.try_get_block(bottom_world).as_ref().map_or(false, |b| !is_air(b)),
        "land keel bottom should be solid at the ground surface",
    );

    println!(
        "Land keel OK: on_water={}, bottom_y={}, depth={}",
        ship.on_water, place.origin.y, ship.keel.depth,
    );
}

/// Offline: the scatter pass seats ships on a `Water` district, and every ship floats —
/// each footprint cell is water deep enough that the keel clears the seabed. Builds a
/// synthetic water world, inserts one Water district over a central rect, runs
/// `scatter_ships`, and checks the resulting `Ship` claims.
#[tokio::test]
async fn scatter_ships_offline() {
    use crate::editor::World;
    use crate::generator::data::LoadedData;
    use crate::generator::districts::{District, DistrictID, ParcelType};
    use crate::generator::ships::fleet::scatter_ships;
    use crate::generator::ships::keel::keel_depth;
    use crate::generator::BuildClaim;
    use crate::geometry::{Point2D, Point3D, Rect3D};
    use crate::noise::Seed;

    // All-water flatworld: seabed at y=50, sea surface at y=64 → 14 blocks of water.
    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
    let mut world = World::synthetic_water(build_area, 50, 64);

    // One Water district covering a central rect (must run before get_offline_editor,
    // which consumes the world).
    let mut district = District::new(DistrictID(0));
    district.data.parcel_type = ParcelType::Water;
    for x in 40..200 {
        for z in 40..200 {
            district.data.points_2d.insert(Point2D::new(x, z));
        }
    }
    world.districts.insert(DistrictID(0), district);

    let mut editor = world.get_offline_editor();
    let data = LoadedData::load().expect("data");

    let n = scatter_ships(&mut editor, &data, Seed(7)).await;
    assert!(n > 0, "expected ships scattered onto the water district");

    // Every cell claimed for a ship must be water; the deepest keel we could place needs
    // at least this much water under the surface to float clear of the seabed.
    let surface = editor.world().get_motion_blocking_height_at(Point2D::new(96, 96)).expect("surface");
    let mut ship_cells = 0usize;
    for x in 0..256 {
        for z in 0..256 {
            let c = Point2D::new(x, z);
            if editor.world().get_claim(c) == Some(BuildClaim::Ship) {
                ship_cells += 1;
                assert!(editor.world().is_water(c), "ship claim on non-water cell {c:?}");
                let depth = surface - editor.world().get_ocean_floor_height_at(c).expect("seabed");
                assert!(
                    depth >= keel_depth(14) + 1,
                    "ship footprint cell {c:?} too shallow (depth {depth}) — keel would touch bottom",
                );
            }
        }
    }
    assert!(ship_cells >= n, "expected at least one claimed footprint cell per ship");
    println!("scatter_ships_offline: placed {n} ships, {ship_cells} claimed footprint cells");
}

/// Live build: places a row of keels of increasing length into the running
/// For fun: build a single **giant** ship (keel length 100) into the live server's build area, so
/// the whole length-scaled rig can be screenshotted. Needs a **big** build area over water (~200×200,
/// tall) set with `/setbuildarea`, then:
///
/// ```text
/// cargo test build_giant_ship_live -- --nocapture
/// ```
#[tokio::test]
async fn build_giant_ship_live() {
    use crate::editor::{Editor, World};
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships::{ShipSpec, ShipCtx};
    use crate::geometry::{Cardinal, Point2D};
    use crate::http_mod::GDMCHTTPProvider;
    use crate::noise::RNG;
    use crate::util::init_logger;

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let build_area = provider.get_build_area().await.expect("Failed to get build area");
    let world = World::new(&provider).await.expect("Failed to create world");
    let mut editor = Editor::new(build_area, world);

    let data = LoadedData::load().expect("Failed to load data");
    let palette = data
        .palettes
        .get(&PaletteId::from("ship_oak"))
        .expect("ship_oak palette (data/palettes/ships/)")
        .clone();

    const LENGTH: i32 = 100;
    let size = editor.world().world_rect_2d().size;
    // Centre the ship: bows point -z, so anchor (the stern keel point) half a length south of the
    // centre so the hull's midpoint lands on the build-area centre.
    let anchor = Point2D::new(
        (size.x / 2).clamp(0, size.x - 1),
        (size.y / 2 + LENGTH / 2).clamp(0, size.y - 1),
    );

    let mut rng = RNG::new(100);
    let spec = ShipSpec::new(Cardinal::North, LENGTH);
    let mut ctx = ShipCtx::new(&mut editor, &data, &palette, &mut rng);
    let ship = crate::generator::ships::build_ship(&mut ctx, &spec, anchor).await;
    println!(
        "giant ship @ {anchor:?} — {} footing, depth={}, beam={}, weather_deck_y={}, levels={}",
        if ship.on_water { "water" } else { "land" },
        ship.keel.depth,
        ship.hull.max_beam,
        ship.weather_deck_y,
        ship.levels.levels.len(),
    );
    editor.flush_buffer().await;
}

/// server's build area so the shape and proportional depth can be compared in one
/// screenshot. Set a wide build area over water with `/setbuildarea`, then:
///
/// ```text
/// cargo test build_ship_live -- --nocapture
/// ```
///
/// Bows point north (-z); keels are spaced along x. Each floats at its local
/// surface (motion-blocking height).
#[tokio::test]
async fn build_ship_live() {
    use crate::editor::{Editor, World};
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships::{SailState, ShipSpec, ShipCtx};
    use crate::geometry::{Cardinal, Point2D};
    use crate::http_mod::GDMCHTTPProvider;
    use crate::noise::RNG;
    use crate::util::init_logger;

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let build_area = provider.get_build_area().await.expect("Failed to get build area");
    let world = World::new(&provider).await.expect("Failed to create world");
    let mut editor = Editor::new(build_area, world);

    let data = LoadedData::load().expect("Failed to load data");
    let palette = data
        .palettes
        .get(&PaletteId::from("ship_oak"))
        .expect("ship_oak palette (data/palettes/ships/)")
        .clone();

    // A row of ships spanning the size range (no rowboat: ~14–46), centred in the
    // build area. Lengths are spread evenly across the range so every size tier is
    // represented — including a couple of **Small** ships (≤20) that get no additional
    // deck (just a railed main deck). Ships are spaced along x; the hull beam is also
    // along x, so spacing must clear the widest possible hull (max length / beam ratio)
    // plus a gap.
    const KEEL_COUNT: usize = 6;
    const MIN_LEN: i32 = 14;
    const MAX_LEN: i32 = 46;
    let max_beam =
        (MAX_LEN as f32 / crate::generator::ships::DEFAULT_BEAM_RATIO).round() as i32;
    let spacing = max_beam + 4; // widest hull + a gap between ships

    let size = editor.world().world_rect_2d().size;
    // Centre the row in the build area: spread along x about the centre, and (since
    // bows point -z) anchor each half a length south of centre so its midpoint lands
    // on the centre z.
    let center_x = size.x / 2;
    let center_z = size.y / 2;
    let row_width = spacing * (KEEL_COUNT as i32 - 1);
    let start_x = center_x - row_width / 2;

    let mut rng = RNG::new(3);

    for i in 0..KEEL_COUNT {
        // Even spread MIN_LEN..=MAX_LEN → 14, 20, 26, 32, 38, 44 (Small … Huge).
        let length = MIN_LEN + (MAX_LEN - MIN_LEN) * i as i32 / (KEEL_COUNT as i32 - 1);
        let x = (start_x + spacing * i as i32).clamp(0, size.x - 1);
        let anchor_z = (center_z + length / 2).clamp(0, size.y - 1);
        let anchor = Point2D::new(x, anchor_z);
        // Alternate hull shapes so both are visible in one screenshot.
        let hull_shape = if i % 2 == 0 {
            crate::generator::ships::HullShape::Teardrop
        } else {
            crate::generator::ships::HullShape::Oval
        };
        // The deployed-sail billow shape is rolled per ship (weighted Combined/Curtain/Domed).
        // TODO(jib rigging): testing the **furled** (rolled-up) jib — only the rigging/stay should
        // show, no canvas. Switch back to the default `Full` once the stay shape is dialled in.
        let spec = ShipSpec::new(Cardinal::North, length)
            .with_hull_shape(hull_shape)
            .with_sail_state(SailState::Full);

        let mut ctx = ShipCtx::new(&mut editor, &data, &palette, &mut rng);
        let ship = crate::generator::ships::build_ship(&mut ctx, &spec, anchor).await;

        println!(
            "length={} {:?} at {anchor:?} — {} footing, bottom_y={}, depth={}, max_beam={}",
            length,
            ship.hull.shape,
            if ship.on_water { "water" } else { "land" },
            ship.placement.origin.y,
            ship.keel.depth,
            ship.hull.max_beam,
        );
    }

    editor.flush_buffer().await;
}

/// Live end-to-end: classify the build area into districts (so water bodies become `Water`
/// districts), then scatter free-floating ships onto them. Set a build area that spans some
/// open water — ideally a coast or lake — with `/setbuildarea`, then:
///
/// ```text
/// cargo test scatter_ships_live -- --nocapture
/// ```
///
/// Prints how many Water districts were found and how many ships were placed; with no water
/// in the build area it places none (and says so). The ships flush to the server for a
/// screenshot.
#[tokio::test]
async fn scatter_ships_live() {
    use crate::editor::{Editor, World};
    use crate::generator::data::LoadedData;
    use crate::generator::districts::{generate_parcels, ParcelType};
    use crate::generator::ships::fleet::scatter_ships;
    use crate::http_mod::GDMCHTTPProvider;
    use crate::noise::Seed;
    use crate::util::init_logger;

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let build_area = provider.get_build_area().await.expect("Failed to get build area");
    let world = World::new(&provider).await.expect("Failed to create world");
    let mut editor = Editor::new(build_area, world);

    let data = LoadedData::load().expect("Failed to load data");
    let seed = Seed(12345);

    // Partition + classify the terrain; lakes/oceans fall out as `Water` districts.
    generate_parcels(seed, &mut editor).await;

    let water_districts = editor
        .world()
        .districts
        .values()
        .filter(|d| d.data.parcel_type == ParcelType::Water)
        .count();
    println!("Found {water_districts} water district(s)");

    let n = scatter_ships(&mut editor, &data, seed).await;
    println!("Scattered {n} ship(s) across {water_districts} water district(s)");

    if water_districts == 0 {
        println!("(no water in the build area — set a build area over a lake/coast to place ships)");
    }

    editor.flush_buffer().await;
}
