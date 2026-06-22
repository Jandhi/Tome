//! Furnishing for [`RegionType::Plaza`](super::RegionType::Plaza) — a large open
//! space ringed by buildings: the town's civic square. Unlike a nook, a plaza is
//! *built*: we pave the ground (road material with a border accent), then build
//! out one of several [`PlazaType`]s on the most-interior cell — a **market**
//! (cross + awning stalls), a **fountain**, a **well**, a **monument**, or a
//! raised performance **stage** — and ring it with lamp posts and benches with a
//! little greenery in the corners. The roomier types need a bigger open centre;
//! a cramped plaza falls back to the single-cell monument.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::editor::Editor;
use crate::generator::npc::DialogueVolume;
use crate::generator::population::{
    yaw_toward, AnchorScene, AnchorSlot, Occupant, SceneKind, SlotRole,
};
use crate::geometry::{Point2D, Point3D, CARDINALS_2D};
use crate::noise::RNG;

use super::props::{
    cardinal_facing, chebyshev, edge_depth, flatten_blend, inward_dir, is_building, is_path,
    place_bench, place_lantern_post, place_planter, place_tree, put, put_forced,
};
use super::theme::Theme;
use super::Region;

/// What kind of plaza a region becomes — its centrepiece and character. Chosen
/// by the open room available at the most-interior cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlazaType {
    /// A commercial square: a stepped market cross plus scattered awning stalls.
    Market,
    /// 5×5 walled basin with a central spouting pillar — a formal civic square.
    Fountain,
    /// 3×3 covered well: rim wall, water, corner posts, slab roof, hung lantern.
    Well,
    /// Stepped plinth and a tall stone spire with a wall finial — a dry landmark.
    Monument,
    /// A raised wooden performance deck on fence legs, with back-rail and steps.
    Stage,
}

impl PlazaType {
    /// Lowercase key used to look up this type's naming schema in
    /// `data/open_space_names.yaml`.
    pub fn key(self) -> &'static str {
        match self {
            PlazaType::Market => "market",
            PlazaType::Fountain => "fountain",
            PlazaType::Well => "well",
            PlazaType::Monument => "monument",
            PlazaType::Stage => "stage",
        }
    }
}

/// Largest odd square (half-side `radius`: 0=1×1, 1=3×3, 2=5×5) fully inside the
/// region when centred at `c`.
fn max_square_radius(cells: &HashSet<Point2D>, c: Point2D, limit: i32) -> i32 {
    let mut radius = 0;
    while radius < limit {
        let r = radius + 1;
        let fits = (-r..=r).all(|dx| {
            (-r..=r).all(|dz| cells.contains(&Point2D::new(c.x + dx, c.y + dz)))
        });
        if !fits {
            break;
        }
        radius = r;
    }
    radius
}

/// The most-interior region cell (max distance from the perimeter), with the
/// largest odd-square half-radius that fits there.
fn centre_cell(region: &Region, cells: &HashSet<Point2D>) -> (Point2D, i32) {
    let mut dist: HashMap<Point2D, i32> = HashMap::new();
    let mut queue: VecDeque<Point2D> = VecDeque::new();
    for &c in &region.cells {
        if CARDINALS_2D.iter().any(|d| !cells.contains(&(c + *d))) {
            dist.insert(c, 0);
            queue.push_back(c);
        }
    }
    while let Some(c) = queue.pop_front() {
        let dc = dist[&c];
        for d in CARDINALS_2D {
            let n = c + d;
            if cells.contains(&n) && !dist.contains_key(&n) {
                dist.insert(n, dc + 1);
                queue.push_back(n);
            }
        }
    }
    let centre = *region
        .cells
        .iter()
        .max_by_key(|c| dist.get(c).copied().unwrap_or(0))
        .expect("region has cells");
    (centre, max_square_radius(cells, centre, 2))
}

/// Furnish one plaza region in place, returning the [`PlazaType`] it was built
/// as (so the caller can name it for what it is) plus the NPC standing-spot
/// scenes it offers — a stage performer, market vendors at their stalls, and a
/// scatter of onlookers in the crowd. The type is rolled from the open room at
/// the centre. The caller staffs the scenes via the population pass.
pub async fn furnish_plaza(
    editor: &Editor,
    region: &Region,
    rng: &mut RNG,
    theme: &Theme,
) -> (PlazaType, Vec<AnchorScene>) {
    furnish_plaza_inner(editor, region, rng, theme, None).await
}

