//! Furnishing for [`RegionType::Park`](super::RegionType::Park) — edge + large:
//! a green commons. Each park rolls one of several [`ParkType`]s so the town
//! isn't wall-to-wall woods:
//!
//! - **Wooded** — full-size biome trees, a small pond, light wildflowers.
//! - **Flower** — open meadow: heavy wildflowers + tall blooms, a lone tree or two.
//! - **Cemetery** — a walled plot of headstone rows, somber whites, a few dark trees.
//! - **Pond** — a water garden: a large carved pond with lily pads, ringed by trees.
//! - **Lawn** — open green with one central feature (grand tree or monument).
//! - **Zen** — a raked gravel bed with placed rocks, stone lanterns, one shaped tree.
//! - **Fountain** — a walled fountain pool with gravel spokes and flower beds.
//! - **Hedge** — a leafy border + flower apron framing a real walkable hedge maze.
//! - **Cactus** — arid only: spaced cacti, dead bushes, and a rock or two on sand.
//!
//! Arid biomes (desert / badlands) keep the Wooded type but grow it as jungle
//! trees, and add the Cactus type; other treeless biomes (beach / ocean) drop
//! the canopy-dependent Wooded type entirely. All types share the same cell
//! scaffolding (interior / perimeter / seat) and finish with benches against the
//! buildings and lantern posts on the ring.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::editor::Editor;
use crate::generator::npc::DialogueVolume;
use crate::generator::population::{yaw_toward, AnchorScene, Occupant, SlotRole};
use crate::generator::terrain::{generate_tree_feature, Tree};
use crate::geometry::{Point2D, Point3D, CARDINALS_2D};
use crate::minecraft::Biome;
use crate::noise::RNG;

use super::props::{
    chebyshev, edge_depth, flatten_blend, inward_dir, is_building, is_path, lay_soil,
    lay_soil_patch, place_bench, place_lantern_post, put, put_forced,
};
use super::theme::Theme;
use super::Region;

/// What kind of park a region becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParkType {
    Wooded,
    Flower,
    Cemetery,
    Pond,
    Lawn,
    Zen,
    Fountain,
    Hedge,
    Cactus,
}

impl ParkType {
    /// Lowercase key used to look up this type's naming schema in
    /// `data/open_space_names.yaml`.
    pub fn key(self) -> &'static str {
        match self {
            ParkType::Wooded => "wooded",
            ParkType::Flower => "flower",
            ParkType::Cemetery => "cemetery",
            ParkType::Pond => "pond",
            ParkType::Lawn => "lawn",
            ParkType::Zen => "zen",
            ParkType::Fountain => "fountain",
            ParkType::Hedge => "hedge",
            ParkType::Cactus => "cactus",
        }
    }
}

/// Light wildflowers for a wooded park's understory.
const WILD_PLANTS: [&str; 7] = [
    "minecraft:short_grass",
    "minecraft:short_grass",
    "minecraft:poppy",
    "minecraft:dandelion",
    "minecraft:cornflower",
    "minecraft:azure_bluet",
    "minecraft:oxeye_daisy",
];

/// A rich flower mix for a meadow / flower park.
const MEADOW_PLANTS: [&str; 11] = [
    "minecraft:poppy",
    "minecraft:dandelion",
    "minecraft:cornflower",
    "minecraft:azure_bluet",
    "minecraft:oxeye_daisy",
    "minecraft:allium",
    "minecraft:red_tulip",
    "minecraft:orange_tulip",
    "minecraft:white_tulip",
    "minecraft:pink_tulip",
    "minecraft:short_grass",
];

/// Subdued whites and grass for a cemetery.
const SOMBER_PLANTS: [&str; 4] = [
    "minecraft:lily_of_the_valley",
    "minecraft:oxeye_daisy",
    "minecraft:white_tulip",
    "minecraft:short_grass",
];

/// Bold single-colour blooms for the parterre beds of a hedge garden.
const FORMAL_FLOWERS: [&str; 6] = [
    "minecraft:allium",
    "minecraft:poppy",
    "minecraft:cornflower",
    "minecraft:oxeye_daisy",
    "minecraft:orange_tulip",
    "minecraft:pink_tulip",
];

/// Biomes where full-size trees look out of place, so no canopy grows.
fn biome_treeless(biome: &Biome) -> bool {
    let n = biome.name();
    n.contains("desert") || n.contains("badlands") || n.contains("beach") || n.contains("ocean")
}


/// A biome-appropriate full-size park tree (grown server-side via `place
/// feature`), or `None` for treeless biomes. Parks read as a green commons, so
/// these lean to medium/large species rather than the saplings used in nooks.
fn biome_park_tree(biome: &Biome, rng: &mut RNG) -> Option<Tree> {
    if biome_treeless(biome) {
        return None;
    }
    let n = biome.name();
    let weights: Vec<(Tree, f32)> = if n.contains("birch") {
        vec![(Tree::MediumBirch, 3.0), (Tree::LargeBirch, 2.0), (Tree::MediumOak, 1.0)]
    } else if n.contains("taiga")
        || n.contains("spruce")
        || n.contains("pine")
        || n.contains("grove")
        || n.contains("snowy")
        || n.contains("frozen")
    {
        vec![(Tree::MediumPine, 3.0), (Tree::LargePine, 2.0)]
    } else if n.contains("jungle") || n.contains("swamp") || n.contains("mangrove") {
        vec![(Tree::MediumJungle, 3.0), (Tree::LargeJungle, 1.0), (Tree::MediumOak, 1.0)]
    } else if n.contains("savanna") || n.contains("acacia") {
        vec![(Tree::MediumBaobab, 2.0), (Tree::LargeBaobab, 1.0)]
    } else {
        vec![(Tree::MediumOak, 3.0), (Tree::LargeOak, 2.0), (Tree::MediumBirch, 1.0)]
    };
    Some(*rng.choose_weighted_vec(&weights))
}

