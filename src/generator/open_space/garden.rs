//! Furnishing for a town house's own back garden (see
//! `city_houses::plots`): a plot behind the house with a gate to the street or
//! open ground and a winding path to it from the back door. Each culture has
//! its own gardens: medieval ones are fenced cottage gardens (kitchen garden,
//! flowers, orchard or work yard) on lawn patched with worn earth and moss;
//! desert ones are walled courtyards with palms and pots; Japanese ones sit
//! behind bamboo fences as raked-gravel, pond or moss gardens (or a vegetable
//! plot), with stepping stones and stone lanterns. Any garden may instead be
//! walled and closed (see `courtyard_wall`). About one garden in four also
//! gets a rare feature from its culture's list: a well, a scarecrow, a
//! fountain, a little shrine and so on, each a piece in
//! `data/gardens/features/` (see `garden_features`).

use crate::util::{HashMap, HashSet};

use crate::editor::Editor;
use crate::generator::buildings::BuildingID;
use crate::generator::BuildClaim;
use crate::geometry::{Point2D, Point3D, CARDINALS_2D};
use crate::noise::RNG;

use super::props::{cardinal_facing, is_path, place_bench, put, put_forced};
use super::garden_features::GardenFeatures;
use super::garden_style::GardenStyle;
use super::theme::Theme;
use serde_derive::Deserialize;

/// What a garden is laid out as (named in `data/gardens/*.yaml` in snake_case).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GardenLayout {
    Kitchen,
    Flowers,
    Orchard,
    WorkYard,
    /// Desert: a walled courtyard with palms, pots and a bench.
    Courtyard,
    /// Japanese: raked gravel with set rocks, clipped shrubs and a stone lantern.
    Zen,
    /// Japanese: a pond with lily pads under a tree, rocks on its rim.
    Pond,
    /// Japanese: moss and ferns, clipped shrubs, a bamboo clump.
    Moss,
}