/// Like [`furnish_plaza`] but forces a specific [`PlazaType`] instead of rolling
/// one — used by the test harness to lay out every type side by side. Falls back
/// to a monument if the forced type can't fit the available centre.
pub(crate) async fn furnish_plaza_as(
    editor: &Editor,
    region: &Region,
    rng: &mut RNG,
    theme: &Theme,
    plaza_type: PlazaType,
) -> (PlazaType, Vec<AnchorScene>) {
    furnish_plaza_inner(editor, region, rng, theme, Some(plaza_type)).await
}

async fn furnish_plaza_inner(
    editor: &Editor,
    region: &Region,
    rng: &mut RNG,
    theme: &Theme,
    forced: Option<PlazaType>,
) -> (PlazaType, Vec<AnchorScene>) {
    let world = editor.world();
    let cells: HashSet<Point2D> = region.cells.iter().copied().collect();
    let height_at = |c: Point2D| world.get_ocean_floor_height_at(c);

    // The paving accent rings the whole square: every region-perimeter cell that
    // isn't a road entrance gets the border material.
    let border_set: HashSet<Point2D> = region
        .cells
        .iter()
        .copied()
        .filter(|&c| {
            CARDINALS_2D.iter().any(|d| !cells.contains(&(c + *d)))
                && !CARDINALS_2D.iter().any(|d| is_path(world.get_claim(c + *d).as_ref()))
        })
        .collect();

    // Flatten the plaza toward its median surface height so the paving reads as a
    // level square. The flatten *eases out* at the border: the two outermost
    // rings only partly level, lerping from natural ground toward the flat
    // interior, so the plaza doesn't drop off a cliff at its edge.
    let mut heights: Vec<i32> = region.cells.iter().map(|&c| height_at(c)).collect();
    heights.sort_unstable();
    let target_top = heights[heights.len() / 2] - 1; // the flat paved surface y

    let depth = edge_depth(&cells);
    let surf: HashMap<Point2D, i32> = region
        .cells
        .iter()
        .map(|&c| {
            let nat = height_at(c) - 1; // natural surface y
            let t = flatten_blend(depth.get(&c).copied().unwrap_or(2));
            (c, (nat as f32 * (1.0 - t) + target_top as f32 * t).round() as i32)
        })
        .collect();

    // The "flat" plateau: cells that fully leveled to the plaza surface. The
    // eased border rings sit at varying heights, so we *furnish only here* — every
    // centrepiece, stall, lamp, bench, and tree lands on one consistent level.
    let flat: HashSet<Point2D> = region
        .cells
        .iter()
        .copied()
        .filter(|c| surf[c] == target_top)
        .collect();

    // Placement lists drawn from the plateau (never the stepped border):
    //  - `ring_cells`: plateau-edge cells (lamps + corner greenery),
    //  - `seat_cells`: plateau cells backing onto a building (benches).
    // Road-entrance cells are excluded so nothing blocks a way in.
    let mut ring_cells: Vec<Point2D> = Vec::new();
    let mut seat_cells: Vec<Point2D> = Vec::new();
    for &c in &flat {
        if CARDINALS_2D.iter().any(|d| is_path(world.get_claim(c + *d).as_ref())) {
            continue;
        }
        if CARDINALS_2D.iter().any(|d| !flat.contains(&(c + *d))) {
            ring_cells.push(c);
        }
        if CARDINALS_2D.iter().any(|d| is_building(world.get_claim(c + *d).as_ref())) {
            seat_cells.push(c);
        }
    }
    let mut decor_cells = ring_cells.clone();
    let mut border_cells = ring_cells;

    // --- Flatten + pave: theme fill edge to edge, border accent on the ring. ---
    for &c in &region.cells {
        // Never re-grade or pave over ground a building stands on.
        if is_building(world.get_claim(c).as_ref()) {
            continue;
        }
        let s = surf[&c]; // tapered surface y for this cell
        let base = height_at(c) - 1; // current surface y
        // Cut anything above the new surface.
        for y in (s + 1)..=base {
            put_forced(editor, c.x, y, c.y, "minecraft:air").await;
        }
        // Fill dips up to just under the new surface.
        for y in (base + 1)..s {
            put_forced(editor, c.x, y, c.y, theme.subsoil).await;
        }
        let mat = if border_set.contains(&c) { theme.pave_border } else { theme.pave };
        put_forced(editor, c.x, s, c.y, mat).await;
    }

    let mut used: HashSet<Point2D> = HashSet::new();
    // NPC scenes this plaza offers, staffed later by the population pass.
    let mut scenes: Vec<AnchorScene> = Vec::new();

    // --- Centrepiece on the most-interior cell. ---
    // Bigger squares unlock the roomier types; a cramped centre falls back to a
    // monument, the only piece that fits a single cell. The centre and its
    // footprint are measured within the flat plateau so the structure sits level.
    let (centre, radius) = centre_cell(region, &flat);
    // Yaw for an NPC at `feet` looking toward the plaza centre — the shared focal
    // point that market vendors and onlookers all orient on.
    let face_centre = |feet: Point3D| yaw_toward(feet, Point3D::new(centre.x, feet.y, centre.y));
    // A forced type still needs to fit: the 5×5 fountain and the 5×5 stage need
    // radius ≥ 2, the other built pieces radius ≥ 1; anything tighter falls back
    // to a monument.
    let fits = |t: PlazaType| match t {
        PlazaType::Fountain | PlazaType::Stage => radius >= 2,
        PlazaType::Monument => true,
        _ => radius >= 1,
    };
    let plaza_type = match forced {
        Some(t) if fits(t) => t,
        Some(_) => PlazaType::Monument,
        None => match radius {
            r if r >= 2 => *rng.choose(&[
                PlazaType::Market,
                PlazaType::Fountain,
                PlazaType::Well,
                PlazaType::Monument,
                PlazaType::Stage,
            ]),
            1 => *rng.choose(&[
                PlazaType::Market,
                PlazaType::Well,
                PlazaType::Monument,
            ]),
            _ => PlazaType::Monument,
        },
    };
    let centre_h = surf[&centre] + 1; // first air above the (flat) centre paving
    match plaza_type {
        PlazaType::Well => build_well(editor, centre, centre_h, theme).await,
        PlazaType::Fountain => build_fountain(editor, centre, centre_h, theme).await,
        PlazaType::Monument => build_monument(editor, centre, centre_h, radius >= 1, theme).await,
        PlazaType::Stage => {
            // Orient the stage toward the largest open stretch of the square, so
            // the troupe plays to as much of the plaza as possible. `front` is the
            // cardinal with the most plaza cells ahead of the centre — the side the
            // audience gathers on, and the way the performers (and the stage's
            // front step) face.
            let front = stage_front(region, centre);
            build_stage(editor, centre, centre_h, front, theme).await;
            // A troupe up on the deck, calling out over the square. The deck
            // planks sit at `centre_h + 1`, so feet stand on top at `centre_h + 2`.
            // The stage rolls its own cast — a solo act, a duet, or a trio — and
            // `staff_scene` then draws a `performing` script with that many lines
            // (see `NpcData::exchange_of_len`). The cast stands in a row across the
            // deck (perpendicular to `front`), all facing `front` out over the
            // audience.
            let perp = Point2D::new(-front.y, front.x);
            let cast = *rng.choose(&[1, 2, 3]);
            let slots = (0..cast)
                .map(|i| {
                    let off = i - cast / 2;
                    let stand = centre + Point2D::new(perp.x * off, perp.y * off);
                    let feet = Point3D::new(stand.x, centre_h + 2, stand.y);
                    let facing = yaw_toward(feet, feet + Point3D::new(front.x * 8, 0, front.y * 8));
                    let mut slot = AnchorSlot::new(feet, facing, SlotRole::Idle);
                    slot.dialogue = Some("performing".to_string());
                    slot.volume = DialogueVolume::Yelled;
                    slot
                })
                .collect();
            scenes.push(AnchorScene::group(SceneKind::Performance, slots));
        }
        // A market has no centrepiece — it's defined by its U-shaped stalls,
        // placed below across the whole open floor.
        PlazaType::Market => {}
    }
    // Reserve the centrepiece footprint (+1 margin) so nothing crowds it. The
    // margin tracks the actual structure, not the fitted square. A market keeps
    // its whole floor free for stalls, so it reserves nothing.
    if plaza_type != PlazaType::Market {
        let piece_half = match plaza_type {
            PlazaType::Fountain => 2,                  // 5×5 basin
            PlazaType::Stage => 2,                      // 5×5 deck
            PlazaType::Monument => i32::from(radius >= 1), // 3×3 or 1×1
            PlazaType::Well => 1,                      // 3×3 rim
            PlazaType::Market => 0,                    // unreachable (guarded above)
        };
        let margin = piece_half + 1;
        for dx in -margin..=margin {
            for dz in -margin..=margin {
                used.insert(Point2D::new(centre.x + dx, centre.y + dz));
            }
        }
    }

    // --- Lamp posts around the ring, spaced out. ---
    rng.shuffle(&mut border_cells);
    let mut lamps: Vec<Point2D> = Vec::new();
    let lamp_target = (region.area / 40).max(2);
    for &c in &border_cells {
        if lamps.len() >= lamp_target {
            break;
        }
        if used.contains(&c) {
            continue;
        }
        if lamps.iter().any(|l| chebyshev(*l, c) < 5) {
            continue;
        }
        place_lantern_post(editor, c, surf[&c] + 1, theme.wood).await;
        used.insert(c);
        lamps.push(c);
    }

    // --- Benches against the buildings, facing inward. ---
    rng.shuffle(&mut seat_cells);
    let bench_target = (region.area / 30).clamp(2, 6);
    let mut benches: Vec<Point2D> = Vec::new();
    for &c in &seat_cells {
        if benches.len() >= bench_target {
            break;
        }
        if used.contains(&c) {
            continue;
        }
        if benches.iter().any(|b| chebyshev(*b, c) < 3) {
            continue;
        }
        if let Some(inward) = inward_dir(world, c, &cells) {
            place_bench(editor, c, surf[&c] + 1, inward, theme.wood).await;
            used.insert(c);
            benches.push(c);
        }
    }

    // --- Market: U-shaped vendor stalls, scattered goods, and a cart. ---
    if plaza_type == PlazaType::Market {
        // Flat interior cells (off the plateau edge) anchor stalls; each stall's
        // mouth faces the centre so the vendor looks out over the square.
        let mut floor_cells: Vec<Point2D> = flat
            .iter()
            .copied()
            .filter(|&c| CARDINALS_2D.iter().all(|d| flat.contains(&(c + *d))))
            .collect();
        rng.shuffle(&mut floor_cells);

        // --- U-shaped stalls (3×3 footprint, mouth toward the centre). ---
        let stall_target = (region.area / 40).clamp(1, 6);
        let mut stalls: Vec<Point2D> = Vec::new();
        for &mouth in &floor_cells {
            if stalls.len() >= stall_target {
                break;
            }
            let dir = step_toward(mouth, centre); // toward centre = mouth faces this way
            let out = Point2D::new(-dir.x, -dir.y); // stall body extends away from centre
            let perp = Point2D::new(-dir.y, dir.x);
            let cell = |i: i32, j: i32| {
                Point2D::new(
                    mouth.x + perp.x * i + out.x * j,
                    mouth.y + perp.y * i + out.y * j,
                )
            };
            // 3×3 footprint must be flat and free; stalls stay well apart.
            let foot: Vec<Point2D> = (0..3).flat_map(|j| (-1..=1).map(move |i| (i, j))).map(|(i, j)| cell(i, j)).collect();
            if foot.iter().any(|p| used.contains(p) || !flat.contains(p)) {
                continue;
            }
            if stalls.iter().any(|s| chebyshev(*s, mouth) < 5) {
                continue;
            }
            build_market_stall(editor, mouth, out, perp, surf[&mouth] + 1, theme, rng).await;
            for p in foot {
                used.insert(p);
            }
            stalls.push(mouth);

            // The vendor stands inside the U, one cell back from the mouth, so
            // they look out over the front counter toward the square — and hawk
            // their wares with a yelled line that carries across the market.
            let stand = mouth + out;
            let feet = Point3D::new(stand.x, surf[&mouth] + 1, stand.y);
            scenes.push(AnchorScene::solo_with(
                feet,
                face_centre(feet),
                SlotRole::Worker,
                Some("market_cry".to_string()),
                DialogueVolume::Yelled,
            ));
        }

        // --- A cart parked on the floor, if one fits. Footprint: 2-cell bed
        // (along perp), the wheel row beside it (dir), and the handle (−perp). ---
        for &c in &floor_cells {
            let perp = Point2D::new(1, 0);
            let dir = Point2D::new(0, 1);
            let foot = [c, c + perp, c + dir, c + perp + dir, c - perp];
            if foot.iter().any(|p| used.contains(p) || !flat.contains(p)) {
                continue;
            }
            build_cart(editor, c, perp, dir, surf[&c] + 1, theme).await;
            for p in foot {
                used.insert(p);
            }
            break; // one cart is plenty
        }

        // --- A few loose barrels and goods scattered around the stalls. ---
        let loose = ["minecraft:barrel", "minecraft:hay_block", "minecraft:composter", "minecraft:decorated_pot", "minecraft:pumpkin"];
        let scatter_target = (region.area / 30).clamp(2, 8);
        let mut scattered = 0;
        for &c in &floor_cells {
            if scattered >= scatter_target {
                break;
            }
            if used.contains(&c) {
                continue;
            }
            let g = loose[(rng.rand_i32_range(0, loose.len() as i32) as usize) % loose.len()];
            put(editor, c.x, surf[&c] + 1, c.y, g).await;
            used.insert(c);
            scattered += 1;
        }
    }

    // --- Corner greenery: a couple of trees and planters on the ring. ---
    rng.shuffle(&mut decor_cells);
    let mut trees = 0;
    let mut planters = 0;
    for &c in &decor_cells {
        if used.contains(&c) {
            continue;
        }
        if trees < 3 {
            let biome = world.get_surface_biome_at(c);
            if place_tree(editor, theme, &biome, c, surf[&c] + 1, rng).await {
                used.insert(c);
                trees += 1;
                continue;
            }
        }
        if planters < 4 {
            place_planter(editor, c, surf[&c] + 1, theme.wood).await;
            used.insert(c);
            planters += 1;
        }
    }

    // --- Crowd: idle folk gathered on the open floor, each facing the centre.
    // Markets and stages draw the biggest crowds (shoppers, an audience); the
    // quiet civic squares get a lighter scatter loitering at the well, fountain,
    // or monument, with their own dialogue. Cells are interior (fully surrounded
    // by paving), clear of structures, and not against a road entrance. ---
    {
        // Per-type character: how big the crowd is and what they're doing there.
        let (browse_key, crowd_target) = match plaza_type {
            PlazaType::Market => ("browsing", (region.area / 50).clamp(2, 5)),
            PlazaType::Stage => ("browsing", (region.area / 50).clamp(2, 5)),
            PlazaType::Fountain => ("by_the_fountain", (region.area / 70).clamp(2, 4)),
            PlazaType::Well => ("at_the_well", (region.area / 80).clamp(1, 3)),
            PlazaType::Monument => ("at_the_monument", (region.area / 90).clamp(1, 2)),
        };
        let mut crowd_cells: Vec<Point2D> = flat
            .iter()
            .copied()
            .filter(|&c| c != centre && !used.contains(&c))
            .filter(|&c| CARDINALS_2D.iter().all(|d| flat.contains(&(c + *d))))
            .filter(|&c| !CARDINALS_2D.iter().any(|d| is_path(world.get_claim(c + *d).as_ref())))
            .collect();
        rng.shuffle(&mut crowd_cells);
        let mut crowd: Vec<Point2D> = Vec::new();
        for &c in &crowd_cells {
            if crowd.len() >= crowd_target as usize {
                break;
            }
            if crowd.iter().any(|p| chebyshev(*p, c) < 3) {
                continue;
            }
            let feet = Point3D::new(c.x, surf[&c] + 1, c.y);
            let mut scene = AnchorScene::solo_with(
                feet,
                face_centre(feet),
                SlotRole::Idle,
                Some(browse_key.to_string()),
                DialogueVolume::Normal,
            );
            // Roughly a third of any crowd are children — kids tagging along,
            // browsing the stalls, watching the show, or playing by the water.
            // Marked `ChildOnly` so the plaza roster staffs them with an actual
            // child (it mints exactly as many kids as there are such slots; see
            // `build_roster`).
            if rng.percent(34) {
                scene.slots[0].occupant = Occupant::ChildOnly;
            }
            scenes.push(scene);
            used.insert(c);
            crowd.push(c);
        }
    }

    (plaza_type, scenes)
}