/// The species for a park tree. A desert-*style* settlement always grows jungle
/// trees — a lush, warm canopy that suits the sandstone palette — whatever the
/// underlying biome. Every other style picks a biome-appropriate species, or
/// `None` where a full-size tree looks out of place.
fn park_tree(theme: &Theme, biome: &Biome, rng: &mut RNG) -> Option<Tree> {
    if theme.arid {
        let weights = vec![
            (Tree::MediumJungle, 3.0),
            (Tree::LargeJungle, 2.0),
            (Tree::SmallJungle, 1.0),
        ];
        return Some(*rng.choose_weighted_vec(&weights));
    }
    biome_park_tree(biome, rng)
}

/// Pick a park type for the region.
///
/// - Arid *style* (desert) gains the Cactus park and keeps Wooded, grown as
///   jungle trees (see [`scatter_trees`]) regardless of the world biome.
/// - In any other style, a treeless biome (desert / badlands / beach / ocean)
///   drops the canopy-dependent Wooded type since no fitting species grows there.
fn choose_park_type(biome: &Biome, arid: bool, rng: &mut RNG) -> ParkType {
    let mut weights = vec![
        (ParkType::Wooded, 3.0),
        (ParkType::Flower, 3.0),
        (ParkType::Cemetery, 2.0),
        (ParkType::Pond, 2.0),
        (ParkType::Lawn, 2.0),
        (ParkType::Zen, 2.0),
        (ParkType::Fountain, 2.0),
        (ParkType::Hedge, 2.0),
    ];
    if arid {
        // Desert style grows the wooded park as jungle, so keep it even on sand.
        weights.push((ParkType::Cactus, 3.0));
    } else if biome_treeless(biome) {
        // No native tree (and no jungle override) here, so wooded can't grow.
        // (Zen, fountain, and hedge all work fine on sand.)
        weights.retain(|(t, _)| *t != ParkType::Wooded);
    }
    *rng.choose_weighted_vec(&weights)
}

/// Level the region toward its median surface height — cut above, fill below,
/// keep a grass top — then update the heightmap so the rest of the furnishing
/// reads the new level. The flatten *eases out* at the border: the two outermost
/// rings of cells only partly level, lerping from their natural height back to
/// the flat interior, so the park doesn't drop off in a cliff at its edge.
async fn flatten_region(editor: &mut Editor, region: &Region, theme: &Theme) {
    let cells: HashSet<Point2D> = region.cells.iter().copied().collect();

    // Natural surface height per cell, and the flat median target.
    let nat: HashMap<Point2D, i32> = {
        let world = editor.world();
        region
            .cells
            .iter()
            .filter_map(|&c| world.get_ocean_floor_height_at(c).map(|h| (c, h)))
            .collect()
    };
    let mut sorted: Vec<i32> = nat.values().copied().collect();
    sorted.sort_unstable();
    let target_h = sorted[sorted.len() / 2];

    // Distance of each cell from the region edge (0 = outermost ring).
    let depth = edge_depth(&cells);

    // Blend each cell from natural (edge) toward the flat target (interior).
    let mut points: HashSet<Point3D> = HashSet::new();
    for &c in &region.cells {
        // Never flatten ground a building stands on.
        if is_building(editor.world().get_claim(c).as_ref()) {
            continue;
        }
        let Some(&nat_h) = nat.get(&c) else { continue; };
        let t = flatten_blend(depth.get(&c).copied().unwrap_or(2));
        let tgt = (nat_h as f32 * (1.0 - t) + target_h as f32 * t).round() as i32;
        let surface = tgt - 1; // the grass surface y
        let cur = nat_h - 1; // current surface y

        // Cut anything above the new surface.
        for y in (surface + 1)..=cur {
            put_forced(editor, c.x, y, c.y, "minecraft:air").await;
        }
        // Fill dips up to just under the new surface.
        for y in (cur + 1)..surface {
            put_forced(editor, c.x, y, c.y, theme.subsoil).await;
        }
        // A sand/gravel cap is a gravity block — guarantee a solid subsoil
        // (sandstone) block directly beneath so the cap (and anything a park
        // later rakes on top, e.g. zen's red sand) can't fall when the column
        // was cut to grade.
        if theme.ground.contains("sand") || theme.ground.contains("gravel") {
            put_forced(editor, c.x, surface - 1, c.y, theme.subsoil).await;
        }
        put_forced(editor, c.x, surface, c.y, theme.ground).await;
        points.insert(Point3D::new(c.x, tgt, c.y));
    }

    editor.world_mut().set_heights(&points);
}

