use serde_derive::{Deserialize, Serialize};
use std::{collections::HashMap};

use crate::{data::Loadable, editor::Editor, generator::materials::MaterialId, geometry::Point3D, noise::{RNG, Seed}, minecraft::{string_to_block,Block}};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tree {
    #[serde(rename = "mega_birch")]
    MegaBirch,
    #[serde(rename = "large_birch")]
    LargeBirch,
    #[serde(rename = "medium_birch")]
    MediumBirch,
    #[serde(rename = "small_birch")]
    SmallBirch,
    #[serde(rename = "mega_jungle")]
    MegaJungle,
    #[serde(rename = "large_jungle")]
    LargeJungle,
    #[serde(rename = "medium_jungle")]
    MediumJungle,
    #[serde(rename = "small_jungle")]
    SmallJungle,
    #[serde(rename = "mega_pine")]
    MegaPine,
    #[serde(rename = "large_pine")]
    LargePine,
    #[serde(rename = "medium_pine")]
    MediumPine,
    #[serde(rename = "small_pine")]
    SmallPine,
    #[serde(rename = "mega_hedge")]
    MegaHedge,
    #[serde(rename = "large_hedge")]
    LargeHedge,
    #[serde(rename = "medium_hedge")]
    MediumHedge,
    #[serde(rename = "small_hedge")]
    SmallHedge,
    #[serde(rename = "mega_baobab")]
    MegaBaobab,
    #[serde(rename = "large_baobab")]
    LargeBaobab,
    #[serde(rename = "medium_baobab")]
    MediumBaobab,
    #[serde(rename = "small_baobab")]
    SmallBaobab,
    #[serde(rename = "mega_oak")]
    MegaOak,
    #[serde(rename = "large_oak")]
    LargeOak,
    #[serde(rename = "medium_oak")]
    MediumOak,
    #[serde(rename = "small_oak")]
    SmallOak,
}

pub async fn generate_tree(
    tree: Tree,
    editor: &mut Editor,
    point: Point3D,
    rng: &mut RNG,
    palette: &HashMap<String, HashMap<String, f32>>,
) {
    let new_seed = Seed(rng.next());
    let mut new_rng: RNG = RNG::new(new_seed);
    let wood = new_rng.choose_weighted(palette.get("wood").expect("Wood palette not found"));
    let leaves = new_rng.choose_weighted(palette.get("leaves").expect("Leaves palette not found"));
    let wood_block = string_to_block(wood.as_str()).expect("Failed to convert wood to block");
    let leaf_block = string_to_block(leaves.as_str()).expect("Failed to convert leaves to block");
    println!("Leaf block: {:?}, Wood: {:?}", leaf_block, wood_block);

    match tree {
        Tree::MegaBirch => {
            // Generate a mega birch tree
            generate_mega_birch(editor, point, &wood_block, &leaf_block, rng, 100).await
        }
        Tree::LargeBirch => {
            // Generate a large birch tree
            generate_large_birch(editor, point, &wood_block, &leaf_block, rng, 95).await;
        }
        Tree::MediumBirch => {
            // Generate a medium birch tree
            generate_medium_birch(editor, point, &wood_block, &leaf_block, rng, 96).await;
        }
        Tree::SmallBirch => {
            // Generate a small birch tree
            generate_small_birch(editor, point, &wood_block, &leaf_block, rng, 98).await;
        }
        Tree::MegaJungle => {
            // Generate a mega jungle tree
            // Placeholder for actual implementation
        }
        Tree::LargeJungle => {
            // Generate a large jungle tree
            // Placeholder for actual implementation
        }
        Tree::MediumJungle => {
            // Generate a medium jungle tree
            // Placeholder for actual implementation
        }
        Tree::SmallJungle => {
            // Generate a small jungle tree
            // Placeholder for actual implementation
        }
        Tree::MegaPine => {
            // Generate a mega pine tree
            generate_mega_pine(editor, point, &wood_block, &leaf_block, rng, 95).await;
        }
        Tree::LargePine => {
            // Generate a large pine tree
            generate_large_pine(editor, point, &wood_block, &leaf_block, rng, 95).await;
        }
        Tree::MediumPine => {
            // Generate a medium pine tree
            generate_medium_pine(editor, point, &wood_block, &leaf_block, rng, 95).await;
        }
        Tree::SmallPine => {
            // Generate a small pine tree
            generate_small_pine(editor, point, &wood_block, &leaf_block, rng, 100).await;
        }
        Tree::MegaHedge => {
            // Generate a mega hedge
            generate_mega_hedge(editor, point, &wood_block, &leaf_block, rng, 98).await
        }
        Tree::LargeHedge => {
            // Generate a large hedge
            generate_large_hedge(editor, point, &wood_block, &leaf_block, rng, 100).await
        }
        Tree::MediumHedge => {
            // Generate a medium hedge
            generate_medium_hedge(editor, point, &wood_block, &leaf_block, rng, 100).await
        }
        Tree::SmallHedge => {
            // Generate a small hedge
            generate_small_hedge(editor, point, &wood_block, &leaf_block, rng, 100).await
        }
        Tree::MegaBaobab => {
            // Generate a mega baobab tree
            // Placeholder for actual implementation
        }
        Tree::LargeBaobab => {
            // Generate a large baobab tree
            // Placeholder for actual implementation
        }
        Tree::MediumBaobab => {
            // Generate a medium baobab tree
            // Placeholder for actual implementation
        }
        Tree::SmallBaobab => {
            // Generate a small baobab tree
            // Placeholder for actual implementation
        }
        Tree::MegaOak => {
            // Generate a mega oak tree
            generate_mega_oak(editor, point, &wood_block, &leaf_block, rng, 96).await
        }
        Tree::LargeOak => {
            // Generate a large oak tree
            generate_large_oak(editor, point, &wood_block, &leaf_block, rng, 100).await
        }
        Tree::MediumOak => {
            // Generate a medium oak tree
            generate_medium_oak(editor, point, &wood_block, &leaf_block, rng, 100).await
        }
        Tree::SmallOak => {
            // Generate a small oak tree
            generate_small_oak(editor, point, &wood_block, &leaf_block, rng, 100).await
        }
    }
}


async fn generate_small_birch(
    editor: &mut Editor,
    point: Point3D,
    wood: &Block,
    leaf: &Block,
    rng: &mut RNG,
    leaf_chance: i32,
) {
    let (x0, y0, z0) = (point.x, point.y, point.z);
    let height = rng.rand_i32_range(5, 10) + y0; // rng.range is [start, end), so 10 for inclusive 9

    for y in y0..=(height + 2) {
        if y >= height {
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 }, rng, leaf_chance).await;
        } else {
            editor.place_block_chance(wood, Point3D { x: x0, y, z: z0 }, rng, leaf_chance).await;
        }
        let mid = ((height - y0) / 2 + y0) - 1;
        if y == mid || y == height + 1 {
            editor.place_block_chance(leaf, Point3D { x: x0 + 1, y, z: z0 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 - 1, y, z: z0 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 + 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 - 1 }, rng, leaf_chance).await;
        } else if y > mid && y < height + 2 {
            editor.place_block_chance(leaf, Point3D { x: x0 + 1, y, z: z0 + 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 - 1, y, z: z0 - 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 - 1, y, z: z0 + 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 + 1, y, z: z0 - 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 + 1, y, z: z0 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 - 1, y, z: z0 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 + 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 - 1 }, rng, leaf_chance).await;
        }
    }
}

