//! Stone lanterns (tōrō) for Japanese gardens.
//!
//! A tōrō is a stout 3-tall pedestal lantern: a redstone-block foot ringed by
//! flared stairs, a redstone lamp caged behind oak trapdoors, and a stone cap
//! ringed by flared bottom slabs. The stone comes from the culture palette (so
//! it blends with the town's masonry); the cage is always oak.
//!
//! They're scattered across the interior of green open spaces — parks and
//! nooks — by [`scatter_garden_lanterns`], called from the open-space
//! furnishing pass. Japanese-only; a no-op for every other culture.

use std::collections::HashMap;

use crate::editor::Editor;
use crate::generator::buildings_v2::Culture;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialId, MaterialPlacer, Placer};
use crate::generator::open_space::Region;
use crate::generator::BuildClaim;
use crate::geometry::{Cardinal, Point2D, Point3D, CARDINALS_2D, UP};
use crate::minecraft::{Block, BlockForm};
use crate::noise::RNG;

/// Minimum Chebyshev (chessboard) spacing between two lanterns in a garden. A
/// tōrō is a 3×3 footprint, so 7 leaves a generous gap of clear ground.
const GARDEN_GAP: i32 = 7;

/// One lantern per this many cells of region area, clamped to [1, 2] — a small
/// garden gets a single tōrō, a large one at most a pair. Deliberately sparse:
/// a tōrō is a focal piece, not area lighting.
const AREA_PER_LANTERN: usize = 130;

/// The four cardinal rings of the pedestal.
const RING: [Cardinal; 4] = [Cardinal::North, Cardinal::East, Cardinal::South, Cardinal::West];

/// Scatter tōrō across the interior of a garden `region` (park or nook),
/// built from `stone_block` so they match the surrounding open-space masonry —
/// pass the garden [`Theme`](crate::generator::open_space::Theme)'s `stone`
/// (the same worked stone its monuments and graves use). Japanese-only — a
/// no-op otherwise, so the caller can ring every green space unconditionally.
/// Returns how many landed.
///
/// Candidates are region cells whose full 3×3 footprint is open ground (off
/// water, clear of buildings/roads/walls); they're taken in a seed-deterministic
/// shuffled order and kept at least [`GARDEN_GAP`] apart.
pub async fn scatter_garden_lanterns(
    editor: &Editor,
    region: &Region,
    data: &LoadedData,
    culture: Culture,
    stone_block: &str,
    rng: &mut RNG,
) -> usize {
    if culture != Culture::Japanese {
        return 0;
    }
    // The theme stone is a namespaced block id (e.g. "minecraft:stone_bricks");
    // the material registry is keyed by the bare name, which also resolves the
    // matching stairs/slab forms.
    let stone = MaterialId::new(
        stone_block
            .strip_prefix("minecraft:")
            .unwrap_or(stone_block)
            .to_string(),
    );

    // Sort to a canonical order first so the shuffle (and thus the layout) is
    // reproducible from the seed regardless of region cell ordering.
    let mut candidates: Vec<Point2D> = region
        .cells
        .iter()
        .copied()
        .filter(|&c| has_clear_footprint(editor, c))
        .collect();
    candidates.sort_by_key(|p| (p.x, p.y));
    rng.shuffle(&mut candidates);

    let target = (region.area / AREA_PER_LANTERN).clamp(1, 2);
    let mut placer = MaterialPlacer::new(Placer::new(&data.materials, rng), stone);
    let mut placed: Vec<Point2D> = Vec::new();
    for c in candidates {
        if placed.len() >= target {
            break;
        }
        if placed.iter().any(|p| chebyshev(*p, c) < GARDEN_GAP) {
            continue;
        }
        let foot = editor.world().add_height(c);
        build_stone_lantern(editor, &mut placer, foot).await;
        placed.push(c);
    }
    placed.len()
}

/// Chebyshev (chessboard) distance between two cells.
fn chebyshev(a: Point2D, b: Point2D) -> i32 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

/// The foot cell and its four arms must all be in-bounds, off water, and on
/// open green ground (unclaimed or nature) — so a lantern never clips a
/// building, wall, or road, and never blocks a path entrance.
fn has_clear_footprint(editor: &Editor, c: Point2D) -> bool {
    let world = editor.world();
    std::iter::once(c)
        .chain(CARDINALS_2D.iter().map(|&d| c + d))
        .all(|cell| {
            world.is_in_bounds_2d(cell)
                && !world.is_water(cell)
                && matches!(
                    world.get_claim(cell),
                    None | Some(BuildClaim::None | BuildClaim::Nature)
                )
        })
}

/// Build one tōrō standing on `foot` (the ground cell the redstone block sits
/// on). Fifteen blocks across a plus-shaped 3×3 footprint, three tall.
///
/// `stone` supplies the palette masonry in three forms — full block (cap),
/// stairs (base ring), bottom slab (cap ring). The lit core is a redstone lamp
/// powered (and so kept lit) by a redstone block directly beneath it, caged on
/// four sides by open oak trapdoors hugging the lamp.
async fn build_stone_lantern(editor: &Editor, stone: &mut MaterialPlacer<'_>, foot: Point3D) {
    // Centre column: power → lamp → cap.
    let redstone: Block = "minecraft:redstone_block".into();
    editor.place_block_forced(&redstone, foot).await;

    // Force `lit=true` for an immediate glow; the redstone block beneath keeps
    // it powered so it never reverts on the next block update.
    let lamp = Block::new(
        "minecraft:redstone_lamp".into(),
        Some(HashMap::from([("lit".to_string(), "true".to_string())])),
        None,
    );
    editor.place_block_forced(&lamp, foot + UP).await;

    stone
        .place_block_forced(editor, foot + UP * 2, BlockForm::Block, None, None)
        .await;

    for dir in RING {
        let step: Point3D = dir.into();
        let n = foot + step;

        // Base ring: a stair's tall riser sits opposite its `facing`, so facing
        // *inward* (toward the centre) puts the riser on the outside and flares
        // the step down to ground around the redstone foot.
        let stair_state = HashMap::from([
            ("facing".to_string(), dir.opposite().to_string()),
            ("half".to_string(), "bottom".to_string()),
        ]);
        stone
            .place_block_forced(editor, n, BlockForm::Stairs, Some(&stair_state), None)
            .await;

        // Cage ring: an open oak trapdoor in the neighbour cell. Its panel
        // stands on the side it `facing`s away from, so facing *outward* hangs
        // the panel on the inner face, flush against the lamp.
        let cage_state = HashMap::from([
            ("open".to_string(), "true".to_string()),
            ("half".to_string(), "bottom".to_string()),
            ("facing".to_string(), dir.to_string()),
        ]);
        let trapdoor = Block::new("minecraft:oak_trapdoor".into(), Some(cage_state), None);
        editor.place_block_forced(&trapdoor, n + UP).await;

        // Cap ring: bottom slabs flaring out under the cap block — the roof eave.
        let slab_state = HashMap::from([("type".to_string(), "bottom".to_string())]);
        stone
            .place_block_forced(editor, n + UP * 2, BlockForm::Slab, Some(&slab_state), None)
            .await;
    }
}