/// Unit cardinal step from `from` toward `to`, biased to the longer axis so a
/// stall faces squarely down the plaza rather than diagonally.
fn step_toward(from: Point2D, to: Point2D) -> Point2D {
    let (dx, dz) = (to.x - from.x, to.y - from.y);
    if dx.abs() >= dz.abs() {
        Point2D::new(dx.signum(), 0)
    } else {
        Point2D::new(0, dz.signum())
    }
}

/// Minecraft `facing` word for a unit cardinal step (the direction it points).
fn facing_word(dir: Point2D) -> &'static str {
    match (dir.x, dir.y) {
        (0, d) if d < 0 => "north",
        (0, _) => "south",
        (d, _) if d < 0 => "west",
        _ => "east",
    }
}

/// A market trade: the wool colour of its stall canopy, the vendor's work
/// station (sat at the back of the U), and the goods piled on its counters.
struct Vendor {
    wool: &'static str,
    station: &'static str,
    goods: &'static [&'static str],
}

/// The trades that can take a market stall. Each gets a distinct canopy colour
/// and themed wares so a market reads as a row of different sellers.
const VENDORS: &[Vendor] = &[
    Vendor {
        wool: "minecraft:lime_wool",
        station: "minecraft:composter",
        goods: &["minecraft:pumpkin", "minecraft:melon", "minecraft:hay_block", "minecraft:composter"],
    },
    Vendor {
        wool: "minecraft:pink_wool",
        station: "minecraft:flower_pot",
        goods: &["minecraft:poppy", "minecraft:cornflower", "minecraft:azure_bluet", "minecraft:oxeye_daisy"],
    },
    Vendor {
        wool: "minecraft:light_blue_wool",
        station: "minecraft:barrel",
        goods: &["minecraft:barrel", "minecraft:dried_kelp_block", "minecraft:cauldron"],
    },
    Vendor {
        wool: "minecraft:red_wool",
        station: "minecraft:smoker",
        goods: &["minecraft:hay_block", "minecraft:cauldron", "minecraft:barrel"],
    },
    Vendor {
        wool: "minecraft:yellow_wool",
        station: "minecraft:barrel",
        goods: &["minecraft:hay_block", "minecraft:cake", "minecraft:pumpkin"],
    },
    Vendor {
        wool: "minecraft:purple_wool",
        station: "minecraft:loom",
        goods: &["minecraft:white_wool", "minecraft:blue_wool", "minecraft:red_wool"],
    },
    Vendor {
        wool: "minecraft:white_wool",
        station: "minecraft:cartography_table",
        goods: &["minecraft:bookshelf", "minecraft:barrel", "minecraft:lectern"],
    },
    Vendor {
        wool: "minecraft:brown_wool",
        station: "minecraft:decorated_pot",
        goods: &["minecraft:decorated_pot", "minecraft:flower_pot", "minecraft:clay"],
    },
    Vendor {
        wool: "minecraft:gray_wool",
        station: "minecraft:stonecutter",
        goods: &["minecraft:stone", "minecraft:stone_bricks", "minecraft:chiseled_stone_bricks"],
    },
    Vendor {
        wool: "minecraft:cyan_wool",
        station: "minecraft:fletching_table",
        goods: &["minecraft:hay_block", "minecraft:barrel", "minecraft:lectern"],
    },
];