/// Furnish one park region in place, returning the [`ParkType`] it was built as
/// plus the NPC standing-spot scenes it offers — a sparse scatter of idle folk
/// strolling the green, each facing the park's central feature. The caller staffs
/// the scenes via the population pass (the same one that staffs plaza crowds).
pub async fn furnish_park(
    editor: &mut Editor,
    region: &Region,
    rng: &mut RNG,
    theme: &Theme,
) -> (ParkType, Vec<AnchorScene>) {
    // Level the ground first so everything below sits on a clean, flat park.
    flatten_region(editor, region, theme).await;

    let world = editor.world();
    let cells: HashSet<Point2D> = region.cells.iter().copied().collect();

    // Interior cells (room to plant) vs perimeter; seat cells back onto a
    // building. Road entrances stay clear.
    let mut interior: Vec<Point2D> = Vec::new();
    let mut perimeter: Vec<Point2D> = Vec::new();
    let mut seat: Vec<Point2D> = Vec::new();
    for &c in &region.cells {
        let mut on_perimeter = false;
        let mut touches_path = false;
        let mut touches_building = false;
        for d in CARDINALS_2D {
            let n = c + d;
            if !cells.contains(&n) {
                on_perimeter = true;
            }
            if is_path(world.get_claim(n).as_ref()) {
                touches_path = true;
            }
            if is_building(world.get_claim(n).as_ref()) {
                touches_building = true;
            }
        }
        if touches_path {
            continue;
        }
        if on_perimeter {
            perimeter.push(c);
            if touches_building {
                seat.push(c);
            }
        } else {
            interior.push(c);
        }
    }
    rng.shuffle(&mut interior);
    rng.shuffle(&mut perimeter);
    rng.shuffle(&mut seat);

    // One type per region, chosen from the centroid biome.
    let sum = region.cells.iter().fold(Point2D::ZERO, |a, p| a + *p);
    let centroid = sum / region.cells.len().max(1) as i32;
    // Sample the centroid's biome; if that cell is out of bounds, fall back to the
    // first region cell that has one. A region with no in-bounds cell at all can't
    // be furnished, so bail with no scenes.
    let Some(biome) = world
        .get_surface_biome_at(centroid)
        .or_else(|| region.cells.iter().find_map(|&c| world.get_surface_biome_at(c)))
    else {
        return (ParkType::Wooded, Vec::new());
    };
    let park_type = choose_park_type(&biome, theme.arid, rng);

    // In a sand-floored (desert) biome, green the whole park so flowers, trees,
    // and beds sit on grass rather than bare sand. Sandy features (zen's raked
    // bed, the fountain's spokes) re-lay their own surface over this afterwards.
    // The cemetery (somber bare plots) and cactus (wants open sand) parks keep
    // their natural ground, so they opt out of the green.
    let green_floor = theme.ground != "minecraft:grass_block"
        && !matches!(park_type, ParkType::Cemetery | ParkType::Cactus);
    if green_floor {
        for &c in &region.cells {
            let Some(h) = world.get_ocean_floor_height_at(c) else { continue; };
            lay_soil(editor, c, h).await;
        }
    }

    let mut used: HashSet<Point2D> = HashSet::new();

    match park_type {
        ParkType::Wooded => {
            // Desert style grows the wooded park as jungle, whatever the biome.
            scatter_trees(editor, &interior, &mut used, rng, (region.area / 14).max(1), 4, theme)
                .await;
            small_pond(editor, &cells, &interior, &mut used).await;
            dapple_plants(editor, &interior, &mut used, rng, 22, &WILD_PLANTS).await;
        }
        ParkType::Flower => {
            scatter_trees(editor, &interior, &mut used, rng, (region.area / 70).min(2), 6, theme)
                .await;
            dapple_plants(editor, &interior, &mut used, rng, 55, &MEADOW_PLANTS).await;
            scatter_tall_flowers(editor, &interior, &mut used, rng, (region.area / 30).max(1)).await;
        }
        ParkType::Cemetery => {
            furnish_cemetery(editor, region, &cells, &interior, &perimeter, &mut used, rng, theme).await
        }
        ParkType::Pond => furnish_pond(editor, region, &cells, &interior, &mut used, rng, theme).await,
        ParkType::Lawn => furnish_lawn(editor, region, &interior, &mut used, rng, theme).await,
        ParkType::Zen => furnish_zen(editor, region, &cells, &interior, &mut used, rng, theme).await,
        ParkType::Fountain => {
            furnish_fountain(editor, region, &cells, &interior, &mut used, rng, theme).await
        }
        ParkType::Hedge => {
            furnish_hedge(editor, region, &cells, &interior, &perimeter, &mut used, rng, theme).await
        }
        ParkType::Cactus => {
            furnish_cactus(editor, region, &cells, &interior, &mut used, rng, theme).await
        }
    }

    // Shared finish: benches against the buildings, lamps on the ring. (A walled
    // cemetery marks its perimeter used, so these naturally no-op there.)
    place_benches(editor, &cells, &seat, &mut used, region.area, theme.wood).await;
    place_lamps(editor, &perimeter, &mut used, region.area, theme.wood).await;

    // Idle park-goers strolling the green, all facing the park's central feature.
    // A cemetery is a somber plot, not a place to loiter, so it draws no crowd.
    let scenes = if park_type == ParkType::Cemetery {
        Vec::new()
    } else {
        scatter_park_visitors(editor, region, &interior, &used, rng)
    };

    (park_type, scenes)
}

/// A sparse scatter of idle visitors over the park's open interior, each facing
/// the region centroid (its central feature — the grand tree, fountain, pond, or
/// monument). Candidates are unused interior cells fully surrounded by region
/// cells (clear open ground), kept well apart. Roughly a third are children.
/// Standing spots only — staffed later by the population pass.
fn scatter_park_visitors(
    editor: &Editor,
    region: &Region,
    interior: &[Point2D],
    used: &HashSet<Point2D>,
    rng: &mut RNG,
) -> Vec<AnchorScene> {
    let world = editor.world();
    let region_cells: HashSet<Point2D> = region.cells.iter().copied().collect();

    let sum = region.cells.iter().fold(Point2D::ZERO, |a, p| a + *p);
    let centroid = sum / region.cells.len().max(1) as i32;

    // Sparser than a market crowd — a park is a calm green, not a busy square.
    let target = (region.area / 60).clamp(1, 4) as usize;
    let mut scenes: Vec<AnchorScene> = Vec::new();
    let mut placed: Vec<Point2D> = Vec::new();
    for &c in interior {
        if placed.len() >= target {
            break;
        }
        // Clear ground, off any feature, with elbow room from the next visitor.
        if used.contains(&c) || placed.iter().any(|p| chebyshev(*p, c) < 4) {
            continue;
        }
        if !CARDINALS_2D.iter().all(|d| region_cells.contains(&(c + *d))) {
            continue;
        }
        let Some(feet_y) = world.get_ocean_floor_height_at(c) else { continue; };
        let feet = Point3D::new(c.x, feet_y, c.y);
        let facing = yaw_toward(feet, Point3D::new(centroid.x, feet.y, centroid.y));
        let mut scene = AnchorScene::solo_with(
            feet,
            facing,
            SlotRole::Idle,
            Some("in_the_park".to_string()),
            DialogueVolume::Normal,
        );
        // Roughly a third are children, out playing in the green.
        if rng.percent(34) {
            scene.slots[0].occupant = Occupant::ChildOnly;
        }
        scenes.push(scene);
        placed.push(c);
    }
    scenes
}