async fn generate_medium_birch(
    editor: &mut Editor,
    point: Point3D,
    wood: &Block,
    leaf: &Block,
    rng: &mut RNG,
    leaf_chance: i32,
) {
    let (x0, y0, z0) = (point.x, point.y, point.z);
    let height = rng.rand_i32_range(10, 16) + y0;
    let branch_num = rng.rand_i32_range(2, 6); // 6 is exclusive, so 5 is inclusive

    // Store branch positions
    let mut branches = Vec::new();
    branches.push((x0, height, z0));

    // Trunk and base
    for y in y0..height {
        editor.place_block(wood, Point3D { x: x0, y, z: z0 }).await;
        if y == y0 {
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if rng.rand_i32_range(1, 5) != 4 {
                    editor.place_block(wood, Point3D { x: x0 + dx, y, z: z0 + dz }).await;
                }
            }
        }
    }

    // Branches
    for _ in 0..branch_num {
        let branch_height = rng.rand_i32_range(((height - y0) / 2 + y0) + 2, height);
        let branch_pos = rng.rand_i32_range(1, 17); // 17 is exclusive, so 16 is inclusive

        match branch_pos {
            1 => {
                branches.push((x0 + 2, branch_height, z0));
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 }).await;
            }
            2 => {
                branches.push((x0 + 2, branch_height, z0 + 1));
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height, z: z0 + 1 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 + 1 }).await;
                if rng.rand_i32_range(1, 3) == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 + 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 }).await;
                }
            }
            3 => {
                branches.push((x0 + 2, branch_height, z0 + 2));
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 + 1 }).await;
            }
            4 => {
                branches.push((x0 + 2, branch_height, z0 - 1));
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height, z: z0 - 1 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 - 1 }).await;
                if rng.rand_i32_range(1, 3) == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 - 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 }).await;
                }
            }
            5 => {
                branches.push((x0 + 2, branch_height, z0 - 2));
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 - 1 }).await;
            }
            6 => {
                branches.push((x0 - 2, branch_height, z0));
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 }).await;
            }
            7 => {
                branches.push((x0 - 2, branch_height, z0 + 1));
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height, z: z0 + 1 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 + 1 }).await;
                if rng.rand_i32_range(1, 3) == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 + 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 }).await;
                }
            }
            8 => {
                branches.push((x0 - 2, branch_height, z0 + 2));
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 + 1 }).await;
            }
            9 => {
                branches.push((x0 - 2, branch_height, z0 - 1));
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height, z: z0 - 1 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 - 1 }).await;
                if rng.rand_i32_range(1, 3) == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 - 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 }).await;
                }
            }
            10 => {
                branches.push((x0 - 2, branch_height, z0 - 2));
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 - 1 }).await;
            }
            11 => {
                branches.push((x0 + 1, branch_height, z0 - 2));
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 - 2 }).await;
                if rng.rand_i32_range(1, 3) == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 - 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 - 1 }).await;
                }
            }
            12 => {
                branches.push((x0, branch_height, z0 - 2));
                editor.place_block(wood, Point3D { x: x0, y: branch_height, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 1, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 - 1 }).await;
            }
            13 => {
                branches.push((x0 - 1, branch_height, z0 - 2));
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 - 2 }).await;
                if rng.rand_i32_range(1, 3) == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 - 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 - 1 }).await;
                }
            }
            14 => {
                branches.push((x0 + 1, branch_height, z0 + 2));
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 + 2 }).await;
                if rng.rand_i32_range(1, 3) == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 + 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 + 1 }).await;
                }
            }
            15 => {
                branches.push((x0, branch_height, z0 + 2));
                editor.place_block(wood, Point3D { x: x0, y: branch_height, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 1, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 + 1 }).await;
            }
            16 => {
                branches.push((x0 - 1, branch_height, z0 + 2));
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 + 2 }).await;
                if rng.rand_i32_range(1, 3) == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 + 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 + 1 }).await;
                }
            }
            _ => {}
        }
    }

    // Leaves for each branch
    for &(x1, y1, z1) in &branches {
        // Top leaf
        editor.place_block_chance(leaf, Point3D { x: x1, y: y1 + 3, z: z1 }, rng, leaf_chance).await;

        // Second layer
        for (dx, dz) in &[(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
            editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1 + 2, z: z1 + dz }, rng, leaf_chance).await;
        }

        // Third layer
        let third_layer = [
            (0, 0), (1, 1), (-1, -1), (-1, 1), (1, -1),
            (1, 0), (-1, 0), (0, 1), (0, -1),
        ];
        for (dx, dz) in &third_layer {
            editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1 + 1, z: z1 + dz }, rng, leaf_chance).await;
        }

        // Fourth layer
        let fourth_layer = [
            (1, 1), (-1, -1), (-1, 1), (1, -1),
            (1, 0), (-1, 0), (0, 1), (0, -1),
        ];
        for (dx, dz) in &fourth_layer {
            editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1, z: z1 + dz }, rng, leaf_chance).await;
        }

        // Fifth layer (below)
        for (dx, dz) in &[(1, 0), (-1, 0), (0, 1), (0, -1)] {
            editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1 - 1, z: z1 + dz }, rng, leaf_chance).await;
        }
    }
}