/// Furnish garden `me` in place. `cells` are its cells, `building` the id of
/// the house it belongs to, `into` the unit step away from the road, `gates` the side-gap
/// cells on the road, `doors` where the house's doors let out (a path runs
/// from the one in the garden to the gate), `owners` maps every garden cell in
/// town to its garden, `fences` is every garden's fence posts (`plan_fences`).
/// `style` is the culture's garden style (`data/gardens/<culture>.yaml`),
/// `features` every garden feature piece.
/// `layout` pins the layout; `None` rolls one from the style's. `walled` pins
/// a closed walled garden over a fenced one; `None` rolls it from the style's
/// rule and `house_area` (the house's frontage x depth). Returns the layout,
/// the rare feature built (if any) and whether it was walled.
#[allow(clippy::too_many_arguments)]
pub async fn furnish_garden(
    editor: &Editor,
    _me: usize,
    cells: &[Point2D],
    building: BuildingID,
    into: Point2D,
    gates: &[Point2D],
    doors: &[Point2D],
    owners: &HashMap<Point2D, usize>,
    fences: &HashSet<Point2D>,
    theme: &Theme,
    style: &GardenStyle,
    features: &GardenFeatures,
    layout: Option<GardenLayout>,
    house_area: i32,
    walled: Option<bool>,
    rng: &mut RNG,
) -> (GardenLayout, Option<String>, bool) {
    let world = editor.world();
    // Walled (closed, no gate) or fenced.
    let rule = &style.walled;
    let walled = rule.always
        || walled.unwrap_or_else(|| rng.percent(if house_area >= rule.big_house_area { rule.big } else { rule.other }));
    let set: HashSet<Point2D> = cells.iter().copied().collect();
    let height_at = |c: Point2D| world.get_ocean_floor_height_at(c);
    // The house's own wall: claimed under its id but not part of the garden
    // (garden cells carry the same claim).
    let own_wall = |c: Point2D| !set.contains(&c) && world.get_claim(c) == Some(BuildClaim::Building(building));
    let gate_set: HashSet<Point2D> = gates.iter().copied().collect();
    // This garden's noise: patches in the lawn, the wander of its path.
    let salt = rng.next_i64() as u32;

    // Fence: this garden's share of the town's fence plan (`plan_fences`).
    let fence: Vec<Point2D> = cells.iter().copied().filter(|c| fences.contains(c)).collect();
    let fence_set: HashSet<Point2D> = fence.iter().copied().collect();

    // Cells inside the fence, and the strip of them against the house.
    let inner: Vec<Point2D> = cells.iter().copied().filter(|c| !fence_set.contains(c)).collect();
    let inner_set: HashSet<Point2D> = inner.iter().copied().collect();
    let by_house: Vec<Point2D> = inner
        .iter()
        .copied()
        .filter(|&c| CARDINALS_2D.iter().any(|d| own_wall(c + *d)))
        .collect();

    // Steps from the house wall (0 = the row against it), through the garden.
    let mut from_wall: HashMap<Point2D, i32> = HashMap::default();
    let mut queue: std::collections::VecDeque<Point2D> = by_house.iter().copied().collect();
    for &c in &by_house {
        from_wall.insert(c, 0);
    }
    while let Some(c) = queue.pop_front() {
        let d = from_wall[&c];
        for dir in CARDINALS_2D {
            let n = c + dir;
            if set.contains(&n) && !from_wall.contains_key(&n) {
                from_wall.insert(n, d + 1);
                queue.push_back(n);
            }
        }
    }
    // A strip with no more than one row between the wall row and the fence
    // has no room for beds: it is a back yard, whatever was asked for.
    let shallow = inner.iter().all(|c| from_wall.get(c).is_some_and(|&d| d <= 1));

    let layout = if shallow {
        GardenLayout::WorkYard
    } else if let Some(l) = layout {
        l
    } else if style.layouts.len() == 1 {
        style.layouts[0].layout
    } else {
        // Weighted, in the style's order.
        let total: i32 = style.layouts.iter().map(|w| w.weight).sum();
        let mut roll = rng.rand_i32_range(0, total.max(1));
        let mut pick = style.layouts[style.layouts.len() - 1].layout;
        for w in &style.layouts {
            if roll < w.weight {
                pick = w.layout;
                break;
            }
            roll -= w.weight;
        }
        pick
    };

    // The gate: on a straight run of fence, to the street first, else to open
    // public ground, never into another garden; nearest the middle of the garden.
    let centre = {
        let n = cells.len().max(1) as i32;
        let s = cells.iter().fold(Point2D::new(0, 0), |a, c| a + *c);
        Point2D::new(s.x / n, s.y / n)
    };
    let mut gate: Option<(i32, i32, Point2D, Point2D)> = None; // (priority, distance, cell, outward)
    for &c in &fence {
        let f = |dx: i32, dz: i32| fence_set.contains(&(c + Point2D::new(dx, dz)));
        let straight = (f(1, 0) && f(-1, 0)) || (f(0, 1) && f(0, -1));
        for d in CARDINALS_2D {
            let (out, inside) = (c + d, c - d);
            if set.contains(&out) || own_wall(out) || owners.contains_key(&out) || !inner_set.contains(&inside) {
                continue;
            }
            let claim = world.get_claim(out);
            let priority = if gate_set.contains(&c) && is_path(claim.as_ref()) {
                0
            } else if !straight {
                continue;
            } else if is_path(claim.as_ref()) {
                1
            } else if matches!(claim, Some(BuildClaim::None | BuildClaim::Nature)) {
                2
            } else {
                continue;
            };
            let dist = (c.x - centre.x).abs() + (c.y - centre.y).abs();
            if gate.map_or(true, |(p, dd, _, _)| (priority, dist) < (p, dd)) {
                gate = Some((priority, dist, c, d));
            }
        }
    }

    // A walled garden is closed: the house is the only way in.
    if walled {
        gate = None;
    }

    // A trodden path from the back door to the gate, wandering on the noise;
    // a big garden routes it past a waypoint off to one side.
    let starts: Vec<Point2D> = inner
        .iter()
        .copied()
        .filter(|c| doors.contains(c) || CARDINALS_2D.iter().any(|d| doors.contains(&(*c + *d))))
        .collect();
    let mut path: HashSet<Point2D> = HashSet::default();
    if let (Some((_, _, g, _)), false) = (gate, starts.is_empty()) {
        let walk: HashSet<Point2D> = inner_set.iter().copied().chain(std::iter::once(g)).collect();
        let cost = |c: Point2D| 10 + (40.0 * blotch(c, 2, salt ^ 0x51)) as u32;
        let mut legs: Vec<Point2D> = Vec::new();
        if inner.len() >= 50 {
            let deepest = from_wall.values().copied().max().unwrap_or(0);
            let start_col = starts[0].x * into.y.abs() + starts[0].y * into.x.abs();
            let mut wps: Vec<Point2D> = inner
                .iter()
                .copied()
                .filter(|c| {
                    let d = from_wall.get(c).copied().unwrap_or(0);
                    let col = c.x * into.y.abs() + c.y * into.x.abs();
                    d * 3 >= deepest && d * 3 <= deepest * 2 && (col - start_col).abs() >= 3
                })
                .collect();
            wps.sort_by_key(|c| (c.x, c.y));
            if !wps.is_empty() {
                legs.push(wps[rng.rand_i32_range(0, wps.len() as i32) as usize]);
            }
        }
        legs.push(g);
        let mut from: Vec<Point2D> = starts.clone();
        for target in legs {
            match route(&from, target, &walk, &cost) {
                Some(leg) => {
                    path.extend(leg.iter().copied());
                    from = vec![target];
                }
                None => break,
            }
        }
        path.remove(&g);
    }
    // Kept clear: the path and the ground right outside the door.
    let clear: HashSet<Point2D> = path.iter().copied().chain(starts.iter().copied()).collect();

    // Ground: the theme's soil in soft patches; a yard worn to bare earth near
    // the house, where people walk; the path trodden in.
    for &c in cells {
        let Some(h) = height_at(c) else { continue; };
        let near = from_wall.get(&c).is_some_and(|&d| d <= 2);
        let soil = if path.contains(&c) {
            // One block, or two laid alternately (stepping stones).
            let i = if style.path.len() >= 2 { (c.x + c.y).rem_euclid(2) as usize } else { 0 };
            style.path.get(i).map_or(theme.ground, |b| b.as_str())
        } else if layout == GardenLayout::WorkYard && near && !style.yard_wear.is_empty() {
            GardenStyle::patch(&style.yard_wear, c, salt).unwrap_or(theme.ground)
        } else {
            GardenStyle::patch(&style.lawn, c, salt).unwrap_or(theme.ground)
        };
        put_forced(editor, c.x, h - 1, c.y, soil).await;
    }

    // The boundary: a wall (`courtyard_wall`), or a cottage or bamboo fence
    // with its gate and sometimes a lantern on a corner.
    if walled {
        courtyard_wall(editor, &fence, &fence_set, &inner_set, style, salt).await;
    }
    let mut lantern_done = style.fence.corner_lantern_chance.map_or(true, |p| !rng.percent(p));
    for &c in fence.iter().filter(|_| !walled) {
        let Some(h) = height_at(c) else { continue; };
        if let Some((_, _, g, d)) = gate {
            if c == g {
                put(editor, c.x, h, c.y, &format!("{}[facing={}]", style.fence.gate, cardinal_facing(d))).await;
                continue;
            }
        }
        put(editor, c.x, h, c.y, &style.fence.post).await;
        if !lantern_done {
            let f = |dx: i32, dz: i32| fence_set.contains(&(c + Point2D::new(dx, dz)));
            let corner = (f(1, 0) || f(-1, 0)) && (f(0, 1) || f(0, -1));
            if corner {
                editor.place_block(&"minecraft:lantern".into(), Point3D::new(c.x, h + 1, c.y)).await;
                lantern_done = true;
            }
        }
    }

    // Cells with something standing on them, so the lawn tufts go elsewhere.
    let mut occupied: HashSet<Point2D> = HashSet::default();

    // Props against the house wall: a barrel, a water butt, a composter.
    let mut wall_spots: Vec<Point2D> = by_house.iter().copied().filter(|c| !clear.contains(c)).collect();
    rng.shuffle(&mut wall_spots);
    let props = (cells.len() / 10).clamp(1, 3);
    let mut placed: Vec<Point2D> = Vec::new();
    for c in wall_spots {
        if placed.len() >= props {
            break;
        }
        if placed.iter().any(|p| (p.x - c.x).abs() + (p.y - c.y).abs() < 2) {
            continue;
        }
        let Some(h) = height_at(c) else { continue; };
        let prop = if layout == GardenLayout::WorkYard && placed.is_empty() {
            "woodpile"
        } else {
            rng.choose(&style.wall_props).as_str()
        };
        if prop == "woodpile" {
            let axis = if into.x != 0 { "z" } else { "x" };
            let log = format!("minecraft:{}_log[axis={axis}]", theme.wood);
            put(editor, c.x, h, c.y, &log).await;
            put(editor, c.x, h + 1, c.y, &log).await;
        } else {
            put(editor, c.x, h, c.y, prop).await;
        }
        occupied.insert(c);
        placed.push(c);
    }

    // The bed: the cells inside the fence, off the house wall and the path.
    let mut bed: Vec<Point2D> = inner
        .iter()
        .copied()
        .filter(|c| !occupied.contains(c) && !by_house.contains(c) && !clear.contains(c))
        .collect();

    // Now and then a rare feature, on a clear patch of bed off the wall.
    let mut special = None;
    if layout != GardenLayout::WorkYard && !style.specials.is_empty() && rng.percent(style.special_chance) {
        let name = rng.choose(&style.specials).clone();
        let r = features.get(&name).map_or(1, |f| f.radius);
        let bed_set: HashSet<Point2D> = bed.iter().copied().collect();
        let mut spots: Vec<Point2D> = bed
            .iter()
            .copied()
            .filter(|&c| from_wall.get(&c).is_some_and(|&d| d >= 2) && square_in(&bed_set, c, r))
            .collect();
        spots.sort_by_key(|c| (c.x, c.y));
        if !spots.is_empty() {
            let c = spots[rng.rand_i32_range(0, spots.len() as i32) as usize];
            if let Some(h) = height_at(c) {
                features.build(editor, &name, c, h, -into, theme, rng).await;
                special = Some(name);
                // Its patch, and a ring of lawn round it.
                bed.retain(|b| (b.x - c.x).abs().max((b.y - c.y).abs()) > r + 1);
                for dx in -r..=r {
                    for dz in -r..=r {
                        occupied.insert(c + Point2D::new(dx, dz));
                    }
                }
            }
        }
    }

    // Room for a full-size tree: four steps from the wall, so the crown clears
    // the eaves, with bed all round the trunk and garden under the whole crown.
    let bed_set: HashSet<Point2D> = bed.iter().copied().collect();
    let roomy = |c: Point2D| {
        from_wall.get(&c).is_some_and(|&d| d >= 4)
            && (-2i32..=2).all(|dx| (-2i32..=2).all(|dz| {
                let n = c + Point2D::new(dx, dz);
                if dx.abs() <= 1 && dz.abs() <= 1 { bed_set.contains(&n) } else { set.contains(&n) }
            }))
    };
    let mut roomy_spots: Vec<Point2D> = bed.iter().copied().filter(|&c| roomy(c)).collect();
    rng.shuffle(&mut roomy_spots);
    // Back of the garden first.
    roomy_spots.sort_by_key(|c| std::cmp::Reverse(from_wall.get(c).copied().unwrap_or(0)));
    let near = |a: Point2D, b: Point2D, r: i32| (a.x - b.x).abs().max((a.y - b.y).abs()) <= r;
    let mut big_trees: Vec<Point2D> = Vec::new();
    let trees_here = match layout {
        GardenLayout::Courtyard => false,
        GardenLayout::Zen => rng.percent(50),
        _ => style.trees.shade_trees,
    };
    if trees_here {
        let (want, spacing) = match layout {
            GardenLayout::Orchard => (usize::MAX, 5),
            GardenLayout::Pond | GardenLayout::Moss | GardenLayout::Zen => (1, 0),
            // Elsewhere one shade tree at the back: always in a big garden
            // (an empty lawn otherwise), sometimes in a smaller one.
            _ => (if bed.len() > 60 || rng.percent(40) { 1 } else { 0 }, 0),
        };
        for &c in &roomy_spots {
            if big_trees.len() >= want {
                break;
            }
            if big_trees.iter().any(|&t| near(t, c, spacing - 1)) {
                continue;
            }
            let Some(h) = height_at(c) else { continue; };
            if let Some(soil) = &style.trees.soil {
                put_forced(editor, c.x, h - 1, c.y, soil).await;
            }
            let azalea = layout == GardenLayout::Moss;
            shade_tree(editor, c, h, &style.trees.wood, azalea, rng).await;
            big_trees.push(c);
        }
        // Nothing grows right under the trunk.
        bed.retain(|&c| !big_trees.iter().any(|&t| near(t, c, 1)));
        for &t in &big_trees {
            occupied.insert(t);
        }
    }
    // Columns run away from the house: `col` counts across the plot.
    let across = Point2D::new(into.y.abs(), into.x.abs());
    let col = |c: Point2D| c.x * across.x + c.y * across.y;

    match layout {
        GardenLayout::Kitchen => {
            // Two crop rows to every path row, the paths running away from the house.
            let crops = [rng.choose(&style.crops).as_str(), rng.choose(&style.crops).as_str()];
            for &c in &bed {
                let Some(h) = height_at(c) else { continue; };
                let k = col(c).rem_euclid(3);
                if k == 2 {
                    put_forced(editor, c.x, h - 1, c.y, "minecraft:dirt_path").await;
                } else {
                    put_forced(editor, c.x, h - 1, c.y, "minecraft:farmland[moisture=7]").await;
                    put(editor, c.x, h, c.y, crops[k as usize]).await;
                }
                occupied.insert(c);
            }
        }
        GardenLayout::Flowers => {
            // Beds two wide, one flower to a bed, grass paths between, as the
            // kitchen garden lays its crops.
            let mut kinds: Vec<&str> = style.flowers.iter().map(|f| f.as_str()).collect();
            rng.shuffle(&mut kinds);
            for &c in &bed {
                occupied.insert(c);
                let Some(h) = height_at(c) else { continue; };
                let k = col(c).rem_euclid(3);
                if k == 2 {
                    continue;
                }
                let kind = kinds[(col(c).div_euclid(3).rem_euclid(kinds.len() as i32)) as usize];
                if rng.percent(85) {
                    put(editor, c.x, h, c.y, kind).await;
                }
            }
        }
        GardenLayout::Orchard => {
            // Full-size trees where they fit (planted above), else small fruit
            // trees on a 4-cell spacing; grass and a few flowers between.
            let mut trees: Vec<Point2D> = big_trees.clone();
            let mut spots: Vec<Point2D> = bed
                .iter()
                .copied()
                .filter(|&c| CARDINALS_2D.iter().all(|d| set.contains(&(c + *d)) && !fence_set.contains(&(c + *d)) && !clear.contains(&(c + *d))))
                .collect();
            rng.shuffle(&mut spots);
            for c in spots {
                if !big_trees.is_empty() {
                    break;
                }
                if trees.iter().any(|t| (t.x - c.x).abs().max((t.y - c.y).abs()) < 4) {
                    continue;
                }
                let Some(h) = height_at(c) else { continue; };
                fruit_tree(editor, c, h, theme.wood).await;
                trees.push(c);
            }
            // A hive beside one trunk, for the blossom.
            let hive = trees.iter().find_map(|t| {
                CARDINALS_2D.iter().map(|d| *t + *d).find(|n| bed.contains(n) && !trees.contains(n))
            });
            if let Some(c) = hive {
                if let Some(h) = height_at(c) {
                    put(editor, c.x, h, c.y, &format!("minecraft:beehive[facing={}]", cardinal_facing(into))).await;
                }
            }
            for &c in &bed {
                occupied.insert(c);
                if Some(c) == hive || trees.iter().any(|t| (t.x - c.x).abs().max((t.y - c.y).abs()) <= 1) {
                    continue;
                }
                let Some(h) = height_at(c) else { continue; };
                let roll = rng.rand_i32_range(0, 100);
                let plant = match roll {
                    0..=11 => rng.choose(&style.flowers).as_str(),
                    12..=44 => "minecraft:short_grass",
                    45..=51 => "minecraft:fern",
                    _ => continue,
                };
                put(editor, c.x, h, c.y, plant).await;
            }
        }
        GardenLayout::Courtyard => {
            // Palms at the back, a bench against the back wall, pots about,
            // the floor paved in patches.
            let mut palms: Vec<Point2D> = Vec::new();
            let mut spots: Vec<Point2D> = bed
                .iter()
                .copied()
                .filter(|&c| from_wall.get(&c).is_some_and(|&d| d >= 3) && square_in(&bed_set, c, 1))
                .collect();
            rng.shuffle(&mut spots);
            spots.sort_by_key(|c| std::cmp::Reverse(from_wall.get(c).copied().unwrap_or(0)));
            let want = (bed.len() / 30).clamp(1, 3);
            for c in spots {
                if palms.len() >= want {
                    break;
                }
                if palms.iter().any(|&p| near(p, c, 4)) || occupied.contains(&c) {
                    continue;
                }
                let Some(h) = height_at(c) else { continue; };
                palm_tree(editor, c, h, rng).await;
                palms.push(c);
                occupied.insert(c);
            }
            let side = Point2D::new(into.y.abs(), into.x.abs());
            let seat = bed
                .iter()
                .copied()
                .filter(|&c| {
                    fence_set.contains(&(c + into))
                        && bed.contains(&(c + side))
                        && !occupied.contains(&c)
                        && !occupied.contains(&(c + side))
                        && !palms.iter().any(|&p| near(p, c, 1) || near(p, c + side, 1))
                })
                .min_by_key(|c| ((col(*c) - col(centre)).abs(), c.x, c.y));
            if let Some(c) = seat {
                if let Some(h) = height_at(c) {
                    place_bench(editor, c, h, -into, "sandstone").await;
                    occupied.insert(c);
                    occupied.insert(c + side);
                }
            }
            let mut spots: Vec<Point2D> = bed.iter().copied().filter(|c| !occupied.contains(c)).collect();
            rng.shuffle(&mut spots);
            let n = (bed.len() / 10).clamp(1, 5);
            let mut done: Vec<Point2D> = Vec::new();
            for c in spots {
                if done.len() >= n {
                    break;
                }
                if done.iter().chain(palms.iter()).any(|p| near(*p, c, 1)) {
                    continue;
                }
                let Some(h) = height_at(c) else { continue; };
                if style.pots.is_empty() {
                    break;
                }
                let pot = rng.choose(&style.pots).as_str();
                if pot == "minecraft:flowering_azalea" {
                    put_forced(editor, c.x, h - 1, c.y, "minecraft:grass_block").await;
                }
                put(editor, c.x, h, c.y, pot).await;
                occupied.insert(c);
                done.push(c);
            }
            for &c in &bed {
                if occupied.contains(&c) || palms.iter().any(|&p| near(p, c, 1)) {
                    continue;
                }
                let Some(h) = height_at(c) else { continue; };
                if blotch(c, 3, salt ^ 0x21) > 0.45 {
                    put_forced(editor, c.x, h - 1, c.y, "minecraft:smooth_sandstone").await;
                }
            }
            for &c in &bed {
                occupied.insert(c);
            }
        }
        GardenLayout::Zen => {
            // Raked gravel; a few set rocks on moss; a stone lantern; a clipped
            // shrub or two by the fence.
            for &c in &bed {
                let Some(h) = height_at(c) else { continue; };
                if big_trees.iter().any(|&t| near(t, c, 1)) {
                    continue;
                }
                put_forced(editor, c.x, h - 1, c.y, theme.rake).await;
            }
            let mut spots: Vec<Point2D> = bed.iter().copied().filter(|&c| square_in(&bed_set, c, 1)).collect();
            rng.shuffle(&mut spots);
            let want = (bed.len() / 20).clamp(1, 4);
            let mut rocks: Vec<Point2D> = Vec::new();
            for c in spots {
                if rocks.len() >= want {
                    break;
                }
                if rocks.iter().chain(big_trees.iter()).any(|&r| near(r, c, 3)) {
                    continue;
                }
                let Some(h) = height_at(c) else { continue; };
                let rock = *rng.choose(theme.rocks);
                put(editor, c.x, h, c.y, rock).await;
                if rng.percent(35) {
                    put(editor, c.x, h + 1, c.y, rock).await;
                }
                for d in CARDINALS_2D {
                    let n = c + d;
                    if let Some(hn) = height_at(n) {
                        put_forced(editor, n.x, hn - 1, n.y, "minecraft:moss_block").await;
                        if rng.percent(30) {
                            put(editor, n.x, hn, n.y, "minecraft:moss_carpet").await;
                        }
                    }
                }
                rocks.push(c);
            }
            let free: Vec<Point2D> = bed
                .iter()
                .copied()
                .filter(|c| !rocks.iter().chain(big_trees.iter()).any(|&r| near(r, *c, 1)))
                .collect();
            japanese_extras(editor, &free, &fence_set, theme, features, rng, 2).await;
            for &c in &bed {
                occupied.insert(c);
            }
        }
        GardenLayout::Pond => {
            let water = carve_pond(editor, &bed, &from_wall, &big_trees, &height_at, rng).await;
            // Rocks on the rim, lily pads on the water.
            let rim: Vec<Point2D> = bed
                .iter()
                .copied()
                .filter(|c| !water.contains(c) && CARDINALS_2D.iter().any(|d| water.contains(&(*c + *d))))
                .collect();
            for &c in &rim {
                let Some(h) = height_at(c) else { continue; };
                if hash01(c.x, c.y, salt ^ 0x41) < 0.3 {
                    put(editor, c.x, h, c.y, *rng.choose(theme.rocks)).await;
                    occupied.insert(c);
                } else {
                    put_forced(editor, c.x, h - 1, c.y, "minecraft:moss_block").await;
                }
            }
            for &c in &water {
                if let Some(h) = height_at(c) {
                    if hash01(c.x, c.y, salt ^ 0x43) < 0.2 {
                        put(editor, c.x, h, c.y, "minecraft:lily_pad").await;
                    }
                }
                occupied.insert(c);
            }
            let free: Vec<Point2D> = bed
                .iter()
                .copied()
                .filter(|c| !water.contains(c) && !occupied.contains(c) && !rim.contains(c) && !big_trees.iter().any(|&t| near(t, *c, 1)))
                .collect();
            japanese_extras(editor, &free, &fence_set, theme, features, rng, 1).await;
            for &c in &free {
                let Some(h) = height_at(c) else { continue; };
                if hash01(c.x, c.y, salt ^ 0x45) < 0.15 {
                    put(editor, c.x, h, c.y, "minecraft:fern").await;
                }
            }
            for &c in &bed {
                occupied.insert(c);
            }
        }
        GardenLayout::Moss => {
            // Moss under ferns, a bamboo clump by the back fence, clipped shrubs.
            for &c in &bed {
                let Some(h) = height_at(c) else { continue; };
                if blotch(c, 2, salt ^ 0x31) > 0.3 {
                    put_forced(editor, c.x, h - 1, c.y, "minecraft:moss_block").await;
                }
            }
            let back = bed
                .iter()
                .copied()
                .filter(|&c| fence_set.contains(&(c + into)) && !big_trees.iter().any(|&t| near(t, c, 2)))
                .max_by_key(|c| ((col(*c) - col(centre)).abs(), c.x, c.y));
            let mut taken: Vec<Point2D> = Vec::new();
            if let Some(c) = back {
                taken = bamboo_clump(editor, c, &bed, &height_at, rng).await;
            }
            let free: Vec<Point2D> = bed
                .iter()
                .copied()
                .filter(|c| !taken.iter().any(|&t| near(t, *c, 1)) && !big_trees.iter().any(|&t| near(t, *c, 1)))
                .collect();
            japanese_extras(editor, &free, &fence_set, theme, features, rng, 3).await;
            for &c in &free {
                let Some(h) = height_at(c) else { continue; };
                let r = hash01(c.x, c.y, salt ^ 0x33);
                if r < 0.18 {
                    put(editor, c.x, h, c.y, "minecraft:fern").await;
                } else if r < 0.22 {
                    put(editor, c.x, h, c.y, "minecraft:moss_carpet").await;
                }
            }
            for &c in &bed {
                occupied.insert(c);
            }
        }
        GardenLayout::WorkYard => {
            let extras = &style.yard_extras;
            let mut spots = bed.clone();
            rng.shuffle(&mut spots);
            let n = (bed.len() / 8).clamp(1, 7);
            let mut done: Vec<Point2D> = Vec::new();
            for c in spots {
                if done.len() >= n {
                    break;
                }
                if done.iter().any(|p| (p.x - c.x).abs().max((p.y - c.y).abs()) < 2) {
                    continue;
                }
                let Some(h) = height_at(c) else { continue; };
                put(editor, c.x, h, c.y, rng.choose(extras).as_str()).await;
                occupied.insert(c);
                done.push(c);
            }
        }
    }

    // Tufts on the open lawn left over: grass and the odd fern or flower; on
    // sand the odd dead bush.
    for &c in &inner {
        if occupied.contains(&c) || clear.contains(&c) {
            continue;
        }
        let r = hash01(c.x, c.y, salt ^ 0x77);
        let Some(tuft) = style.tufts.iter().find(|t| r < t.below) else { continue };
        let tuft = if tuft.block == "$flower" {
            if style.flowers.is_empty() {
                continue;
            }
            style.flowers[(c.x + c.y).rem_euclid(style.flowers.len() as i32) as usize].as_str()
        } else {
            tuft.block.as_str()
        };
        let Some(h) = height_at(c) else { continue; };
        put(editor, c.x, h, c.y, tuft).await;
    }
    (layout, special, walled)
}

