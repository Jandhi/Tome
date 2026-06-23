//! Shared open-space props: small placers and cell predicates reused across the
//! per-type furnishers (nook, plaza, …). All placement is in world coordinates;
//! `h` is the first air cell above the surface (surface block sits at `h - 1`).

use std::collections::{HashMap, HashSet, VecDeque};

use crate::editor::{Editor, World};
use crate::generator::terrain::{generate_tree_feature, Tree};
use crate::generator::BuildClaim;
use crate::geometry::{Point2D, Point3D, CARDINALS_2D};
use crate::minecraft::{string_to_block, Biome};
use crate::noise::RNG;

use super::theme::Theme;

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

/// Orthogonal distance of each region cell from the region edge (0 = a cell on
/// the perimeter), via a multi-source BFS inward. Shared by the open-space
/// furnishers to taper edge effects (e.g. the flatten lerp).
pub(super) fn edge_depth(cells: &HashSet<Point2D>) -> HashMap<Point2D, i32> {
    let mut depth: HashMap<Point2D, i32> = HashMap::new();
    let mut queue: VecDeque<Point2D> = VecDeque::new();
    for &c in cells {
        if CARDINALS_2D.iter().any(|d| !cells.contains(&(c + *d))) {
            depth.insert(c, 0);
            queue.push_back(c);
        }
    }
    while let Some(c) = queue.pop_front() {
        let dc = depth[&c];
        for d in CARDINALS_2D {
            let n = c + d;
            if cells.contains(&n) && !depth.contains_key(&n) {
                depth.insert(n, dc + 1);
                queue.push_back(n);
            }
        }
    }
    depth
}

/// Edge-taper weight for flattening, by distance from the region edge: the two
/// outermost rings only partly level toward the flat target, so the surface
/// eases into the surrounding ground instead of dropping off a cliff. `1.0` =
/// fully flat.
pub(super) fn flatten_blend(depth: i32) -> f32 {
    match depth {
        0 => 0.34, // outermost ring: mostly natural
        1 => 0.67, // second ring: partway
        _ => 1.0,  // interior: fully flat
    }
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

/// A small tree species. A desert-*style* settlement always grows small jungle
/// trees (matching its warm palette) whatever the biome; every other style
/// picks a small, biome-appropriate species, or `None` for biomes where a tree
/// looks out of place (desert/badlands/etc.). All returned variants have a
/// palette in the `small_mixed` forest.
pub(super) fn biome_tree(theme: &Theme, biome: &Biome, rng: &mut RNG) -> Option<Tree> {
    if theme.arid {
        let weights = vec![(Tree::SmallJungle, 4.0), (Tree::MediumJungle, 1.0)];
        return Some(*rng.choose_weighted_vec(&weights));
    }
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

/// Place a biome-appropriate small vanilla tree at `c` (grown server-side via
/// `place feature`). Returns `false` (placing nothing) for treeless biomes.
pub(super) async fn place_tree(
    editor: &Editor,
    theme: &Theme,
    biome: &Biome,
    c: Point2D,
    h: i32,
    rng: &mut RNG,
) -> bool {
    let Some(tree) = biome_tree(theme, biome, rng) else {
        return false;
    };
    lay_soil_patch(editor, c, h).await;
    let _ = generate_tree_feature(tree, editor, Point3D::new(c.x, h, c.y), rng).await;
    true
}

/// Lay grassy soil at a cell's surface (`h - 1`) so flowers and tree trunks sit
/// on grass even in a sand-floored (desert) open space.
pub(super) async fn lay_soil(editor: &Editor, c: Point2D, h: i32) {
    put_forced(editor, c.x, h - 1, c.y, "minecraft:grass_block").await;
}

/// Like [`lay_soil`] but a small plus-shaped patch (the cell + its cardinal
/// neighbours), so a tree reads as planted in soil rather than on a lone tile.
pub(super) async fn lay_soil_patch(editor: &Editor, c: Point2D, h: i32) {
    lay_soil(editor, c, h).await;
    for d in CARDINALS_2D {
        let n = c + d;
        let hn = editor.world().get_ocean_floor_height_at(n);
        lay_soil(editor, n, hn).await;
    }
}

/// A two-wide bench (`wood` stairs) backed against a wall, seat facing `inward`.
///
/// A stair's solid riser sits on the side *opposite* its `facing`, so the bench
/// faces away from the seated occupant: we face it outward (toward the wall) so
/// the riser lands on the open side as a backrest. (Facing it `inward` puts the
/// backrest against the wall and the seat the wrong way round.) A second stair is
/// laid alongside, perpendicular to `inward`, so the bench is two blocks wide.
pub(super) async fn place_bench(editor: &Editor, c: Point2D, h: i32, inward: Point2D, wood: &str) {
    let outward = Point2D::new(-inward.x, -inward.y);
    let block = string_to_block(&format!("minecraft:{}_stairs[facing={}]", wood, cardinal_facing(outward)))
        .expect("bench stair block");
    // Extend one cell along the wall (perpendicular to inward) for a two-wide seat.
    let along = Point2D::new(-inward.y, inward.x);
    for cell in [c, c + along] {
        editor.place_block(&block, Point3D::new(cell.x, h, cell.y)).await;
    }
}

/// A planter: a `wood` base with a leafy azalea on top.
pub(super) async fn place_planter(editor: &Editor, c: Point2D, h: i32, wood: &str) {
    put(editor, c.x, h, c.y, &format!("minecraft:{}_planks", wood)).await;
    editor.place_block(&"minecraft:azalea".into(), Point3D::new(c.x, h + 1, c.y)).await;
}

/// A lantern on a `wood` fence post.
pub(super) async fn place_lantern_post(editor: &Editor, c: Point2D, h: i32, wood: &str) {
    put(editor, c.x, h, c.y, &format!("minecraft:{}_fence", wood)).await;
    editor.place_block(&"minecraft:lantern".into(), Point3D::new(c.x, h + 1, c.y)).await;
}