async fn generate_large_birch(
    editor: &mut Editor,
    point: Point3D,
    wood: &Block,
    leaf: &Block,
    rng: &mut RNG,
    leaf_chance: i32,
) {
    let (x0, y0, z0) = (point.x, point.y, point.z);
    let height = rng.rand_i32_range(16, 24) + y0; // 24 exclusive, so 23 inclusive
    let branch_num = rng.rand_i32_range(3, 8); // 8 exclusive, so 7 inclusive

    let mut branches = Vec::new();
    branches.push((x0, height, z0));

    // Trunk and base
    for y in y0..height {
        editor.place_block(wood, Point3D { x: x0, y, z: z0 }).await;
        if y == y0 {
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if rng.rand_i32_range(1, 5) != 4 {
                    editor.place_block(wood, Point3D { x: x0 + dx, y, z: z0 + dz }).await;
                }
            }
        }
    }

    for _ in 0..branch_num {
        let branch_height = rng.rand_i32_range(((height - y0) / 2 + y0) + 4, height);
        let branch_pos = rng.rand_i32_range(1, 25); // 25 exclusive, so 24 inclusive

        match branch_pos {
            1 => {
                branches.push((x0 + 3, branch_height, z0));
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 1, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 3, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 }).await;
            }
            2 => {
                branches.push((x0 + 3, branch_height, z0 + 1));
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height, z: z0 + 1 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 1, z: z0 + 1 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 + 1 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 3, z: z0 + 1 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 + 1 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 3, z: z0 + 1 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 3, z: z0 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 }).await;
                }
            }
            3 => {
                branches.push((x0 + 3, branch_height, z0 + 3));
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 1, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 3, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 + 1 }).await;
            }
            4 => {
                branches.push((x0 + 3, branch_height, z0 - 1));
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height, z: z0 - 1 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 1, z: z0 - 1 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 - 1 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 3, z: z0 - 1 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 - 1 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 3, z: z0 - 1 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 3, z: z0 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 }).await;
                }
            }
            5 => {
                branches.push((x0 + 3, branch_height, z0 - 3));
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 1, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 3, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 - 1 }).await;
            }
            6 => {
                branches.push((x0 - 3, branch_height, z0));
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 1, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 3, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 }).await;
            }
            7 => {
                branches.push((x0 - 3, branch_height, z0 + 1));
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height, z: z0 + 1 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 1, z: z0 + 1 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 + 1 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 3, z: z0 + 1 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 + 1 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 3, z: z0 + 1 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 3, z: z0 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 }).await;
                }
            }
            8 => {
                branches.push((x0 - 3, branch_height, z0 + 3));
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 1, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 3, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 + 1 }).await;
            }
            9 => {
                branches.push((x0 - 3, branch_height, z0 - 1));
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height, z: z0 - 1 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 1, z: z0 - 1 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 - 1 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 3, z: z0 - 1 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 - 1 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 3, z: z0 - 1 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 3, z: z0 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 }).await;
                }
            }
            10 => {
                branches.push((x0 - 3, branch_height, z0 - 3));
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 1, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 3, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 - 1 }).await;
            }
            11 => {
                branches.push((x0 + 1, branch_height, z0 - 3));
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 - 3 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 3, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 - 1 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 3, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 4, z: z0 - 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 3, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 4, z: z0 - 1 }).await;
                }
            }
            12 => {
                branches.push((x0, branch_height, z0 - 3));
                editor.place_block(wood, Point3D { x: x0, y: branch_height, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 1, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 3, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 4, z: z0 - 1 }).await;
            }
            13 => {
                branches.push((x0 - 1, branch_height, z0 - 3));
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 - 3 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 3, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 - 1 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 3, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 4, z: z0 - 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 3, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 4, z: z0 - 1 }).await;
                }
            }
            14 => {
                branches.push((x0 + 1, branch_height, z0 + 3));
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 + 3 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 3, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 + 1 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 3, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 4, z: z0 + 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 3, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 4, z: z0 + 1 }).await;
                }
            }
            15 => {
                branches.push((x0, branch_height, z0 + 3));
                editor.place_block(wood, Point3D { x: x0, y: branch_height, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 1, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 3, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 4, z: z0 + 1 }).await;
            }
            16 => {
                branches.push((x0 - 1, branch_height, z0 + 3));
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 + 3 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 3, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 + 1 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 3, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 4, z: z0 + 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 3, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 4, z: z0 + 1 }).await;
                }
            }
            17 => {
                branches.push((x0 + 3, branch_height, z0 + 2));
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 1, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 + 2 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 3, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 + 1 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 3, z: z0 + 1 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 + 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 3, z: z0 + 1 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 }).await;
                }
            }
            18 => {
                branches.push((x0 + 3, branch_height, z0 - 2));
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 1, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 - 2 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 3, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 - 1 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 3, z: z0 - 1 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 - 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 3, z: z0 - 1 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 }).await;
                }
            }
            19 => {
                branches.push((x0 - 3, branch_height, z0 + 2));
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 1, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 + 2 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 3, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 + 1 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 3, z: z0 + 1 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 + 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 3, z: z0 + 1 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 }).await;
                }
            }
            20 => {
                branches.push((x0 - 3, branch_height, z0 - 2));
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 1, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 - 2 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 3, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 - 1 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 3, z: z0 - 1 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 - 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 3, z: z0 - 1 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 }).await;
                }
            }
            21 => {
                branches.push((x0 + 2, branch_height, z0 + 3));
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 + 3 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 3, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 + 1 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 3, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 + 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 3, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 4, z: z0 + 1 }).await;
                }
            }
            22 => {
                branches.push((x0 - 2, branch_height, z0 + 3));
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 + 3 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 3, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 + 1 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 3, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 + 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 3, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 4, z: z0 + 1 }).await;
                }
            }
            23 => {
                branches.push((x0 + 2, branch_height, z0 - 3));
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 - 3 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 3, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 - 1 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 3, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 4, z: z0 - 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 3, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 4, z: z0 - 1 }).await;
                }
            }
            24 => {
                branches.push((x0 - 2, branch_height, z0 - 3));
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 - 3 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 3, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 - 1 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 3, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 4, z: z0 - 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 3, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 4, z: z0 - 1 }).await;
                }
            }
            _ => {}
        }
    }

    // Leaves for each branch
    for &(x1, y1, z1) in &branches {
        // Top leaf
        editor.place_block_chance(leaf, Point3D { x: x1, y: y1 + 4, z: z1 }, rng, leaf_chance).await;

        // 2nd layer
        for (dx, dz) in &[
            (0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)
        ] {
            editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1 + 3, z: z1 + dz }, rng, leaf_chance).await;
        }

        // 3rd layer
        let third_layer = [
            (0, 0), (1, 1), (-1, -1), (-1, 1), (1, -1),
            (1, 0), (-1, 0), (0, 1), (0, -1),
            (2, 0), (-2, 0), (0, 2), (0, -2)
        ];
        for (dx, dz) in &third_layer {
            editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1 + 2, z: z1 + dz }, rng, leaf_chance).await;
        }

        // 4th layer
        let fourth_layer = [
            (1, 1), (-1, -1), (-1, 1), (1, -1),
            (1, 0), (-1, 0), (0, 1), (0, -1),
            (2, 0), (-2, 0), (0, 2), (0, -2),
            (2, 1), (-2, -1), (-2, 1), (2, -1),
            (1, 2), (-1, -2), (-1, 2), (1, -2)
        ];
        for (dx, dz) in &fourth_layer {
            editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1 + 1, z: z1 + dz }, rng, leaf_chance).await;
        }

        // 5th layer
        let fifth_layer = [
            (1, 1), (-1, -1), (-1, 1), (1, -1),
            (1, 0), (-1, 0), (0, 1), (0, -1),
            (2, 0), (-2, 0), (0, 2), (0, -2)
        ];
        for (dx, dz) in &fifth_layer {
            editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1, z: z1 + dz }, rng, leaf_chance).await;
        }

        // 6th layer (below)
        for (dx, dz) in &[
            (1, 0), (-1, 0), (0, 1), (0, -1),
            (1, 1), (-1, -1), (-1, 1), (1, -1)
        ] {
            editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1 - 1, z: z1 + dz }, rng, leaf_chance).await;
        }
        for (dx, dz) in &[
            (1, 0), (-1, 0), (0, 1), (0, -1)
        ] {
            editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1 - 2, z: z1 + dz }, rng, leaf_chance).await;
        }
    }
}

async fn generate_mega_birch(
    editor: &mut Editor,
    point: Point3D,
    wood: &Block,
    leaf: &Block,
    rng: &mut RNG,
    leaf_chance: i32,
) {
    // Place 4 large birches in a 2x2 grid
    let offsets = [(0, 0), (1, 0), (1, 1), (0, 1)];
    for (dx, dz) in offsets {
        let new_point = Point3D { x: point.x + dx, y: point.y, z: point.z + dz };
        generate_large_birch(editor, new_point, wood, leaf, rng, leaf_chance).await;
    }
}


