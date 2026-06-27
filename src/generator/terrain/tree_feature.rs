//! Vanilla tree generation via the server's `place feature` command. Instead of
//! placing logs/leaves ourselves (see [`generate_tree`](super::generate_tree)),
//! this asks Minecraft to grow one of its own configured tree features, so the
//! result matches what worldgen produces in the wild.
//!
//! Trade-offs versus the hand-authored generator: the feature is grown
//! server-side by vanilla, so it uses **vanilla wood/leaf blocks** (our forest
//! palettes don't apply), it's seeded by the **world**, not our RNG (so it isn't
//! reproducible from our seed), and it **bypasses the editor's block cache**
//! (so it's invisible to later `get_block` reads and to offline/dry-run mode).

use crate::{editor::Editor, geometry::Point3D, noise::RNG};

use super::Tree;

/// The vanilla configured-feature id (`minecraft:` namespace) that best matches
/// each [`Tree`] species/size, for use with `place feature`.
pub fn tree_feature_id(tree: Tree) -> &'static str {
    match tree {
        // Oak: plain for small, the big branchy "fancy oak" for large/mega.
        Tree::SmallOak | Tree::MediumOak => "minecraft:oak",
        Tree::LargeOak | Tree::MegaOak => "minecraft:fancy_oak",

        // Birch: plain, and the tall variant for the big ones.
        Tree::SmallBirch | Tree::MediumBirch => "minecraft:birch",
        Tree::LargeBirch | Tree::MegaBirch => "minecraft:super_birch_bees_0002",

        // Pine / spruce: conifers, with the 2×2 giant for mega.
        Tree::SmallPine | Tree::MediumPine => "minecraft:spruce",
        Tree::LargePine => "minecraft:pine",
        Tree::MegaPine => "minecraft:mega_spruce",

        // Jungle: standard tree, 2×2 giant for mega.
        Tree::SmallJungle | Tree::MediumJungle | Tree::LargeJungle => "minecraft:jungle_tree",
        Tree::MegaJungle => "minecraft:mega_jungle_tree",

        // Hedge: no vanilla hedge — an azalea tree is the closest small, leafy shrub.
        Tree::SmallHedge | Tree::MediumHedge | Tree::LargeHedge | Tree::MegaHedge => {
            "minecraft:azalea_tree"
        }

        // Baobab: vanilla has no baobab — acacia is the nearest flat-crowned tree.
        Tree::SmallBaobab | Tree::MediumBaobab | Tree::LargeBaobab | Tree::MegaBaobab => {
            "minecraft:acacia"
        }

        // Cherry blossom: the one vanilla cherry feature for every size.
        Tree::SmallCherry | Tree::MediumCherry | Tree::LargeCherry => "minecraft:cherry",

        // Cactus has no vanilla tree feature — `generate_tree_feature` intercepts
        // it and builds a column directly, so this id is never actually queried.
        Tree::Cactus => "minecraft:cactus",
    }
}

/// Build a short cactus column (1–3 tall) on a sand footing, in place of a
/// vanilla feature (there's no cactus equivalent). Height is rolled from `rng`.
async fn place_cactus_column(editor: &Editor, point: Point3D, rng: &mut RNG) {
    let (x, y, z) = (point.x, point.y, point.z);
    editor
        .place_block_forced(&"minecraft:sand".into(), Point3D { x, y: y - 1, z })
        .await;
    let height = 1 + rng.rand_i32(3); // 1..=3
    for i in 0..height {
        editor
            .place_block(&"minecraft:cactus".into(), Point3D { x, y: y + i, z })
            .await;
    }
}

/// Grow a vanilla tree feature for `tree` at `point` (build-area-local
/// coordinates) by asking the server to `place feature`. `rng` is only consumed
/// by the cactus path (which we build ourselves); vanilla features are seeded by
/// the world.
pub async fn generate_tree_feature(
    tree: Tree,
    editor: &Editor,
    point: Point3D,
    rng: &mut RNG,
) -> anyhow::Result<()> {
    // Cactus has no vanilla `place feature` — build the column ourselves.
    if matches!(tree, Tree::Cactus) {
        place_cactus_column(editor, point, rng).await;
        return Ok(());
    }
    editor.place_feature(tree_feature_id(tree), point).await
}