/// A walled garden's wall on the fence cells, closed all round (the house is
/// the way in): two blocks high, a base course and an upper course (each
/// weathered in patches by noise) under a slab coping, with pilasters a block
/// taller at the corners, the ends and every fourth cell of a straight run;
/// and, in the cultures that have one, a climbing plant spilling over the
/// coping here and there, with vines down the inside face.
async fn courtyard_wall(
    editor: &Editor,
    fence: &[Point2D],
    fence_set: &HashSet<Point2D>,
    inner: &HashSet<Point2D>,
    style: &GardenStyle,
    salt: u32,
) {
    let world = editor.world();
    let style = &style.wall;
    let f = |c: Point2D, dx: i32, dz: i32| fence_set.contains(&(c + Point2D::new(dx, dz)));
    for &c in fence {
        let Some(h) = world.get_ocean_floor_height_at(c) else { continue; };
        let (ew, ns) = (f(c, 1, 0) || f(c, -1, 0), f(c, 0, 1) || f(c, 0, -1));
        let links = CARDINALS_2D.iter().filter(|d| fence_set.contains(&(c + **d))).count();
        let corner = ew && ns;
        let run = if ew { c.x } else { c.y };
        if corner || links <= 1 || run.rem_euclid(4) == 0 {
            for y in h..=h + 2 {
                put(editor, c.x, y, c.y, &style.pier).await;
            }
            if corner {
                put(editor, c.x, h + 3, c.y, &style.pier_cap).await;
            }
            continue;
        }
        let worn = |y: i32| blotch(Point2D::new(c.x + y * 7, c.y), 2, salt ^ 0x71) > 0.6;
        put(editor, c.x, h, c.y, if worn(0) { &style.base.1 } else { &style.base.0 }).await;
        put(editor, c.x, h + 1, c.y, if worn(1) { &style.body.1 } else { &style.body.0 }).await;
        // The creeper: broad noise picks the patches, per-cell noise rags
        // their edges and leaves gaps.
        let grow = 0.65 * blotch(c, 4, salt ^ 0x65) + 0.35 * hash01(c.x, c.y, salt ^ 0x67);
        let Some((a, b)) = style.creeper.as_ref().filter(|_| grow > 0.6) else {
            put(editor, c.x, h + 2, c.y, &style.coping).await;
            continue;
        };
        let leaves = |k: u32| if hash01(c.x, c.y, salt ^ k) < 0.55 { a.as_str() } else { b.as_str() };
        put(editor, c.x, h + 2, c.y, leaves(0x69)).await;
        // Thick in the middle of a patch: a second tuft on top.
        if grow > 0.72 && hash01(c.x, c.y, salt ^ 0x6b) < 0.5 {
            put(editor, c.x, h + 3, c.y, leaves(0x6d)).await;
        }
        for d in CARDINALS_2D {
            let n = c + d;
            let r = hash01(n.x, n.y, salt ^ 0x6f);
            if !inner.contains(&n) || r > 0.6 {
                continue;
            }
            // Vines down the inside face, hanging from the wall at `-d`, one
            // block or two.
            let vine = format!("minecraft:vine[{}=true]", cardinal_facing(-d));
            put(editor, n.x, h + 1, n.y, &vine).await;
            if r < 0.25 {
                put(editor, n.x, h, n.y, &vine).await;
            }
        }
    }
}