/// A U-shaped vendor stall on a 3×3 footprint: counters wrap three sides (the
/// open mouth faces the plaza centre), the vendor's work station sits at the back
/// inside the U, two front posts and a cantilevered canopy of coloured wool roof
/// it over, and a lantern hangs at the mouth. `mouth` is the front-centre cell,
/// `out` points away from the centre (into the stall) and `perp` is the side
/// axis. `h` is the first air cell above the paving.
async fn build_market_stall(
    editor: &Editor,
    mouth: Point2D,
    out: Point2D,
    perp: Point2D,
    h: i32,
    theme: &Theme,
    rng: &mut RNG,
) {
    let v = rng.choose(VENDORS);
    let fence = format!("minecraft:{}_fence", theme.wood);
    // (i, j): i = side offset (perp), j = depth from the mouth (out).
    let cell = |i: i32, j: i32| {
        Point2D::new(mouth.x + perp.x * i + out.x * j, mouth.y + perp.y * i + out.y * j)
    };

    // Counters wrap the back and both sides; the two front corners are posts, the
    // back-centre is the work station, the rest carry goods.
    let counters = [(-1, 0), (1, 0), (-1, 1), (1, 1), (-1, 2), (0, 2), (1, 2)];
    let posts = [(-1, 0), (1, 0)];
    let station = (0, 2);
    for &(i, j) in &counters {
        let p = cell(i, j);
        if (i, j) == station {
            put(editor, p.x, h, p.y, v.station).await;
        } else {
            put(editor, p.x, h, p.y, theme.stone).await; // counter top
            if !posts.contains(&(i, j)) {
                let g = v.goods[(rng.rand_i32_range(0, v.goods.len() as i32) as usize) % v.goods.len()];
                put(editor, p.x, h + 1, p.y, g).await;
            }
        }
    }
    // Two front posts rise from their counters to carry the canopy.
    for &(i, j) in &posts {
        let p = cell(i, j);
        put(editor, p.x, h + 1, p.y, &fence).await;
        put(editor, p.x, h + 2, p.y, &fence).await;
    }
    // Coloured wool canopy over the whole 3×3, plus a row of overhang toward the
    // centre. The back of the roof cantilevers off the front posts.
    for j in -1..3 {
        for i in -1..=1 {
            let p = cell(i, j);
            put(editor, p.x, h + 3, p.y, v.wool).await;
        }
    }
    // A lantern hung at the mouth lights the wares.
    let m = cell(0, 0);
    put(editor, m.x, h + 2, m.y, "minecraft:lantern[hanging=true]").await;
}