// --- Type recipes -----------------------------------------------------------

/// Cemetery: a low border wall, headstone rows on a grid, a couple of dark
/// trees, somber whites, and lanterns on the wall.
async fn furnish_cemetery(
    editor: &Editor,
    region: &Region,
    cells: &HashSet<Point2D>,
    interior: &[Point2D],
    perimeter: &[Point2D],
    used: &mut HashSet<Point2D>,
    rng: &mut RNG,
    theme: &Theme,
) {
    let world = editor.world();

    // Border wall around the plot.
    for &c in perimeter {
        if used.contains(&c) {
            continue;
        }
        let Some(h) = world.get_ocean_floor_height_at(c) else { continue; };
        put(editor, c.x, h, c.y, theme.wall).await;
        used.insert(c);
    }
    // A lantern on every eighth wall post so the plot reads at night.
    for (i, &c) in perimeter.iter().enumerate() {
        if i % 8 == 0 {
            let Some(h) = world.get_ocean_floor_height_at(c) else { continue; };
            put(editor, c.x, h + 1, c.y, "minecraft:lantern").await;
        }
    }

    // Headstones on a 3-spaced grid → tidy rows with wider walking gaps.
    let min_x = cells.iter().map(|c| c.x).min().unwrap_or(0);
    let max_x = cells.iter().map(|c| c.x).max().unwrap_or(0);
    let min_z = cells.iter().map(|c| c.y).min().unwrap_or(0);
    let max_z = cells.iter().map(|c| c.y).max().unwrap_or(0);
    let mut gx = min_x;
    while gx <= max_x {
        let mut gz = min_z;
        while gz <= max_z {
            let c = Point2D::new(gx, gz);
            if cells.contains(&c) && !used.contains(&c) {
                if let Some(h) = world.get_ocean_floor_height_at(c) {
                    place_grave(editor, c, h, used, rng, theme).await;
                }
            }
            gz += 3;
        }
        gx += 3;
    }

    scatter_trees(editor, interior, used, rng, (region.area / 60).max(1), 5, theme).await;
    dapple_plants(editor, interior, used, rng, 12, &SOMBER_PLANTS).await;
}

/// Water garden: a large carved pond with lily pads, ringed by trees and light
/// wildflowers.
async fn furnish_pond(
    editor: &Editor,
    region: &Region,
    cells: &HashSet<Point2D>,
    interior: &[Point2D],
    used: &mut HashSet<Point2D>,
    rng: &mut RNG,
    theme: &Theme,
) {
    let max_cells = (region.area / 3).clamp(9, 60);
    let water = carve_pond(editor, cells, interior, used, max_cells).await;
    for &(c, h0) in &water {
        if rng.percent(30) {
            put(editor, c.x, h0, c.y, "minecraft:lily_pad").await;
        }
    }
    scatter_trees(editor, interior, used, rng, (region.area / 25).max(1), 4, theme).await;
    dapple_plants(editor, interior, used, rng, 18, &WILD_PLANTS).await;
}

/// Town green: open grass with one central feature (a grand tree or a small
/// monument) and a sparse dapple of daisies.
async fn furnish_lawn(
    editor: &Editor,
    region: &Region,
    interior: &[Point2D],
    used: &mut HashSet<Point2D>,
    rng: &mut RNG,
    theme: &Theme,
) {
    let world = editor.world();
    let sum = region.cells.iter().fold(Point2D::ZERO, |a, p| a + *p);
    let centroid = sum / region.cells.len().max(1) as i32;

    if let Some(centre) = interior.iter().copied().min_by_key(|c| c.distance_squared(&centroid)) {
        if let Some(h) = world.get_ocean_floor_height_at(centre) {
            let grand_tree = rng.percent(50);
            let tree = if grand_tree {
                world
                    .get_surface_biome_at(centre)
                    .and_then(|b| park_tree(theme, &b, rng))
            } else {
                None
            };
            if let Some(tree) = tree {
                lay_soil_patch(editor, centre, h).await;
                let _ = generate_tree_feature(tree, editor, Point3D::new(centre.x, h, centre.y), rng).await;
            } else {
                place_monument(editor, centre, h, theme).await;
            }
            used.insert(centre);
            for d in CARDINALS_2D {
                used.insert(centre + d);
            }
        }
    }

    dapple_plants(editor, interior, used, rng, 8, &WILD_PLANTS).await;
}