/// Whether the `(2r+1)` square round `c` lies in `set`.
fn square_in(set: &HashSet<Point2D>, c: Point2D, r: i32) -> bool {
    (-r..=r).all(|dx| (-r..=r).all(|dz| set.contains(&(c + Point2D::new(dx, dz)))))
}

/// A Japanese garden's furniture: a stone lantern in the open and up to
/// `shrubs` clipped azalea shrubs against the fence, on cells of `free`.
async fn japanese_extras(
    editor: &Editor,
    free: &[Point2D],
    fence: &HashSet<Point2D>,
    theme: &Theme,
    features: &GardenFeatures,
    rng: &mut RNG,
    shrubs: usize,
) {
    let world = editor.world();
    let set: HashSet<Point2D> = free.iter().copied().collect();
    let mut spots: Vec<Point2D> = free.iter().copied().filter(|&c| square_in(&set, c, 1)).collect();
    rng.shuffle(&mut spots);
    let lantern = spots.first().copied();
    if let Some(c) = lantern {
        if let Some(h) = world.get_ocean_floor_height_at(c) {
            features.build(editor, "stone_lantern", c, h, Point2D::new(0, -1), theme, rng).await;
        }
    }
    let mut by_fence: Vec<Point2D> = free
        .iter()
        .copied()
        .filter(|&c| {
            CARDINALS_2D.iter().any(|d| fence.contains(&(c + *d)))
                && lantern.map_or(true, |l| (l.x - c.x).abs().max((l.y - c.y).abs()) > 1)
        })
        .collect();
    rng.shuffle(&mut by_fence);
    let mut done: Vec<Point2D> = Vec::new();
    for c in by_fence {
        if done.len() >= shrubs {
            break;
        }
        if done.iter().any(|p| (p.x - c.x).abs() + (p.y - c.y).abs() < 4) {
            continue;
        }
        let Some(h) = world.get_ocean_floor_height_at(c) else { continue; };
        let leaves = if rng.percent(50) {
            "minecraft:flowering_azalea_leaves[persistent=true]"
        } else {
            "minecraft:azalea_leaves[persistent=true]"
        };
        put(editor, c.x, h, c.y, leaves).await;
        // Some shrubs are two cells long, clipped low.
        if rng.percent(50) {
            let along = CARDINALS_2D.iter().map(|d| c + *d).find(|n| {
                set.contains(n) && CARDINALS_2D.iter().any(|d| fence.contains(&(*n + *d))) && lantern != Some(*n)
            });
            if let Some(n) = along {
                if let Some(hn) = world.get_ocean_floor_height_at(n) {
                    put(editor, n.x, hn, n.y, leaves).await;
                }
            }
        }
        done.push(c);
    }
}