/// A small parked handcart on a 2×2 footprint: a plank bed loaded with a barrel
/// and hay, trapdoor wheels down each long side, and a fence pull-handle.
/// `base` is one bed corner; `perp` runs along the bed, `dir` across it.
async fn build_cart(editor: &Editor, base: Point2D, perp: Point2D, dir: Point2D, h: i32, theme: &Theme) {
    let planks = format!("minecraft:{}_planks", theme.wood);
    let fence = format!("minecraft:{}_fence", theme.wood);
    let wheel_f = facing_word(dir);
    let wheel = format!("minecraft:{}_trapdoor[facing={wheel_f},half=bottom,open=true]", theme.wood);
    let bed0 = base;
    let bed1 = base + perp;
    // Plank bed.
    put(editor, bed0.x, h, bed0.y, &planks).await;
    put(editor, bed1.x, h, bed1.y, &planks).await;
    // Load: a barrel and a stack of hay.
    put(editor, bed0.x, h + 1, bed0.y, "minecraft:barrel[facing=up]").await;
    put(editor, bed1.x, h + 1, bed1.y, "minecraft:hay_block").await;
    // Trapdoor "wheels" stood up against the long side facing `dir`.
    put(editor, (bed0 + dir).x, h, (bed0 + dir).y, &wheel).await;
    put(editor, (bed1 + dir).x, h, (bed1 + dir).y, &wheel).await;
    // A fence pull-handle off one end.
    let handle = bed0 - perp;
    put(editor, handle.x, h, handle.y, &fence).await;
}