async fn generate_small_pine(
    editor: &mut Editor,
    point: Point3D,
    wood: &Block,
    leaf: &Block,
    rng: &mut RNG,
    leaf_chance: i32,
) {
    let (x0, y0, z0) = (point.x, point.y, point.z);
    let height = rng.rand_i32_range(5, 8) + y0; // 8 exclusive, so 7 inclusive

    for y in y0..=(height + 1) {
        if y == height + 1 {
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 }, rng, leaf_chance).await;
            continue;
        }
        editor.place_block(wood, Point3D { x: x0, y, z: z0 }).await;

        if y % 2 == height % 2 && y > y0 + 1 {
            let positions = [
                (1, 0), (-1, 0), (0, 1), (0, -1)
            ];
            for (dx, dz) in positions {
                editor.place_block_chance(leaf, Point3D { x: x0 + dx, y, z: z0 + dz }, rng, leaf_chance).await;
            }
        } else if y == height - 1 {
            let positions = [
                (1, 1), (-1, -1), (-1, 1), (1, -1),
                (1, 0), (-1, 0), (0, 1), (0, -1)
            ];
            for (dx, dz) in positions {
                editor.place_block_chance(leaf, Point3D { x: x0 + dx, y, z: z0 + dz }, rng, leaf_chance).await;
            }
        } else if y % 2 == (height - 1) % 2 && y > y0 {
            let positions = [
                (2, 0), (-2, 0), (0, 2), (0, -2),
                (1, 1), (-1, -1), (-1, 1), (1, -1),
                (1, 0), (-1, 0), (0, 1), (0, -1)
            ];
            for (dx, dz) in positions {
                editor.place_block_chance(leaf, Point3D { x: x0 + dx, y, z: z0 + dz }, rng, leaf_chance).await;
            }
        }
    }
}

async fn generate_medium_pine(
    editor: &mut Editor,
    point: Point3D,
    wood: &Block,
    leaf: &Block,
    rng: &mut RNG,
    leaf_chance: i32,
) {
    let (x0, y0, z0) = (point.x, point.y, point.z);
    let height = rng.rand_i32_range(8, 14) + y0; // 14 is exclusive, so 13 is inclusive

    for y in y0..=(height + 1) {
        if y == height + 1 {
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 }, rng, leaf_chance).await;
            continue;
        }
        editor.place_block(wood, Point3D { x: x0, y, z: z0 }).await;

        if y == height {
            for (dx, dz) in [
                (1, 0), (-1, 0), (0, 1), (0, -1)
            ] {
                editor.place_block_chance(leaf, Point3D { x: x0 + dx, y, z: z0 + dz }, rng, leaf_chance).await;
            }
        } else if y == height - 1 {
            let positions = [
                (2, 0), (-2, 0), (0, 2), (0, -2),
                (1, 1), (-1, -1), (-1, 1), (1, -1),
                (2, 1), (-2, -1), (-2, 1), (2, -1),
                (1, 2), (-1, -2), (-1, 2), (1, -2)
            ];
            for (dx, dz) in positions {
                editor.place_block_chance(leaf, Point3D { x: x0 + dx, y, z: z0 + dz }, rng, leaf_chance).await;
            }
        } else if y % 2 == height % 2 && y > y0 + 2 {
            let positions = [
                (2, 0), (-2, 0), (0, 2), (0, -2),
                (1, 1), (-1, -1), (-1, 1), (1, -1),
                (1, 0), (-1, 0), (0, 1), (0, -1)
            ];
            for (dx, dz) in positions {
                editor.place_block_chance(leaf, Point3D { x: x0 + dx, y, z: z0 + dz }, rng, leaf_chance).await;
            }
        } else if y % 2 == (height - 1) % 2 && y > y0 + 1 {
            let positions = [
                (2, 0), (-2, 0), (0, 2), (0, -2),
                (1, 1), (-1, -1), (-1, 1), (1, -1),
                (2, 1), (-2, -1), (-2, 1), (2, -1),
                (1, 2), (-1, -2), (-1, 2), (1, -2),
                (3, 0), (-3, 0), (0, 3), (0, -3),
                (2, 2), (-2, -2), (-2, 2), (2, -2)
            ];
            for (dx, dz) in positions {
                editor.place_block_chance(leaf, Point3D { x: x0 + dx, y, z: z0 + dz }, rng, leaf_chance).await;
            }
        }
    }
}

async fn generate_large_pine(
    editor: &mut Editor,
    point: Point3D,
    wood: &Block,
    leaf: &Block,
    rng: &mut RNG,
    leaf_chance: i32,
) {
    let (x0, y0, z0) = (point.x, point.y, point.z);
    let height = rng.rand_i32_range(14, 22) + y0; // 22 exclusive, so 21 inclusive

    for y in y0..=(height + 2) {
        if y == height + 1 {
            let positions = [
                (0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)
            ];
            for (dx, dz) in positions {
                editor.place_block_chance(leaf, Point3D { x: x0 + dx, y, z: z0 + dz }, rng, leaf_chance).await;
            }
            continue;
        } else if y == height + 2 {
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 }, rng, leaf_chance).await;
            continue;
        }
        editor.place_block(wood, Point3D { x: x0, y, z: z0 }).await;

        if y == height {
            let positions = [
                (1, 0), (-1, 0), (0, 1), (0, -1)
            ];
            for (dx, dz) in positions {
                editor.place_block_chance(leaf, Point3D { x: x0 + dx, y, z: z0 + dz }, rng, leaf_chance).await;
            }
        } else if y % 3 == (height - 1) % 3 && y > y0 + 5 {
            let positions = [
                (2, 0), (-2, 0), (0, 2), (0, -2),
                (1, 1), (-1, -1), (-1, 1), (1, -1),
                (1, 0), (-1, 0), (0, 1), (0, -1)
            ];
            for (dx, dz) in positions {
                editor.place_block_chance(leaf, Point3D { x: x0 + dx, y, z: z0 + dz }, rng, leaf_chance).await;
            }
        } else if y % 3 == (height - 2) % 3 && y > y0 + 4 {
            let positions = [
                (2, 0), (-2, 0), (0, 2), (0, -2),
                (1, 1), (-1, -1), (-1, 1), (1, -1),
                (2, 1), (-2, -1), (-2, 1), (2, -1),
                (1, 2), (-1, -2), (-1, 2), (1, -2),
                (3, 0), (-3, 0), (0, 3), (0, -3),
                (2, 2), (-2, -2), (-2, 2), (2, -2)
            ];
            for (dx, dz) in positions {
                editor.place_block_chance(leaf, Point3D { x: x0 + dx, y, z: z0 + dz }, rng, leaf_chance).await;
            }
        } else if y % 3 == height % 3 && y > y0 + 3 {
            let positions = [
                (2, 0), (-2, 0), (0, 2), (0, -2),
                (2, 1), (-2, -1), (-2, 1), (2, -1),
                (1, 2), (-1, -2), (-1, 2), (1, -2),
                (3, 0), (-3, 0), (0, 3), (0, -3),
                (2, 2), (-2, -2), (-2, 2), (2, -2),
                (3, 1), (-3, 1), (1, 3), (1, -3),
                (3, 2), (-3, 2), (2, 3), (2, -3),
                (3, -1), (-3, -1), (-1, 3), (-1, -3),
                (3, -2), (-3, -2), (-2, 3), (-2, -3),
                (4, 0), (-4, 0), (0, 4), (0, -4)
            ];
            for (dx, dz) in positions {
                editor.place_block_chance(leaf, Point3D { x: x0 + dx, y, z: z0 + dz }, rng, leaf_chance).await;
            }
        }
    }
}

async fn generate_mega_pine(
    editor: &mut Editor,
    point: Point3D,
    wood: &Block,
    leaf: &Block,
    rng: &mut RNG,
    leaf_chance: i32,
) {
    let offsets = [(0, 0), (1, 0), (1, 1), (0, 1)];
    for (dx, dz) in offsets {
        let new_point = Point3D { x: point.x + dx, y: point.y, z: point.z + dz };
        generate_large_pine(editor, new_point, wood, leaf, rng, leaf_chance).await;
    }
}

