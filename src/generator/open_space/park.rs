//! Furnishing for [`RegionType::Park`](super::RegionType::Park) — edge + large:
//! a green commons. Unlike a plaza we keep the grass and the terrain: scatter
//! full-size biome trees with breathing room, sink a small pond on a flat spot,
//! dapple the ground with flowers, and add a few benches and lamps.

use std::collections::{HashMap, HashSet};

use crate::editor::Editor;
use crate::generator::terrain::{generate_tree, Forest, ForestId, Tree};
use crate::geometry::{Point2D, Point3D, CARDINALS_2D};
use crate::minecraft::Biome;
use crate::noise::RNG;

use super::props::{
    chebyshev, inward_dir, is_building, is_path, place_bench, place_lantern_post, put_forced,
};
use super::Region;

/// Biome-appropriate forest preset for a park's full-size trees, or `None` for
/// treeless biomes.
fn park_forest_id(biome: &Biome) -> Option<&'static str> {
    let n = biome.name();
    if n.contains("birch") {
        Some("birch_forest")
    } else if n.contains("taiga")
        || n.contains("spruce")
        || n.contains("pine")
        || n.contains("grove")
        || n.contains("snowy")
        || n.contains("frozen")
    {
        Some("pine_forest")
    } else if n.contains("desert")
        || n.contains("badlands")
        || n.contains("beach")
        || n.contains("ocean")
    {
        None
    } else {
        Some("oak_forest")
    }
}

/// Medium or smaller — park trees cap at medium so they don't tower over the
/// commons (the forest presets also offer large/mega, which read far too big).
fn is_park_sized(t: Tree) -> bool {
    matches!(
        t,
        Tree::MediumOak
            | Tree::SmallOak
            | Tree::MediumBirch
            | Tree::SmallBirch
            | Tree::MediumPine
            | Tree::SmallPine
            | Tree::MediumJungle
            | Tree::SmallJungle
            | Tree::MediumBaobab
            | Tree::SmallBaobab
            | Tree::MediumHedge
            | Tree::SmallHedge
    )
}

/// A wild ground plant for park dappling.
fn ground_plant(rng: &mut RNG) -> &'static str {
    *rng.choose(&[
        "minecraft:short_grass",
        "minecraft:short_grass",
        "minecraft:poppy",
        "minecraft:dandelion",
        "minecraft:cornflower",
        "minecraft:azure_bluet",
        "minecraft:oxeye_daisy",
    ])
}

/// Furnish one park region in place.
pub async fn furnish_park(
    editor: &Editor,
    region: &Region,
    rng: &mut RNG,
    forests: &HashMap<ForestId, Forest>,
) {
    let world = editor.world();
    let cells: HashSet<Point2D> = region.cells.iter().copied().collect();
    let height_at = |c: Point2D| world.get_ocean_floor_height_at(c);

    // Interior cells (trunk clearance) vs perimeter; seat cells back onto a
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

    let mut used: HashSet<Point2D> = HashSet::new();

    // --- Full-size trees, biome-appropriate, well spaced. ---
    let sum = region.cells.iter().fold(Point2D::ZERO, |a, p| a + *p);
    let centroid = sum / region.cells.len().max(1) as i32;
    let biome = world.get_surface_biome_at(centroid);
    let forest = park_forest_id(&biome).and_then(|id| forests.get(&ForestId::new(id.to_string())));

    rng.shuffle(&mut interior);
    if let Some(forest) = forest {
        // Keep only medium-and-smaller species (with a palette), so park trees
        // stay in scale. Falls back to the full set if a forest has none.
        let mut species: Vec<(Tree, f32)> = forest
            .trees()
            .iter()
            .filter(|(t, _)| is_park_sized(**t) && forest.tree_palette().contains_key(*t))
            .map(|(t, w)| (*t, *w))
            .collect();
        if species.is_empty() {
            species = forest.trees().iter().map(|(t, w)| (*t, *w)).collect();
        }

        let tree_target = (region.area / 14).max(1);
        let mut trees: Vec<Point2D> = Vec::new();
        for &c in &interior {
            if trees.len() >= tree_target {
                break;
            }
            if used.contains(&c) || trees.iter().any(|t| chebyshev(*t, c) < 4) {
                continue;
            }
            let tree_type = *rng.choose_weighted_vec(&species);
            if let Some(palette) = forest.tree_palette().get(&tree_type) {
                generate_tree(tree_type, editor, Point3D::new(c.x, height_at(c), c.y), rng, palette)
                    .await;
                used.insert(c);
                for d in CARDINALS_2D {
                    used.insert(c + d);
                }
                trees.push(c);
            }
        }
    }

    // --- A small pond on a flat 3×3 interior spot (forced so it sinks in). ---
    'pond: for &c in &interior {
        if used.contains(&c) {
            continue;
        }
        let h0 = height_at(c);
        let flat = (-1..=1).all(|dx| {
            (-1..=1).all(|dz| {
                let p = Point2D::new(c.x + dx, c.y + dz);
                cells.contains(&p) && !used.contains(&p) && height_at(p) == h0
            })
        });
        if !flat {
            continue;
        }
        for dx in -1..=1 {
            for dz in -1..=1 {
                let p = Point2D::new(c.x + dx, c.y + dz);
                put_forced(editor, p.x, h0 - 1, p.y, "minecraft:water").await;
                used.insert(p);
            }
        }
        break 'pond;
    }

    // --- Dapple the remaining ground with flowers and grass. ---
    for &c in &interior {
        if used.contains(&c) || !rng.percent(22) {
            continue;
        }
        let plant = ground_plant(rng);
        put_forced(editor, c.x, height_at(c), c.y, plant).await;
        used.insert(c);
    }

    // --- Benches against the buildings, facing inward. ---
    rng.shuffle(&mut seat);
    let bench_target = (region.area / 40).clamp(1, 4);
    let mut benches: Vec<Point2D> = Vec::new();
    for &c in &seat {
        if benches.len() >= bench_target {
            break;
        }
        if used.contains(&c) || benches.iter().any(|b| chebyshev(*b, c) < 4) {
            continue;
        }
        if let Some(inward) = inward_dir(world, c, &cells) {
            place_bench(editor, c, height_at(c), inward).await;
            used.insert(c);
            benches.push(c);
        }
    }

    // --- Lantern posts along the perimeter. ---
    rng.shuffle(&mut perimeter);
    let lamp_target = (region.area / 50).max(1);
    let mut lamps: Vec<Point2D> = Vec::new();
    for &c in &perimeter {
        if lamps.len() >= lamp_target {
            break;
        }
        if used.contains(&c) || lamps.iter().any(|l| chebyshev(*l, c) < 6) {
            continue;
        }
        place_lantern_post(editor, c, height_at(c)).await;
        used.insert(c);
        lamps.push(c);
    }
}