/// The cardinal direction a stage should face: the one with the most plaza cells
/// ahead of the centre, so the troupe plays to the largest part of the square.
/// Falls back to north (−z) for a perfectly symmetric plaza.
fn stage_front(region: &Region, centre: Point2D) -> Point2D {
    CARDINALS_2D
        .iter()
        .copied()
        .max_by_key(|d| {
            region
                .cells
                .iter()
                .filter(|p| (p.x - centre.x) * d.x + (p.y - centre.y) * d.y > 0)
                .count()
        })
        .unwrap_or(Point2D::new(0, -1))
}

/// A raised performance stage: a wooden 5×5 deck on fence legs (open
/// underneath), a low back-rail, and an access staircase up one side. `h` is the
/// first air cell above the (flat) centre paving. `front` is the cardinal
/// direction the stage faces — the audience sits on this side and the performers
/// look out over it; the back-rail is opposite, and the access stairs climb the
/// deck's side so they never block the front.
async fn build_stage(editor: &Editor, c: Point2D, h: i32, front: Point2D, theme: &Theme) {
    let pr = 2; // deck half-side: always a 5×5 deck
    // Local axes: `front`/`back` run front-to-back, `perp` spans the width.
    let perp = Point2D::new(-front.y, front.x);
    let cell = |i: i32, j: i32| Point2D::new(c.x + perp.x * i + front.x * j, c.y + perp.y * i + front.y * j);
    let fence = format!("minecraft:{}_fence", theme.wood);
    let deck = format!("minecraft:{}_planks", theme.wood);
    // Legs on the perimeter at floor level; plank deck one block up across the
    // whole footprint (the interior is left open, held by the deck above). `j`
    // runs back (−pr) to front (+pr); `i` spans the width.
    for i in -pr..=pr {
        for j in -pr..=pr {
            let p = cell(i, j);
            if i.abs() == pr || j.abs() == pr {
                put(editor, p.x, h, p.y, &fence).await;
            }
            put(editor, p.x, h + 1, p.y, &deck).await;
        }
    }
    // Low back-rail along the far edge, with lanterns on its corners.
    for i in -pr..=pr {
        let p = cell(i, -pr);
        put(editor, p.x, h + 2, p.y, &fence).await;
    }
    for &i in &[-pr, pr] {
        let p = cell(i, -pr);
        put(editor, p.x, h + 3, p.y, "minecraft:lantern").await;
    }
    // Access staircase up the `+perp` side: a two-step climb (ground stair just
    // outside the deck, then the deck-edge cell turned into a stair) rising toward
    // the deck centre, so the performers' front stays clear.
    let side = perp; // the side the stairs climb (arbitrary; either flank works)
    let toward_deck = Point2D::new(-side.x, -side.y);
    let step = format!("minecraft:{}_stairs[facing={},half=bottom]", theme.wood, cardinal_facing(toward_deck));
    let upper = cell(pr, 0); // deck-edge cell → stair up to deck level
    let lower = cell(pr + 1, 0); // one cell out, sitting on the paving
    put(editor, upper.x, h + 1, upper.y, &step).await;
    put(editor, lower.x, h, lower.y, &step).await;
}