async fn generate_small_hedge(
    editor: &mut Editor,
    point: Point3D,
    wood: &Block,
    leaf: &Block,
    rng: &mut RNG,
    leaf_chance: i32,
) {
    let (x0, y0, z0) = (point.x, point.y, point.z);
    let height = rng.rand_i32_range(4, 8) + y0; // 8 exclusive, so 7 inclusive

    for y in y0..=(height + 1) {
        if y == height + 1 {
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 }, rng, leaf_chance).await;
            continue;
        } else {
            editor.place_block(wood, Point3D { x: x0, y, z: z0 }).await;
        }
        if y > y0 {
            editor.place_block_chance(leaf, Point3D { x: x0 + 1, y, z: z0 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 - 1, y, z: z0 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 + 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 - 1 }, rng, leaf_chance).await;
        }
    }
}

async fn generate_medium_hedge(
    editor: &mut Editor,
    point: Point3D,
    wood: &Block,
    leaf: &Block,
    rng: &mut RNG,
    leaf_chance: i32,
) {
    let (x0, y0, z0) = (point.x, point.y, point.z);
    let height = rng.rand_i32_range(8, 14) + y0; // 14 exclusive, so 13 inclusive

    for y in y0..=(height + 1) {
        if y == height + 1 {
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 }, rng, leaf_chance).await;
            continue;
        } else {
            editor.place_block(wood, Point3D { x: x0, y, z: z0 }).await;
        }
        if y > y0 {
            editor.place_block_chance(leaf, Point3D { x: x0 + 1, y, z: z0 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 - 1, y, z: z0 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 + 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 - 1 }, rng, leaf_chance).await;
        }
        if y > y0 + 2 && y < height {
            editor.place_block_chance(leaf, Point3D { x: x0 + 1, y, z: z0 + 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 - 1, y, z: z0 - 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 - 1, y, z: z0 + 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 + 1, y, z: z0 - 1 }, rng, leaf_chance).await;
        }
    }
}

async fn generate_large_hedge(
    editor: &mut Editor,
    point: Point3D,
    wood: &Block,
    leaf: &Block,
    rng: &mut RNG,
    leaf_chance: i32,
) {
    let (x0, y0, z0) = (point.x, point.y, point.z);
    let height = rng.rand_i32_range(14, 20) + y0; // 20 exclusive, so 19 inclusive

    for y in y0..=(height + 1) {
        if y == height + 1 {
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 }, rng, leaf_chance).await;
            continue;
        } else {
            editor.place_block(wood, Point3D { x: x0, y, z: z0 }).await;
        }
        if y > y0 {
            editor.place_block_chance(leaf, Point3D { x: x0 + 1, y, z: z0 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 - 1, y, z: z0 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 + 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 - 1 }, rng, leaf_chance).await;
        }
        if y > y0 + 2 && y < height {
            editor.place_block_chance(leaf, Point3D { x: x0 + 1, y, z: z0 + 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 - 1, y, z: z0 - 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 - 1, y, z: z0 + 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 + 1, y, z: z0 - 1 }, rng, leaf_chance).await;
        }
        if y > y0 + 3 && y < height - 1 {
            editor.place_block_chance(leaf, Point3D { x: x0 + 2, y, z: z0 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 - 2, y, z: z0 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 + 2 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 - 2 }, rng, leaf_chance).await;
        }
        if y > y0 + 5 && y < height - 3 {
            editor.place_block_chance(leaf, Point3D { x: x0 + 2, y, z: z0 + 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 - 2, y, z: z0 + 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 + 1, y, z: z0 + 2 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 + 1, y, z: z0 - 2 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 + 2, y, z: z0 - 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 - 2, y, z: z0 - 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 - 1, y, z: z0 + 2 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 - 1, y, z: z0 - 2 }, rng, leaf_chance).await;
        }
    }
}

async fn generate_mega_hedge(
    editor: &mut Editor,
    point: Point3D,
    wood: &Block,
    leaf: &Block,
    rng: &mut RNG,
    leaf_chance: i32,
) {
    let offsets = [(0, 0), (1, 0), (1, 1), (0, 1)];
    for (dx, dz) in offsets {
        let new_point = Point3D { x: point.x + dx, y: point.y, z: point.z + dz };
        generate_large_hedge(editor, new_point, wood, leaf, rng, leaf_chance).await;
    }
}
async fn generate_small_oak(
    editor: &mut Editor,
    point: Point3D,
    wood: &Block,
    leaf: &Block,
    rng: &mut RNG,
    leaf_chance: i32,
) {
    let (x0, y0, z0) = (point.x, point.y, point.z);
    let stem_height = rng.rand_i32_range(4, 8); // 8 exclusive, so 7 inclusive

    for y in y0..=(stem_height + y0 + 1) {
        if y == stem_height + y0 + 1 {
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 + 1, y, z: z0 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 - 1, y, z: z0 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 + 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 - 1 }, rng, leaf_chance).await;
            continue;
        }
        editor.place_block(wood, Point3D { x: x0, y, z: z0 }).await;
        let mid = ((stem_height as f32) / 2.0).floor() as i32 + y0 - 1;
        if y == mid {
            editor.place_block_chance(leaf, Point3D { x: x0 + 1, y, z: z0 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0 - 1, y, z: z0 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 + 1 }, rng, leaf_chance).await;
            editor.place_block_chance(leaf, Point3D { x: x0, y, z: z0 - 1 }, rng, leaf_chance).await;
        } else if y == stem_height + y0 || y == ((stem_height as f32) / 2.0).floor() as i32 + y0 {
            let positions = [
                (2, 0), (-2, 0), (0, 2), (0, -2),
                (1, 1), (-1, -1), (-1, 1), (1, -1),
                (1, 0), (-1, 0), (0, 1), (0, -1),
            ];
            for (dx, dz) in positions {
                editor.place_block_chance(leaf, Point3D { x: x0 + dx, y, z: z0 + dz }, rng, leaf_chance).await;
            }
        } else if y > ((stem_height as f32) / 2.0).floor() as i32 + y0 && y < stem_height + y0 {
            let positions = [
                (2, 0), (-2, 0), (0, 2), (0, -2),
                (2, 1), (-2, 1), (1, 2), (1, -2),
                (2, -1), (-2, -1), (-1, 2), (-1, -2),
                (1, 1), (-1, -1), (-1, 1), (1, -1),
                (1, 0), (-1, 0), (0, 1), (0, -1),
            ];
            for (dx, dz) in positions {
                editor.place_block_chance(leaf, Point3D { x: x0 + dx, y, z: z0 + dz }, rng, leaf_chance).await;
            }
        }
    }
}