/// A pond carved in the middle of the bed: up to a fifth of it, spread from
/// the cell deepest in the garden on noise, kept to cells level with that one
/// so the water stays in, and clear of trees. Returns its cells.
async fn carve_pond(
    editor: &Editor,
    bed: &[Point2D],
    from_wall: &HashMap<Point2D, i32>,
    trees: &[Point2D],
    height_at: &dyn Fn(Point2D) -> Option<i32>,
    rng: &mut RNG,
) -> HashSet<Point2D> {
    let bed_set: HashSet<Point2D> = bed.iter().copied().collect();
    let ok = |c: Point2D| {
        from_wall.get(&c).is_some_and(|&d| d >= 2)
            && square_in(&bed_set, c, 1)
            && !trees.iter().any(|t| (t.x - c.x).abs().max((t.y - c.y).abs()) <= 2)
    };
    let mut seeds: Vec<Point2D> = bed.iter().copied().filter(|&c| ok(c)).collect();
    // The middle of the garden's depth, nearest its middle column.
    let deepest = seeds.iter().map(|c| from_wall[c]).max().unwrap_or(0);
    let n = seeds.len().max(1) as i32;
    let mid = seeds.iter().fold(Point2D::new(0, 0), |a, c| a + *c) / n;
    seeds.sort_by_key(|c| ((from_wall[c] - (deepest + 2) / 2).abs() * 4 + (c.x - mid.x).abs() + (c.y - mid.y).abs(), c.x, c.y));
    let Some(&first) = seeds.first() else { return HashSet::default() };
    let Some(h0) = height_at(first) else { return HashSet::default() };
    let target = (bed.len() / 5).clamp(3, 18);
    let salt = rng.next_i64() as u32;
    let score = |a: &Point2D| ((a.x - first.x).pow(2) + (a.y - first.y).pow(2)) as f32 + 3.0 * hash01(a.x, a.y, salt);
    let mut water: HashSet<Point2D> = HashSet::default();
    let mut frontier = vec![first];
    while water.len() < target && !frontier.is_empty() {
        frontier.sort_by(|a, b| score(a).partial_cmp(&score(b)).unwrap().then((a.x, a.y).cmp(&(b.x, b.y))));
        let c = frontier.remove(0);
        if water.contains(&c) || !ok(c) || height_at(c) != Some(h0) {
            continue;
        }
        water.insert(c);
        for d in CARDINALS_2D {
            frontier.push(c + d);
        }
    }
    for &c in &water {
        put_forced(editor, c.x, h0 - 2, c.y, "minecraft:mud").await;
        put_forced(editor, c.x, h0 - 1, c.y, "minecraft:water").await;
    }
    water
}

