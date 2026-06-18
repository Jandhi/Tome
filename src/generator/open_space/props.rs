//! Shared open-space props: small placers and cell predicates reused across the
//! per-type furnishers (nook, plaza, …). All placement is in world coordinates;
//! `h` is the first air cell above the surface (surface block sits at `h - 1`).

use std::collections::HashSet;

use crate::editor::{Editor, World};
use crate::generator::terrain::{generate_tree, Forest, Tree};
use crate::generator::BuildClaim;
use crate::geometry::{Point2D, Point3D, CARDINALS_2D};
use crate::minecraft::{string_to_block, Biome};
use crate::noise::RNG;

/// Place a single block from an id string (`"id"` or `"id[state=…]"`).
pub(super) async fn put(editor: &Editor, x: i32, y: i32, z: i32, id: &str) {
    let block = string_to_block(id).unwrap_or_else(|| panic!("bad block id: {id}"));
    editor.place_block(&block, Point3D::new(x, y, z)).await;
}

/// Like [`put`] but forced — overrides whatever is already there.
pub(super) async fn put_forced(editor: &Editor, x: i32, y: i32, z: i32, id: &str) {
    let block = string_to_block(id).unwrap_or_else(|| panic!("bad block id: {id}"));
    editor.place_block_forced(&block, Point3D::new(x, y, z)).await;
}

/// Chebyshev (chessboard) distance between two cells — used for prop spacing.
pub(super) fn chebyshev(a: Point2D, b: Point2D) -> i32 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

/// Is this claim a building wall a bench can back against?
pub(super) fn is_building(claim: Option<&BuildClaim>) -> bool {
    matches!(
        claim,
        Some(BuildClaim::Building(_) | BuildClaim::Structure(_) | BuildClaim::ProductionArea(_))
    )
}

/// Is this claim a road we must not block?
pub(super) fn is_path(claim: Option<&BuildClaim>) -> bool {
    matches!(claim, Some(BuildClaim::Path(_) | BuildClaim::PathPlanned(_)))
}

/// Minecraft stair `facing` value for a cardinal step (the direction the seat
/// opens toward).
pub(super) fn cardinal_facing(dir: Point2D) -> &'static str {
    match (dir.x, dir.y) {
        (0, -1) => "north",
        (0, 1) => "south",
        (1, 0) => "east",
        _ => "west",
    }
}

/// For a cell against a building, the inward direction a seat should face (away
/// from the wall, into the open space). `None` if there's no building to back
/// against or the open side isn't part of the region.
pub(super) fn inward_dir(world: &World, c: Point2D, cells: &HashSet<Point2D>) -> Option<Point2D> {
    for d in CARDINALS_2D {
        if is_building(world.get_claim(c + d).as_ref()) {
            let inward = Point2D::new(-d.x, -d.y);
            if cells.contains(&(c + inward)) {
                return Some(inward);
            }
        }
    }
    None
}

/// A small, biome-appropriate tree species, or `None` for biomes where a tree
/// looks out of place (desert/badlands/etc.). All returned variants have a
/// palette in the `small_mixed` forest.
pub(super) fn biome_tree(biome: &Biome, rng: &mut RNG) -> Option<Tree> {
    let n = biome.name();
    let weights: Vec<(Tree, f32)> = if n.contains("birch") {
        vec![(Tree::SmallBirch, 4.0), (Tree::SmallOak, 1.0)]
    } else if n.contains("taiga")
        || n.contains("spruce")
        || n.contains("pine")
        || n.contains("grove")
        || n.contains("snowy")
        || n.contains("frozen")
    {
        vec![(Tree::SmallPine, 4.0), (Tree::SmallHedge, 1.0)]
    } else if n.contains("jungle") || n.contains("swamp") || n.contains("mangrove") {
        vec![(Tree::SmallJungle, 4.0), (Tree::SmallOak, 1.0)]
    } else if n.contains("desert")
        || n.contains("badlands")
        || n.contains("beach")
        || n.contains("ocean")
    {
        return None;
    } else {
        vec![
            (Tree::SmallOak, 4.0),
            (Tree::SmallHedge, 2.0),
            (Tree::SmallBirch, 1.0),
        ]
    };
    Some(*rng.choose_weighted_vec(&weights))
}

/// Place a biome-appropriate small tree at `c`. Returns `false` (placing
/// nothing) for treeless biomes or a missing palette.
pub(super) async fn place_tree(
    editor: &Editor,
    forest: &Forest,
    biome: &Biome,
    c: Point2D,
    h: i32,
    rng: &mut RNG,
) -> bool {
    let Some(tree) = biome_tree(biome, rng) else {
        return false;
    };
    let Some(palette) = forest.tree_palette().get(&tree) else {
        return false;
    };
    generate_tree(tree, editor, Point3D::new(c.x, h, c.y), rng, palette).await;
    true
}

/// A bench (oak stairs) backed against a wall, seat facing `inward`.
pub(super) async fn place_bench(editor: &Editor, c: Point2D, h: i32, inward: Point2D) {
    let block = string_to_block(&format!("minecraft:oak_stairs[facing={}]", cardinal_facing(inward)))
        .expect("bench stair block");
    editor.place_block(&block, Point3D::new(c.x, h, c.y)).await;
}

/// A planter: a wood base with a leafy azalea on top.
pub(super) async fn place_planter(editor: &Editor, c: Point2D, h: i32) {
    editor.place_block(&"minecraft:oak_planks".into(), Point3D::new(c.x, h, c.y)).await;
    editor.place_block(&"minecraft:azalea".into(), Point3D::new(c.x, h + 1, c.y)).await;
}

/// A lantern on a fence post.
pub(super) async fn place_lantern_post(editor: &Editor, c: Point2D, h: i32) {
    editor.place_block(&"minecraft:oak_fence".into(), Point3D::new(c.x, h, c.y)).await;
    editor.place_block(&"minecraft:lantern".into(), Point3D::new(c.x, h + 1, c.y)).await;
}