/// Zen garden: a raked gravel bed with a few placed rocks, stone lanterns, and
/// (where trees grow) a single shaped tree. Spare and calm.
async fn furnish_zen(
    editor: &Editor,
    region: &Region,
    cells: &HashSet<Point2D>,
    interior: &[Point2D],
    used: &mut HashSet<Point2D>,
    rng: &mut RNG,
    theme: &Theme,
) {
    let world = editor.world();

    // Rake the whole interior. Left unmarked so features sit on top.
    for &c in interior {
        if used.contains(&c) {
            continue;
        }
        let Some(h) = world.get_ocean_floor_height_at(c) else { continue; };
        put_forced(editor, c.x, h - 1, c.y, theme.rake).await;
    }

    // A few placed rocks, well spaced.
    let rock_target = (region.area / 30).max(2);
    let mut rocks: Vec<Point2D> = Vec::new();
    for &c in interior {
        if rocks.len() >= rock_target {
            break;
        }
        if used.contains(&c) || rocks.iter().any(|r| chebyshev(*r, c) < 3) {
            continue;
        }
        let Some(h) = world.get_ocean_floor_height_at(c) else { continue; };
        place_rock(editor, c, h, cells, used, rng, theme).await;
        rocks.push(c);
    }

    // A couple of stone lanterns.
    let lantern_target = (region.area / 50).max(1);
    let mut lanterns = 0;
    for &c in interior {
        if lanterns >= lantern_target {
            break;
        }
        if used.contains(&c) {
            continue;
        }
        let Some(h) = world.get_ocean_floor_height_at(c) else { continue; };
        put(editor, c.x, h, c.y, theme.stone).await;
        put(editor, c.x, h + 1, c.y, "minecraft:lantern").await;
        used.insert(c);
        lanterns += 1;
    }

    // One shaped tree near the centre (skipped in treeless biomes).
    let sum = region.cells.iter().fold(Point2D::ZERO, |a, p| a + *p);
    let centroid = sum / region.cells.len().max(1) as i32;
    if let Some(centre) = interior
        .iter()
        .copied()
        .filter(|c| !used.contains(c))
        .min_by_key(|c| c.distance_squared(&centroid))
    {
        if let (Some(biome), Some(h)) = (
            world.get_surface_biome_at(centre),
            world.get_ocean_floor_height_at(centre),
        ) {
            if let Some(tree) = park_tree(theme, &biome, rng) {
                lay_soil_patch(editor, centre, h).await;
                let _ = generate_tree_feature(tree, editor, Point3D::new(centre.x, h, centre.y), rng).await;
                used.insert(centre);
                for d in CARDINALS_2D {
                    used.insert(centre + d);
                }
            }
        }
    }

    // A whisper of greenery.
    dapple_plants(editor, interior, used, rng, 6, &["minecraft:fern", "minecraft:short_grass"]).await;
}

/// Fountain garden: a walled fountain pool with a central lantern pillar, gravel
/// spokes out to the edges, and flower beds filling the quarters.
async fn furnish_fountain(
    editor: &Editor,
    region: &Region,
    cells: &HashSet<Point2D>,
    interior: &[Point2D],
    used: &mut HashSet<Point2D>,
    rng: &mut RNG,
    theme: &Theme,
) {
    let world = editor.world();
    let sum = region.cells.iter().fold(Point2D::ZERO, |a, p| a + *p);
    let centroid = sum / region.cells.len().max(1) as i32;

    let Some(centre) = interior.iter().copied().min_by_key(|c| c.distance_squared(&centroid)) else {
        return;
    };
    let Some(h) = world.get_ocean_floor_height_at(centre) else {
        return;
    };

    // Build the largest fountain that fits (5×5), else a plain monument.
    if square_fits(cells, used, centre, 2) {
        let r = 2;
        build_fountain(editor, centre, h, r, theme).await;
        for dx in -r..=r {
            for dz in -r..=r {
                used.insert(Point2D::new(centre.x + dx, centre.y + dz));
            }
        }
        // Gravel spokes from just outside the basin out to the edge.
        for d in CARDINALS_2D {
            let mut step = r + 1;
            loop {
                let p = Point2D::new(centre.x + d.x * step, centre.y + d.y * step);
                if !cells.contains(&p) || used.contains(&p) {
                    break;
                }
                let Some(hp) = world.get_ocean_floor_height_at(p) else { break; };
                put_forced(editor, p.x, hp - 1, p.y, theme.path).await;
                used.insert(p);
                step += 1;
            }
        }
    } else {
        place_monument(editor, centre, h, theme).await;
        used.insert(centre);
        for d in CARDINALS_2D {
            used.insert(centre + d);
        }
    }

    // Flower beds fill the rest.
    dapple_plants(editor, interior, used, rng, 45, &MEADOW_PLANTS).await;
}

/// Hedge maze: a leafy border frames a flower apron ring, and the inner area
/// (inset 2 from the edge) is carved into a real walkable maze of 2-tall hedges.
async fn furnish_hedge(
    editor: &Editor,
    region: &Region,
    cells: &HashSet<Point2D>,
    interior: &[Point2D],
    perimeter: &[Point2D],
    used: &mut HashSet<Point2D>,
    rng: &mut RNG,
    theme: &Theme,
) {
    let _ = (region, interior);
    let world = editor.world();

    // Leafy border hedge (two tall) framing the garden on the non-road edge.
    for &c in perimeter {
        if used.contains(&c) {
            continue;
        }
        let Some(h) = world.get_ocean_floor_height_at(c) else { continue; };
        place_hedge(editor, c, h, theme.hedge).await;
        used.insert(c);
    }

    // Distance of each cell from the region edge, so we can inset the maze.
    let depth = edge_depth(cells);

    // Apron: the ring one in from the border gets a tidy strip of flowers.
    for &c in cells {
        if used.contains(&c) || depth.get(&c).copied().unwrap_or(0) != 1 {
            continue;
        }
        if rng.percent(40) {
            if let Some(h) = world.get_ocean_floor_height_at(c) {
                let flower = *rng.choose(&FORMAL_FLOWERS);
                lay_soil(editor, c, h).await;
                put_forced(editor, c.x, h, c.y, flower).await;
            }
        }
        used.insert(c);
    }

    // Maze fills the inner area (inset 2 from the border). Carve passages with a
    // randomized backtracker, hedge everything else.
    let maze_area: HashSet<Point2D> = cells
        .iter()
        .copied()
        .filter(|c| depth.get(c).copied().unwrap_or(0) >= 2 && !used.contains(c))
        .collect();
    let passages = carve_maze(&maze_area, rng);
    for &c in &maze_area {
        if !passages.contains(&c) {
            if let Some(h) = world.get_ocean_floor_height_at(c) {
                place_hedge(editor, c, h, theme.hedge).await;
            }
        }
        used.insert(c); // keep passages clear of later props too
    }
}