/// A clump of bamboo round `c`: three to five stalks on `bed` cells next to
/// it, four to seven high. Returns the stalk cells.
pub(super) async fn bamboo_clump(editor: &Editor, c: Point2D, bed: &[Point2D], height_at: &dyn Fn(Point2D) -> Option<i32>, rng: &mut RNG) -> Vec<Point2D> {
    let mut cells: Vec<Point2D> = bed.iter().copied().filter(|b| (b.x - c.x).abs() <= 1 && (b.y - c.y).abs() <= 1).collect();
    cells.sort_by_key(|b| (b.x, b.y));
    rng.shuffle(&mut cells);
    cells.truncate(rng.rand_i32_range(3, 6) as usize);
    for &s in &cells {
        let Some(h) = height_at(s) else { continue; };
        put_forced(editor, s.x, h - 1, s.y, "minecraft:grass_block").await;
        let tall = rng.rand_i32_range(4, 8);
        for i in 0..tall {
            let leaves = match tall - 1 - i {
                0 | 1 => "large",
                2 => "small",
                _ => "none",
            };
            put(editor, s.x, h + i, s.y, &format!("minecraft:bamboo[age=1,leaves={leaves},stage=0]")).await;
        }
    }
    cells
}

/// A date palm: a jungle-log trunk five to seven high under a star of fronds,
/// long ones to the four sides drooping at the tips, short ones to the corners.
async fn palm_tree(editor: &Editor, c: Point2D, h: i32, rng: &mut RNG) {
    put_forced(editor, c.x, h - 1, c.y, "minecraft:coarse_dirt").await;
    let top = h + rng.rand_i32_range(5, 8) - 1;
    for y in h..=top {
        put_forced(editor, c.x, y, c.y, "minecraft:jungle_log").await;
    }
    let leaves = "minecraft:jungle_leaves[persistent=true]";
    put(editor, c.x, top + 1, c.y, leaves).await;
    for d in CARDINALS_2D {
        put(editor, c.x + d.x, top + 1, c.y + d.y, leaves).await;
        put(editor, c.x + 2 * d.x, top + 1, c.y + 2 * d.y, leaves).await;
        put(editor, c.x + 3 * d.x, top, c.y + 3 * d.y, leaves).await;
    }

    for (dx, dz) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
        put(editor, c.x + 2 * dx, top, c.y + 2 * dz, leaves).await;
    }
}