async fn generate_medium_oak(
    editor: &mut Editor,
    point: Point3D,
    wood: &Block,
    leaf: &Block,
    rng: &mut RNG,
    leaf_chance: i32,
) {
    let (x0, y0, z0) = (point.x, point.y, point.z);
    let height = rng.rand_i32_range(10, 16) + y0; // 16 exclusive, so 15 inclusive
    let branch_num = rng.rand_i32_range(3, 8); // 8 exclusive, so 7 inclusive

    let mut branches = Vec::new();
    branches.push((x0, height - 1, z0));

    // Trunk and base
    for y in y0..height {
        editor.place_block(wood, Point3D { x: x0, y, z: z0 }).await;
        if y == y0 {
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if rng.rand_i32_range(1, 5) != 4 {
                    editor.place_block(wood, Point3D { x: x0 + dx, y, z: z0 + dz }).await;
                }
            }
        }
    }

    for _ in 0..branch_num {
        let branch_height = rng.rand_i32_range(((height - y0) / 2 + y0) + 4, height);
        let branch_pos = rng.rand_i32_range(1, 25); // 25 exclusive, so 24 inclusive

        match branch_pos {
            1 => {
                branches.push((x0 + 3, branch_height, z0));
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 }).await;
            }
            2 => {
                branches.push((x0 + 3, branch_height, z0 + 1));
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height, z: z0 + 1 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 + 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 }).await;
            }
            3 => {
                branches.push((x0 + 3, branch_height, z0 + 2));
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height, z: z0 + 2 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 + 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 + 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 + 1 }).await;
            }
            4 => {
                branches.push((x0 + 3, branch_height, z0 + 3));
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 + 1 }).await;
            }
            5 => {
                branches.push((x0 + 2, branch_height, z0 + 3));
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height, z: z0 + 3 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 + 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 + 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 + 1 }).await;
            }
            6 => {
                branches.push((x0 + 1, branch_height, z0 + 3));
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height, z: z0 + 3 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 + 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 1, z: z0 + 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 1, z: z0 + 1 }).await;
            }
            7 => {
                branches.push((x0, branch_height, z0 + 3));
                editor.place_block(wood, Point3D { x: x0, y: branch_height, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 1, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 1, z: z0 + 1 }).await;
            }
            8 => {
                branches.push((x0 - 1, branch_height, z0 + 3));
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height, z: z0 + 3 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 + 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 1, z: z0 + 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 1, z: z0 + 1 }).await;
            }
            9 => {
                branches.push((x0 - 2, branch_height, z0 + 3));
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height, z: z0 + 3 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 + 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 + 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 + 1 }).await;
            }
            10 => {
                branches.push((x0 - 3, branch_height, z0 + 3));
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 + 1 }).await;
            }
            11 => {
                branches.push((x0 - 3, branch_height, z0 + 2));
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height, z: z0 + 2 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 + 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 + 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 + 1 }).await;
            }
            12 => {
                branches.push((x0 - 3, branch_height, z0 + 1));
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height, z: z0 + 1 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 + 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 }).await;
            }
            13 => {
                branches.push((x0 - 3, branch_height, z0));
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 }).await;
            }
            14 => {
                branches.push((x0 - 3, branch_height, z0 - 1));
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height, z: z0 - 1 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 - 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 }).await;
            }
            15 => {
                branches.push((x0 - 3, branch_height, z0 - 2));
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height, z: z0 - 2 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 - 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 - 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 - 1 }).await;
            }
            16 => {
                branches.push((x0 - 3, branch_height, z0 - 3));
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 - 1 }).await;
            }
            17 => {
                branches.push((x0 - 2, branch_height, z0 - 3));
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height, z: z0 - 3 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 - 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 - 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 - 1 }).await;
            }
            18 => {
                branches.push((x0 - 1, branch_height, z0 - 3));
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height, z: z0 - 3 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 - 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 1, z: z0 - 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 1, z: z0 - 1 }).await;
            }
            19 => {
                branches.push((x0, branch_height, z0 - 3));
                editor.place_block(wood, Point3D { x: x0, y: branch_height, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 1, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 1, z: z0 - 1 }).await;
            }
            20 => {
                branches.push((x0 + 1, branch_height, z0 - 3));
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height, z: z0 - 3 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 - 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 1, z: z0 - 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 1, z: z0 - 1 }).await;
            }
            21 => {
                branches.push((x0 + 2, branch_height, z0 - 3));
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height, z: z0 - 3 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 - 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 - 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 - 1 }).await;
            }
            22 => {
                branches.push((x0 + 3, branch_height, z0 - 3));
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 - 1 }).await;
            }
            23 => {
                branches.push((x0 + 3, branch_height, z0 - 2));
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height, z: z0 - 2 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 - 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 - 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 - 1 }).await;
            }
            24 => {
                branches.push((x0 + 3, branch_height, z0 - 1));
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height, z: z0 - 1 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 - 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 }).await;
            }
            _ => {}
        }
    }

    // Leaves for each branch
    for &(x1, y1, z1) in &branches {
        // Top layer
        for (dx, dz) in &[(1, 0), (-1, 0), (0, 1), (0, -1), (0, 0)] {
            editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1 + 2, z: z1 + dz }, rng, leaf_chance).await;
        }

        // Second layer
        let second_layer = [
            (0, 0), (1, 1), (-1, -1), (-1, 1), (1, -1),
            (1, 0), (-1, 0), (0, 1), (0, -1),
            (2, 0), (-2, 0), (0, 2), (0, -2),
            (2, 1), (-2, -1), (-2, 1), (2, -1),
            (1, 2), (-1, -2), (-1, 2), (1, -2),
        ];
        for (dx, dz) in &second_layer {
            editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1 + 1, z: z1 + dz }, rng, leaf_chance).await;
        }

        // Third layer
        let third_layer = [
            (1, 1), (-1, -1), (-1, 1), (1, -1),
            (1, 0), (-1, 0), (0, 1), (0, -1),
            (2, 0), (-2, 0), (0, 2), (0, -2),
        ];
        for (dx, dz) in &third_layer {
            editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1, z: z1 + dz }, rng, leaf_chance).await;
        }
    }
}