/// 3×3 covered well centred at `c`; `h` is the first air cell above the paving.
/// The centre is a real water shaft dug into the ground, with a chain dropping
/// down the middle of it from the roof.
async fn build_well(editor: &Editor, c: Point2D, h: i32, theme: &Theme) {
    let fence = format!("minecraft:{}_fence", theme.wood);
    // The `chain` block was renamed to `iron_chain` when copper chains arrived in
    // 1.21.9+; the old id silently fails to place, so use the current one.
    let chain = "minecraft:iron_chain";
    /// How deep the water shaft is dug below the paving.
    const SHAFT_DEPTH: i32 = 5;
    let pave = h - 1; // the plaza paving surface y

    // Rim wall around the 8 edge cells (the centre stays open as the shaft).
    for dx in -1..=1 {
        for dz in -1..=1 {
            if dx == 0 && dz == 0 {
                continue;
            }
            put(editor, c.x + dx, h, c.y + dz, theme.wall).await;
        }
    }

    // Dig the centre hole and fill it with water, from the paving down. The
    // surrounding paving + ground walls keep it from draining.
    for y in (pave - SHAFT_DEPTH)..=pave {
        put_forced(editor, c.x, y, c.y, "minecraft:water").await;
    }

    // A chain drops down the middle: hung in air under the roof, then waterlogged
    // as it descends into the water so the shaft still reads as water around it.
    for y in h..=h + 2 {
        put_forced(editor, c.x, y, c.y, chain).await;
    }
    for y in (pave - SHAFT_DEPTH + 1)..=pave {
        put_forced(editor, c.x, y, c.y, &format!("{chain}[waterlogged=true]")).await;
    }

    // Corner posts up to the roof.
    for &(dx, dz) in &[(-1, -1), (1, -1), (-1, 1), (1, 1)] {
        put(editor, c.x + dx, h + 1, c.y + dz, &fence).await;
        put(editor, c.x + dx, h + 2, c.y + dz, &fence).await;
    }
    // 3×3 bottom-slab roof.
    let roof = format!("{}[type=bottom]", theme.slab);
    for dx in -1..=1 {
        for dz in -1..=1 {
            put(editor, c.x + dx, h + 3, c.y + dz, &roof).await;
        }
    }
}