/// Cactus garden (arid biomes only): spaced columns of cactus on open sand,
/// scattered dead bushes, and a rock or two. Everything sits on sand — cacti pop
/// off any other block and snap if a neighbour is solid, so trunks are spaced
/// out and their cardinal neighbours are reserved clear.
async fn furnish_cactus(
    editor: &Editor,
    region: &Region,
    cells: &HashSet<Point2D>,
    interior: &[Point2D],
    used: &mut HashSet<Point2D>,
    rng: &mut RNG,
    theme: &Theme,
) {
    let world = editor.world();
    // Sand a cactus can actually stand on (red sand counts); fall back to plain
    // sand if the theme ground is something exotic.
    let pad = if theme.ground.contains("sand") { theme.ground } else { "minecraft:sand" };

    // Cactus columns, well spaced so no two ever sit edge-to-edge.
    let target = (region.area / 12).max(2);
    let mut placed: Vec<Point2D> = Vec::new();
    for &c in interior {
        if placed.len() >= target {
            break;
        }
        if used.contains(&c) || placed.iter().any(|t| chebyshev(*t, c) < 2) {
            continue;
        }
        let Some(h) = world.get_ocean_floor_height_at(c) else { continue; };
        // Grow the cactus through the Tree system; it lays its own sand footing.
        let _ = generate_tree_feature(Tree::Cactus, editor, Point3D::new(c.x, h, c.y), rng).await;
        used.insert(c);
        // Keep the four sides clear so the cactus survives the next tick.
        for d in CARDINALS_2D {
            used.insert(c + d);
        }
        placed.push(c);
    }

    // A couple of rocks for relief.
    let rock_target = (region.area / 80).max(1);
    let mut rocks = 0;
    for &c in interior {
        if rocks >= rock_target {
            break;
        }
        if used.contains(&c) {
            continue;
        }
        let Some(h) = world.get_ocean_floor_height_at(c) else { continue; };
        place_rock(editor, c, h, cells, used, rng, theme).await;
        rocks += 1;
    }

    // Dead bushes dotted over the sand (they can't sit on grass, so lay sand).
    for &c in interior {
        if used.contains(&c) || !rng.percent(15) {
            continue;
        }
        let Some(h) = world.get_ocean_floor_height_at(c) else { continue; };
        put_forced(editor, c.x, h - 1, c.y, pad).await;
        put_forced(editor, c.x, h, c.y, "minecraft:dead_bush").await;
        used.insert(c);
    }
}

// --- Shared primitives ------------------------------------------------------

/// A two-tall hedge block of the given foliage.
async fn place_hedge(editor: &Editor, c: Point2D, h: i32, hedge: &str) {
    put(editor, c.x, h, c.y, hedge).await;
    put(editor, c.x, h + 1, c.y, hedge).await;
}

/// Carve a maze over `area` with a randomized recursive backtracker and return
/// the passage cells. Maze "rooms" sit on even offsets from the area's min
/// corner; carving a wall opens the cell between two rooms. Cells on odd/odd
/// offsets are never carved, forming the hedge lattice. Runs from every room so
/// disconnected pockets each get their own maze.
fn carve_maze(area: &HashSet<Point2D>, rng: &mut RNG) -> HashSet<Point2D> {
    let min_x = area.iter().map(|c| c.x).min().unwrap_or(0);
    let min_z = area.iter().map(|c| c.y).min().unwrap_or(0);
    let is_room = |c: Point2D| (c.x - min_x) % 2 == 0 && (c.y - min_z) % 2 == 0;

    let mut rooms: Vec<Point2D> = area.iter().copied().filter(|&c| is_room(c)).collect();
    rooms.sort_by_key(|c| (c.x, c.y)); // deterministic start order

    const STEPS: [(i32, i32); 4] = [(2, 0), (-2, 0), (0, 2), (0, -2)];
    let mut passages: HashSet<Point2D> = HashSet::new();
    let mut visited: HashSet<Point2D> = HashSet::new();

    for start in rooms {
        if visited.contains(&start) {
            continue;
        }
        visited.insert(start);
        passages.insert(start);
        let mut stack = vec![start];
        while let Some(&cur) = stack.last() {
            // Unvisited rooms two cells away whose connecting wall is in `area`.
            let opts: Vec<(Point2D, Point2D)> = STEPS
                .iter()
                .filter_map(|&(dx, dz)| {
                    let next = Point2D::new(cur.x + dx, cur.y + dz);
                    let mid = Point2D::new(cur.x + dx / 2, cur.y + dz / 2);
                    (area.contains(&next) && area.contains(&mid) && !visited.contains(&next))
                        .then_some((next, mid))
                })
                .collect();
            if opts.is_empty() {
                stack.pop();
                continue;
            }
            let (next, mid) = opts[rng.rand_i32(opts.len() as i32) as usize];
            visited.insert(next);
            passages.insert(next);
            passages.insert(mid);
            stack.push(next);
        }
    }
    passages
}

/// A small rock: a centre stone (sometimes two tall) with a random skirt of
/// same-block neighbours, kept inside the region.
async fn place_rock(
    editor: &Editor,
    c: Point2D,
    h: i32,
    cells: &HashSet<Point2D>,
    used: &mut HashSet<Point2D>,
    rng: &mut RNG,
    theme: &Theme,
) {
    let rock = *rng.choose(theme.rocks);
    put(editor, c.x, h, c.y, rock).await;
    used.insert(c);
    if rng.percent(40) {
        put(editor, c.x, h + 1, c.y, rock).await;
    }
    for d in CARDINALS_2D {
        let n = c + d;
        if rng.percent(35) && cells.contains(&n) && !used.contains(&n) {
            let Some(hn) = editor.world().get_ocean_floor_height_at(n) else { continue; };
            put(editor, n.x, hn, n.y, rock).await;
            used.insert(n);
        }
    }
}