/// Cheapest 4-connected route over `walk` from any of `from` to `to`, each
/// cell entered costing `cost(cell)`. `None` when `to` can't be reached.
fn route(from: &[Point2D], to: Point2D, walk: &HashSet<Point2D>, cost: &dyn Fn(Point2D) -> u32) -> Option<Vec<Point2D>> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    let mut best: HashMap<Point2D, u32> = HashMap::default();
    let mut prev: HashMap<Point2D, Point2D> = HashMap::default();
    let mut heap = BinaryHeap::new();
    for &s in from {
        if walk.contains(&s) {
            best.insert(s, 0);
            heap.push(Reverse((0u32, s.x, s.y)));
        }
    }
    while let Some(Reverse((d, x, z))) = heap.pop() {
        let c = Point2D::new(x, z);
        if c == to {
            let mut out = vec![c];
            let mut cur = c;
            while let Some(&p) = prev.get(&cur) {
                out.push(p);
                cur = p;
            }
            return Some(out);
        }
        if best.get(&c).is_some_and(|&b| d > b) {
            continue;
        }
        for dir in CARDINALS_2D {
            let n = c + dir;
            if !walk.contains(&n) {
                continue;
            }
            let nd = d + cost(n);
            if best.get(&n).map_or(true, |&b| nd < b) {
                best.insert(n, nd);
                prev.insert(n, c);
                heap.push(Reverse((nd, n.x, n.y)));
            }
        }
    }
    None
}