/// 5×5 walled basin with a central spouting pillar, centred at `c`.
async fn build_fountain(editor: &Editor, c: Point2D, h: i32, theme: &Theme) {
    for dx in -2..=2 {
        for dz in -2..=2 {
            let (x, z) = (c.x + dx, c.y + dz);
            let cheb = dx.abs().max(dz.abs());
            match cheb {
                2 => put(editor, x, h, z, theme.wall).await, // basin wall
                1 => put(editor, x, h, z, "minecraft:water").await, // water ring
                _ => {
                    // Central pillar with a water spout on top.
                    put(editor, x, h, z, theme.stone_accent).await;
                    put(editor, x, h + 1, z, theme.stone).await;
                    put(editor, x, h + 2, z, "minecraft:water").await;
                }
            }
        }
    }
}

/// Stepped plinth + pillar + lantern. `wide` builds a 3×3 base, else a 1×1.
/// A tall spire: a stepped stone plinth, a slim worked-stone shaft, and a
/// slender finial of wall blocks tapering to a point. Walls render as a thin
/// central post, so the cap reads as a spike rather than a flat pillar — and
/// there's no light on top. `wide` squares get a broader 3×3 base, a socle
/// course, and a taller shaft than cramped ones.
async fn build_monument(editor: &Editor, c: Point2D, h: i32, wide: bool, theme: &Theme) {
    // Stepped plinth: a broad base, and (when there's room) a 1×1 accent socle
    // the shaft rises from.
    if wide {
        for dx in -1..=1 {
            for dz in -1..=1 {
                put(editor, c.x + dx, h, c.y + dz, theme.stone).await;
            }
        }
        put(editor, c.x, h + 1, c.y, theme.stone_accent).await;
    } else {
        put(editor, c.x, h, c.y, theme.stone).await;
    }

    // Slim shaft of worked stone, tall enough to be seen across the square.
    let shaft_base = if wide { h + 2 } else { h + 1 };
    let shaft_height = if wide { 3 } else { 2 };
    let shaft_top = shaft_base + shaft_height - 1;
    for y in shaft_base..=shaft_top {
        put(editor, c.x, y, c.y, theme.stone).await;
    }

    // Spire: an accent capital, then the shaft narrows into a slender needle of
    // wall blocks tapering to a point (a thin post, not a full column).
    put(editor, c.x, shaft_top + 1, c.y, theme.stone_accent).await;
    let spire_height = 2;
    for i in 1..=spire_height {
        put(editor, c.x, shaft_top + 1 + i, c.y, theme.wall).await;
    }
}