/// True if the `(2r+1)²` square centred at `c` lies entirely in the region and
/// is clear of `used`.
fn square_fits(cells: &HashSet<Point2D>, used: &HashSet<Point2D>, c: Point2D, r: i32) -> bool {
    (-r..=r).all(|dx| {
        (-r..=r).all(|dz| {
            let p = Point2D::new(c.x + dx, c.y + dz);
            cells.contains(&p) && !used.contains(&p)
        })
    })
}

/// A walled fountain pool of half-width `r` with a central lantern pillar. Every
/// water cell is ringed by the wall or the pillar, so the pool can't spill.
async fn build_fountain(editor: &Editor, c: Point2D, h: i32, r: i32, theme: &Theme) {
    for dx in -r..=r {
        for dz in -r..=r {
            let (x, z) = (c.x + dx, c.y + dz);
            let cheb = dx.abs().max(dz.abs());
            if cheb == r {
                put(editor, x, h, z, theme.wall).await;
            } else if dx == 0 && dz == 0 {
                put(editor, x, h, z, theme.stone).await;
                put(editor, x, h + 1, z, "minecraft:lantern").await;
            } else {
                put_forced(editor, x, h, z, "minecraft:water").await;
            }
        }
    }
}

/// Scatter up to `target` trees over `interior`, keeping at least `min_gap`
/// Chebyshev distance between trunks. Species come from [`park_tree`]: jungle
/// everywhere for the desert style, otherwise a biome-appropriate species
/// (skipping treeless biomes).
async fn scatter_trees(
    editor: &Editor,
    interior: &[Point2D],
    used: &mut HashSet<Point2D>,
    rng: &mut RNG,
    target: usize,
    min_gap: i32,
    theme: &Theme,
) {
    let world = editor.world();
    let mut placed: Vec<Point2D> = Vec::new();
    for &c in interior {
        if placed.len() >= target {
            break;
        }
        if used.contains(&c) || placed.iter().any(|t| chebyshev(*t, c) < min_gap) {
            continue;
        }
        let Some(biome) = world.get_surface_biome_at(c) else {
            continue;
        };
        let Some(tree) = park_tree(theme, &biome, rng) else {
            continue;
        };
        let Some(h) = world.get_ocean_floor_height_at(c) else {
            continue;
        };
        lay_soil_patch(editor, c, h).await;
        let _ = generate_tree_feature(tree, editor, Point3D::new(c.x, h, c.y), rng).await;
        used.insert(c);
        for d in CARDINALS_2D {
            used.insert(c + d);
        }
        placed.push(c);
    }
}

/// Dapple free interior cells with ground plants at the given percent chance.
async fn dapple_plants(
    editor: &Editor,
    interior: &[Point2D],
    used: &mut HashSet<Point2D>,
    rng: &mut RNG,
    percent: i32,
    plants: &[&str],
) {
    let world = editor.world();
    for &c in interior {
        if used.contains(&c) || !rng.percent(percent) {
            continue;
        }
        let plant = *rng.choose(plants);
        let Some(h) = world.get_ocean_floor_height_at(c) else { continue; };
        lay_soil(editor, c, h).await;
        put_forced(editor, c.x, h, c.y, plant).await;
        used.insert(c);
    }
}

/// Scatter up to `target` two-block tall flowers across the meadow.
async fn scatter_tall_flowers(
    editor: &Editor,
    interior: &[Point2D],
    used: &mut HashSet<Point2D>,
    rng: &mut RNG,
    target: usize,
) {
    let world = editor.world();
    let mut placed = 0;
    for &c in interior {
        if placed >= target {
            break;
        }
        if used.contains(&c) || !rng.percent(60) {
            continue;
        }
        let kind = *rng.choose(&["rose_bush", "lilac", "peony", "sunflower"]);
        let Some(h) = world.get_ocean_floor_height_at(c) else { continue; };
        lay_soil(editor, c, h).await;
        put_forced(editor, c.x, h, c.y, &format!("minecraft:{kind}[half=lower]")).await;
        put_forced(editor, c.x, h + 1, c.y, &format!("minecraft:{kind}[half=upper]")).await;
        used.insert(c);
        placed += 1;
    }
}

/// A small pond on the first flat 3×3 interior spot (forced so it sinks in).
async fn small_pond(
    editor: &Editor,
    cells: &HashSet<Point2D>,
    interior: &[Point2D],
    used: &mut HashSet<Point2D>,
) {
    let world = editor.world();
    for &c in interior {
        if used.contains(&c) {
            continue;
        }
        let Some(h0) = world.get_ocean_floor_height_at(c) else { continue; };
        let flat = (-1..=1).all(|dx| {
            (-1..=1).all(|dz| {
                let p = Point2D::new(c.x + dx, c.y + dz);
                cells.contains(&p)
                    && !used.contains(&p)
                    && world.get_ocean_floor_height_at(p) == Some(h0)
            })
        });
        if !flat {
            continue;
        }
        // Contained: every cell bordering the 3×3 basin must be solid at the
        // water level (height >= h0), or the pond would spill out that side.
        let contained = (-1..=1).all(|dx| {
            (-1..=1).all(|dz| {
                let p = Point2D::new(c.x + dx, c.y + dz);
                CARDINALS_2D.iter().all(|d| {
                    let n = p + *d;
                    let inside = (n.x - c.x).abs() <= 1 && (n.y - c.y).abs() <= 1;
                    inside || world.get_ocean_floor_height_at(n).is_some_and(|hn| hn >= h0)
                })
            })
        });
        if !contained {
            continue;
        }
        for dx in -1..=1 {
            for dz in -1..=1 {
                let p = Point2D::new(c.x + dx, c.y + dz);
                put_forced(editor, p.x, h0 - 1, p.y, "minecraft:water").await;
                used.insert(p);
            }
        }
        return;
    }
}