/// A hash of a cell into [0, 1).
pub(super) fn hash01(x: i32, z: i32, salt: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (z as u32).wrapping_mul(0x85EB_CA77) ^ salt.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65536.0
}

/// Smooth value noise in [0, 1): hashed corners `scale` cells apart, blended,
/// so neighbouring cells agree and patches form instead of speckle.
pub(super) fn blotch(c: Point2D, scale: i32, salt: u32) -> f32 {
    let (gx, gz) = (c.x.div_euclid(scale), c.y.div_euclid(scale));
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let tx = smooth(c.x.rem_euclid(scale) as f32 / scale as f32);
    let tz = smooth(c.y.rem_euclid(scale) as f32 / scale as f32);
    let a = hash01(gx, gz, salt) * (1.0 - tx) + hash01(gx + 1, gz, salt) * tx;
    let b = hash01(gx, gz + 1, salt) * (1.0 - tx) + hash01(gx + 1, gz + 1, salt) * tx;
    a * (1.0 - tz) + b * tz
}

/// A full-size tree: a four- or five-block trunk under a rounded crown five
/// across, built block by block so it looks the same offline and live. The
/// style's `wood` picks the timber (oak unless spruce, birch, dark oak or cherry);
/// `azalea` grows an oak with a crown of azalea, some of it in flower.
async fn shade_tree(editor: &Editor, c: Point2D, h: i32, wood: &str, azalea: bool, rng: &mut RNG) {
    let wood = if azalea {
        "oak"
    } else {
        match wood {
            "spruce" | "birch" | "dark_oak" | "cherry" => wood,
            _ => "oak",
        }
    };
    let log = format!("minecraft:{wood}_log");
    let plain = format!("minecraft:{wood}_leaves[persistent=true]");
    let trunk = rng.rand_i32_range(4, 6);
    let top = h + trunk - 1;
    // Two wide layers (corners off, a ragged outer ring), then a narrow cap.
    for (y, r) in [(top - 1, 2i32), (top, 2), (top + 1, 1), (top + 2, 1)] {
        for dx in -r..=r {
            for dz in -r..=r {
                let corner = dx.abs() == r && dz.abs() == r;
                if corner && (r == 2 || y == top + 2) {
                    continue;
                }
                let outer = dx.abs() == r || dz.abs() == r;
                if outer && r == 2 && y == top && rng.percent(30) {
                    continue;
                }
                if dx == 0 && dz == 0 && y <= top {
                    continue;
                }
                let leaves = if !azalea {
                    plain.as_str()
                } else if rng.percent(35) {
                    "minecraft:flowering_azalea_leaves[persistent=true]"
                } else {
                    "minecraft:azalea_leaves[persistent=true]"
                };
                put(editor, c.x + dx, y, c.y + dz, leaves).await;
            }
        }
    }
    for y in h..=top {
        put_forced(editor, c.x, y, c.y, &log).await;
    }
}

/// A small fruit tree: a short trunk and a rounded crown of persistent leaves.
async fn fruit_tree(editor: &Editor, c: Point2D, h: i32, wood: &str) {
    let leaves_wood = match wood {
        "spruce" | "birch" | "jungle" | "acacia" | "dark_oak" | "mangrove" | "cherry" => wood,
        _ => "oak",
    };
    let log = format!("minecraft:{wood}_log");
    let leaves = format!("minecraft:{leaves_wood}_leaves[persistent=true]");
    put(editor, c.x, h, c.y, &log).await;
    put(editor, c.x, h + 1, c.y, &log).await;
    for dx in -1..=1 {
        for dz in -1..=1 {
            if dx == 0 && dz == 0 {
                continue;
            }
            put(editor, c.x + dx, h + 2, c.y + dz, &leaves).await;
            if dx == 0 || dz == 0 {
                put(editor, c.x + dx, h + 3, c.y + dz, &leaves).await;
            }
        }
    }
    put(editor, c.x, h + 2, c.y, &log).await;
    put(editor, c.x, h + 3, c.y, &leaves).await;
}