async fn generate_large_oak(
    editor: &mut Editor,
    point: Point3D,
    wood: &Block,
    leaf: &Block,
    rng: &mut RNG,
    leaf_chance: i32,
) {
    let (x0, y0, z0) = (point.x, point.y, point.z);
    let height = rng.rand_i32_range(16, 24) + y0; // 24 exclusive, so 23 inclusive
    let branch_num = rng.rand_i32_range(5, 11); // 11 exclusive, so 10 inclusive

    let mut branches = Vec::new();
    branches.push((x0, height - 1, z0));

    // Trunk and base
    for y in y0..height {
        editor.place_block(wood, Point3D { x: x0, y, z: z0 }).await;
        if y == y0 {
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if rng.rand_i32_range(1, 5) != 4 {
                    editor.place_block(wood, Point3D { x: x0 + dx, y, z: z0 + dz }).await;
                }
            }
        }
    }

    
    for _ in 0..branch_num {
        let branch_height = rng.rand_i32_range(((height - y0) / 2 + y0) + 5, height - 1);
        let branch_pos = rng.rand_i32_range(1, 41); // 41 exclusive, so 40 inclusive

        match branch_pos {
            1 => {
                branches.push((x0 + 5, branch_height, z0));
                editor.place_block(wood, Point3D { x: x0 + 5, y: branch_height, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 + 4, y: branch_height - 1, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 }).await;
            }
            2 => {
                branches.push((x0 + 5, branch_height, z0 + 1));
                editor.place_block(wood, Point3D { x: x0 + 5, y: branch_height, z: z0 + 1 }).await;
                editor.place_block(wood, Point3D { x: x0 + 4, y: branch_height - 1, z: z0 + 1 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 + 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 }).await;
            }
            3 => {
                branches.push((x0 + 5, branch_height, z0 + 2));
                editor.place_block(wood, Point3D { x: x0 + 5, y: branch_height, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 4, y: branch_height - 1, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 + 1 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 + 1 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 }).await;
            }
            4 => {
                branches.push((x0 + 5, branch_height, z0 + 3));
                editor.place_block(wood, Point3D { x: x0 + 5, y: branch_height, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 4, y: branch_height - 1, z: z0 + 3 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 + 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 + 2 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 + 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 + 1 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 + 1 }).await;
            }
            5 => {
                branches.push((x0 + 5, branch_height, z0 + 4));
                editor.place_block(wood, Point3D { x: x0 + 5, y: branch_height, z: z0 + 4 }).await;
                editor.place_block(wood, Point3D { x: x0 + 4, y: branch_height - 1, z: z0 + 3 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 + 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 + 2 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 + 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 + 1 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 + 1 }).await;
            }
            6 => {
                branches.push((x0 + 5, branch_height, z0 + 5));
                editor.place_block(wood, Point3D { x: x0 + 5, y: branch_height, z: z0 + 5 }).await;
                editor.place_block(wood, Point3D { x: x0 + 4, y: branch_height - 1, z: z0 + 4 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 + 1 }).await;
            }
            7 => {
                branches.push((x0 + 4, branch_height, z0 + 5));
                editor.place_block(wood, Point3D { x: x0 + 4, y: branch_height, z: z0 + 5 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 1, z: z0 + 4 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 + 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 + 2 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 + 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 + 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 + 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 + 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 + 1 }).await;
            }
            8 => {
                branches.push((x0 + 3, branch_height, z0 + 5));
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height, z: z0 + 5 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 1, z: z0 + 4 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 + 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 + 2 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 + 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 + 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 + 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 + 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 + 1 }).await;
            }
            9 => {
                branches.push((x0 + 2, branch_height, z0 + 5));
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height, z: z0 + 5 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 + 4 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 + 1 }).await;
            }
            10 => {
                branches.push((x0 + 1, branch_height, z0 + 5));
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height, z: z0 + 5 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 + 4 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 + 3 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 + 3 }).await;
                }
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 + 1 }).await;
            }
            11 => {
                branches.push((x0, branch_height, z0 + 5));
                editor.place_block(wood, Point3D { x: x0, y: branch_height, z: z0 + 5 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 1, z: z0 + 4 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 + 1 }).await;
            }
            12 => {
                branches.push((x0 - 1, branch_height, z0 + 5));
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height, z: z0 + 5 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 + 4 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 + 3 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 + 3 }).await;
                }
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 + 1 }).await;
            }
            13 => {
                branches.push((x0 - 2, branch_height, z0 + 5));
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height, z: z0 + 5 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 + 4 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 + 1 }).await;
            }
            14 => {
                branches.push((x0 - 3, branch_height, z0 + 5));
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height, z: z0 + 5 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 1, z: z0 + 4 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 + 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 + 2 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 + 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 + 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 + 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 + 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 + 1 }).await;
            }
            15 => {
                branches.push((x0 - 4, branch_height, z0 + 5));
                editor.place_block(wood, Point3D { x: x0 - 4, y: branch_height, z: z0 + 5 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 1, z: z0 + 4 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 + 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 + 2 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 + 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 + 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 + 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 + 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 + 1 }).await;
            }
            16 => {
                branches.push((x0 - 5, branch_height, z0 + 5));
                editor.place_block(wood, Point3D { x: x0 - 5, y: branch_height, z: z0 + 5 }).await;
                editor.place_block(wood, Point3D { x: x0 - 4, y: branch_height - 1, z: z0 + 4 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 + 1 }).await;
            }
            17 => {
                branches.push((x0 - 5, branch_height, z0 + 4));
                editor.place_block(wood, Point3D { x: x0 - 5, y: branch_height, z: z0 + 4 }).await;
                editor.place_block(wood, Point3D { x: x0 - 4, y: branch_height - 1, z: z0 + 3 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 + 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 + 2 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 + 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 + 1 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 + 1 }).await;
            }
            18 => {
                branches.push((x0 - 5, branch_height, z0 + 3));
                editor.place_block(wood, Point3D { x: x0 - 5, y: branch_height, z: z0 + 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 4, y: branch_height - 1, z: z0 + 3 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 + 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 + 2 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 + 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 + 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 + 1 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 + 1 }).await;
            }
            19 => {
                branches.push((x0 - 5, branch_height, z0 + 2));
                editor.place_block(wood, Point3D { x: x0 - 5, y: branch_height, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 4, y: branch_height - 1, z: z0 + 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 + 1 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 + 1 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 }).await;
            }
            20 => {
                branches.push((x0 - 5, branch_height, z0 + 1));
                editor.place_block(wood, Point3D { x: x0 - 5, y: branch_height, z: z0 + 1 }).await;
                editor.place_block(wood, Point3D { x: x0 - 4, y: branch_height - 1, z: z0 + 1 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 + 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 }).await;
            }
            21 => {
                branches.push((x0 - 5, branch_height, z0));
                editor.place_block(wood, Point3D { x: x0 - 5, y: branch_height, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 - 4, y: branch_height - 1, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 }).await;
            }
            22 => {
                branches.push((x0 - 5, branch_height, z0 - 1));
                editor.place_block(wood, Point3D { x: x0 - 5, y: branch_height, z: z0 - 1 }).await;
                editor.place_block(wood, Point3D { x: x0 - 4, y: branch_height - 1, z: z0 - 1 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 - 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 }).await;
            }
            23 => {
                branches.push((x0 - 5, branch_height, z0 - 2));
                editor.place_block(wood, Point3D { x: x0 - 5, y: branch_height, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 4, y: branch_height - 1, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 - 1 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 - 1 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 }).await;
            }
            24 => {
                branches.push((x0 - 5, branch_height, z0 - 3));
                editor.place_block(wood, Point3D { x: x0 - 5, y: branch_height, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 4, y: branch_height - 1, z: z0 - 3 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 - 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 - 2 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 - 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 - 1 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 - 1 }).await;
            }
            25 => {
                branches.push((x0 - 5, branch_height, z0 - 4));
                editor.place_block(wood, Point3D { x: x0 - 5, y: branch_height, z: z0 - 4 }).await;
                editor.place_block(wood, Point3D { x: x0 - 4, y: branch_height - 1, z: z0 - 3 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 - 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 - 2 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 - 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 - 1 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 - 1 }).await;
            }
            26 => {
                branches.push((x0 - 5, branch_height, z0 - 5));
                editor.place_block(wood, Point3D { x: x0 - 5, y: branch_height, z: z0 - 5 }).await;
                editor.place_block(wood, Point3D { x: x0 - 4, y: branch_height - 1, z: z0 - 4 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 - 1 }).await;
            }
            27 => {
                branches.push((x0 + 5, branch_height, z0 - 5));
                editor.place_block(wood, Point3D { x: x0 + 5, y: branch_height, z: z0 - 5 }).await;
                editor.place_block(wood, Point3D { x: x0 + 4, y: branch_height - 1, z: z0 - 4 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 - 1 }).await;
            }
            28 => {
                branches.push((x0 + 4, branch_height, z0 - 5));
                editor.place_block(wood, Point3D { x: x0 + 4, y: branch_height, z: z0 - 5 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 1, z: z0 - 4 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 - 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 - 2 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 - 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 - 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 - 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 - 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 - 1 }).await;
            }
            29 => {
                branches.push((x0 + 3, branch_height, z0 - 5));
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height, z: z0 - 5 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 1, z: z0 - 4 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 - 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 - 2 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 - 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 - 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 - 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 - 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 - 1 }).await;
            }
            30 => {
                branches.push((x0 + 2, branch_height, z0 - 5));
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height, z: z0 - 5 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 1, z: z0 - 4 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 - 1 }).await;
            }
            31 => {
                branches.push((x0 + 1, branch_height, z0 - 5));
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height, z: z0 - 5 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 1, z: z0 - 4 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 - 3 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 - 3 }).await;
                }
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 - 1 }).await;
            }
            32 => {
                branches.push((x0, branch_height, z0 - 5));
                editor.place_block(wood, Point3D { x: x0, y: branch_height, z: z0 - 5 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 1, z: z0 - 4 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 - 1 }).await;
            }
            33 => {
                branches.push((x0 - 1, branch_height, z0 - 5));
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height, z: z0 - 5 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 1, z: z0 - 4 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 - 3 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 - 3 }).await;
                }
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 - 1 }).await;
            }
            34 => {
                branches.push((x0 - 2, branch_height, z0 - 5));
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height, z: z0 - 5 }).await;
                editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 1, z: z0 - 4 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0, y: branch_height - 2, z: z0 - 1 }).await;
            }
            35 => {
                branches.push((x0 - 3, branch_height, z0 - 5));
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height, z: z0 - 5 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 1, z: z0 - 4 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 - 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 - 2 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 - 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 - 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 - 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 - 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 - 1 }).await;
            }
            36 => {
                branches.push((x0 - 4, branch_height, z0 - 5));
                editor.place_block(wood, Point3D { x: x0 - 4, y: branch_height, z: z0 - 5 }).await;
                editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 1, z: z0 - 4 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 - 3, y: branch_height - 2, z: z0 - 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 - 2 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 - 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 - 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 - 2, y: branch_height - 2, z: z0 - 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 - 2 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 - 1, y: branch_height - 2, z: z0 - 1 }).await;
            }
            37 => {
                branches.push((x0 + 5, branch_height, z0 - 1));
                editor.place_block(wood, Point3D { x: x0 + 5, y: branch_height, z: z0 - 1 }).await;
                editor.place_block(wood, Point3D { x: x0 + 4, y: branch_height - 1, z: z0 - 1 }).await;
                let b = rng.rand_i32_range(1, 3);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 - 1 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 }).await;
            }
            38 => {
                branches.push((x0 + 5, branch_height, z0 - 2));
                editor.place_block(wood, Point3D { x: x0 + 5, y: branch_height, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 4, y: branch_height - 1, z: z0 - 2 }).await;
                editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 - 1 }).await;
                editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 - 1 }).await;
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 }).await;
            }
            39 => {
                branches.push((x0 + 5, branch_height, z0 - 3));
                editor.place_block(wood, Point3D { x: x0 + 5, y: branch_height, z: z0 - 3 }).await;
                editor.place_block(wood, Point3D { x: x0 + 4, y: branch_height - 1, z: z0 - 3 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 - 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 - 2 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 - 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 - 1 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 - 1 }).await;
            }
            40 => {
                branches.push((x0 + 5, branch_height, z0 - 4));
                editor.place_block(wood, Point3D { x: x0 + 5, y: branch_height, z: z0 - 4 }).await;
                editor.place_block(wood, Point3D { x: x0 + 4, y: branch_height - 1, z: z0 - 3 }).await;
                let b = rng.rand_i32_range(1, 4);
                if b == 1 {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 - 3 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 - 2 }).await;
                } else if b == 2 {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 - 2 }).await;
                } else {
                    editor.place_block(wood, Point3D { x: x0 + 3, y: branch_height - 2, z: z0 - 2 }).await;
                    editor.place_block(wood, Point3D { x: x0 + 2, y: branch_height - 2, z: z0 - 1 }).await;
                }
                editor.place_block(wood, Point3D { x: x0 + 1, y: branch_height - 2, z: z0 - 1 }).await;
            }
            _ => { // No branch placed
            }
        }
    }


    for &(x1, y1, z1) in &branches {
        if x1 == x0 && z1 == z0 {
            // More leaves for the main stem/branch
            for (dx, dz) in &[
                (1, 1), (-1, -1), (-1, 1), (1, -1)
            ] {
                editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1 + 3, z: z1 + dz }, rng, leaf_chance).await;
            }
            for (dx, dz) in &[
                (3, 0), (-3, 0), (0, 3), (0, -3),
                (2, 2), (-2, -2), (-2, 2), (2, -2)
            ] {
                editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1 + 2, z: z1 + dz }, rng, leaf_chance).await;
            }
            for (dx, dz) in &[
                (4, 1), (-4, -1), (-4, 1), (4, -1),
                (1, 4), (-1, 4), (1, -4), (-1, -4),
                (4, 0), (-4, 0), (0, 4), (0, -4),
                (3, 3), (-3, -3), (-3, 3), (3, -3),
                (2, 3), (-2, 3), (2, -3), (-2, -3),
                (3, 2), (-3, 2), (3, -2), (-3, -2)
            ] {
                editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1 + 1, z: z1 + dz }, rng, leaf_chance).await;
            }
            for (dx, dz) in &[
                (2, -1), (-2, -1), (-1, 2), (-1, -2),
                (2, 1), (-2, 1), (1, 2), (1, -2)
            ] {
                editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1, z: z1 + dz }, rng, leaf_chance).await;
            }
        } else {
            editor.place_block_chance(leaf, Point3D { x: x1, y: y1 - 1, z: z1 }, rng, leaf_chance).await;
        }

        // Top leaf cluster
        for (dx, dz) in &[
            (1, 0), (-1, 0), (0, 1), (0, -1), (0, 0)
        ] {
            editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1 + 3, z: z1 + dz }, rng, leaf_chance).await;
        }

        // Second layer
        let second_layer = [
            (0, 0), (1, 1), (-1, -1), (-1, 1), (1, -1),
            (1, 0), (-1, 0), (0, 1), (0, -1),
            (2, 0), (-2, 0), (0, 2), (0, -2),
            (2, 1), (-2, -1), (-2, 1), (2, -1),
            (1, 2), (-1, -2), (-1, 2), (1, -2),
        ];
        for (dx, dz) in &second_layer {
            editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1 + 2, z: z1 + dz }, rng, leaf_chance).await;
        }

        // Third layer
        let third_layer = [
            (1, 1), (-1, -1), (-1, 1), (1, -1),
            (1, 0), (-1, 0), (0, 1), (0, -1),
            (2, 0), (-2, 0), (0, 2), (0, -2),
        ];
        for (dx, dz) in &third_layer {
            editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1 + 1, z: z1 + dz }, rng, leaf_chance).await;
        }

        // Fourth layer
        let fourth_layer = [
            (1, 1), (-1, -1), (-1, 1), (1, -1),
            (1, 0), (-1, 0), (0, 1), (0, -1),
            (2, 0), (-2, 0), (0, 2), (0, -2),
        ];
        for (dx, dz) in &fourth_layer {
            editor.place_block_chance(leaf, Point3D { x: x1 + dx, y: y1, z: z1 + dz }, rng, leaf_chance).await;
        }
    }
}

async fn generate_mega_oak(
    editor: &mut Editor,
    point: Point3D,
    wood: &Block,
    leaf: &Block,
    rng: &mut RNG,
    leaf_chance: i32,
) {
    let offsets = [(0, 0), (1, 0), (1, 1), (0, 1)];
    for (dx, dz) in offsets {
        let new_point = Point3D { x: point.x + dx, y: point.y, z: point.z + dz };
        generate_large_oak(editor, new_point, wood, leaf, rng, leaf_chance).await;
    }
}