/// Carve a pond by flood-filling flat, same-height cells out from a seed (up to
/// `max_cells`). Returns the water cells with their original surface height so
/// the caller can float lily pads. Water sits one block below the old surface.
async fn carve_pond(
    editor: &Editor,
    cells: &HashSet<Point2D>,
    interior: &[Point2D],
    used: &mut HashSet<Point2D>,
    max_cells: usize,
) -> Vec<(Point2D, i32)> {
    let world = editor.world();
    // Seed on the first flat 3×3 interior spot.
    let Some(&seed) = interior.iter().find(|&&c| {
        if used.contains(&c) {
            return false;
        }
        let Some(h0) = world.get_ocean_floor_height_at(c) else {
            return false;
        };
        (-1..=1).all(|dx| {
            (-1..=1).all(|dz| {
                let p = Point2D::new(c.x + dx, c.y + dz);
                cells.contains(&p)
                    && !used.contains(&p)
                    && world.get_ocean_floor_height_at(p) == Some(h0)
            })
        })
    }) else {
        return Vec::new();
    };

    let Some(h0) = world.get_ocean_floor_height_at(seed) else {
        return Vec::new();
    };
    let mut pond: Vec<(Point2D, i32)> = Vec::new();
    let mut queue: VecDeque<Point2D> = VecDeque::new();
    let mut seen: HashSet<Point2D> = HashSet::new();
    queue.push_back(seed);
    seen.insert(seed);
    while let Some(c) = queue.pop_front() {
        if pond.len() >= max_cells {
            break;
        }
        if !cells.contains(&c) || used.contains(&c) || world.get_ocean_floor_height_at(c) != Some(h0) {
            continue;
        }
        // Only fill where every side is walled by solid terrain (>= h0) or more
        // same-height pond water, so the pool can't spill downhill. Leak-prone
        // fringe cells stay dry and become the shore.
        let contained = CARDINALS_2D
            .iter()
            .all(|d| world.get_ocean_floor_height_at(c + *d).is_some_and(|hn| hn >= h0));
        if !contained {
            continue;
        }
        put_forced(editor, c.x, h0 - 1, c.y, "minecraft:water").await;
        used.insert(c);
        pond.push((c, h0));
        for d in CARDINALS_2D {
            let n = c + d;
            if seen.insert(n) {
                queue.push_back(n);
            }
        }
    }
    pond
}

/// A small stone monument: plinth, pillar, and a lantern on top.
async fn place_monument(editor: &Editor, c: Point2D, h: i32, theme: &Theme) {
    put(editor, c.x, h, c.y, theme.stone).await;
    put(editor, c.x, h + 1, c.y, theme.stone_accent).await;
    put(editor, c.x, h + 2, c.y, theme.wall).await;
    put(editor, c.x, h + 3, c.y, "minecraft:lantern").await;
}

/// A grave: a podzol/coarse-dirt plot topped with a headstone in one of four
/// shapes for variety — a full block (40%), a lone wall (20%), a block capped
/// with a wall (20%), or a single stair (20%).
async fn place_grave(
    editor: &Editor,
    c: Point2D,
    h: i32,
    used: &mut HashSet<Point2D>,
    rng: &mut RNG,
    theme: &Theme,
) {
    put_forced(editor, c.x, h - 1, c.y, theme.grave_mound).await;
    let stone = *rng.choose(theme.graves);
    match rng.rand_i32(100) {
        0..=39 => {
            // Full block — a plain blocky headstone.
            put(editor, c.x, h, c.y, stone).await;
        }
        40..=59 => {
            // A lone wall post — slimmer, like a marker stone.
            put(editor, c.x, h, c.y, theme.wall).await;
        }
        60..=79 => {
            // Block base capped with a wall — a taller, tiered headstone.
            put(editor, c.x, h, c.y, stone).await;
            put(editor, c.x, h + 1, c.y, theme.wall).await;
        }
        _ => {
            // A single stair — a slanted headstone (slab → stairs of the theme).
            let stairs = theme.slab.replace("_slab", "_stairs");
            let facing = *rng.choose(&["north", "south", "east", "west"]);
            put(editor, c.x, h, c.y, &format!("{stairs}[facing={facing},half=bottom]")).await;
        }
    }
    used.insert(c);
}

/// Benches against the buildings, seats facing inward.
async fn place_benches(
    editor: &Editor,
    cells: &HashSet<Point2D>,
    seat: &[Point2D],
    used: &mut HashSet<Point2D>,
    area: usize,
    wood: &str,
) {
    let world = editor.world();
    let target = (area / 40).clamp(1, 4);
    let mut placed: Vec<Point2D> = Vec::new();
    for &c in seat {
        if placed.len() >= target {
            break;
        }
        if used.contains(&c) || placed.iter().any(|b| chebyshev(*b, c) < 4) {
            continue;
        }
        let Some(h) = world.get_ocean_floor_height_at(c) else { continue; };
        if let Some(inward) = inward_dir(world, c, cells) {
            place_bench(editor, c, h, inward, wood).await;
            used.insert(c);
            placed.push(c);
        }
    }
}

/// Lantern posts along the perimeter, spaced out.
async fn place_lamps(
    editor: &Editor,
    perimeter: &[Point2D],
    used: &mut HashSet<Point2D>,
    area: usize,
    wood: &str,
) {
    let world = editor.world();
    let target = (area / 50).max(1);
    let mut placed: Vec<Point2D> = Vec::new();
    for &c in perimeter {
        if placed.len() >= target {
            break;
        }
        if used.contains(&c) || placed.iter().any(|l| chebyshev(*l, c) < 6) {
            continue;
        }
        let Some(h) = world.get_ocean_floor_height_at(c) else { continue; };
        place_lantern_post(editor, c, h, wood).await;
        used.insert(c);
        placed.push(c);
    }
}
