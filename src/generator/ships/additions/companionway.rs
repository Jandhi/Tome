//! Stage 3 · **Companionways** — connect the interior levels (and the weather deck) with a hatch
//! cut through each level's ceiling and a **stair flight** down to its floor (a **ladder** when
//! there isn't a clear straight run). Also lays the **hold floor** the lower stair lands on.
//!
//! Geometry is in the local frame, transformed by `Placement`; facings come from `ShipDir`. The
//! connector is set **off the centreline** (`z != 0`) so it never fouls the keel-stepped masts
//! (which run at `z = 0`). Hatch + connector cells are recorded on `DeckState::hatch_cells` so the
//! later furnish pass keeps them clear.

use std::collections::HashMap;

use crate::generator::materials::{MaterialId, MaterialPlacer, Placer};
use crate::geometry::Point3D;
use crate::minecraft::{string_to_block, BlockForm};

use super::super::levels::{build_ship_levels, ShipLevel};
use super::super::palette::ShipPart;
use super::super::{ShipDir, ShipCtx};
use super::{DeckContext, DeckState, SizeTier};

/// How a level is connected to the space above it.
enum Connector {
    /// A straight stair flight starting at station `hx` (top), descending toward +x at `z = cz`.
    Stair { hx: i32, cz: i32 },
    /// A vertical ladder at `(cx, cz)` against a backing post.
    Ladder { cx: i32, cz: i32 },
}

pub async fn build(ctx: &mut ShipCtx<'_>, dc: &DeckContext<'_>, state: &mut DeckState) {
    let deck_y = dc.deck.deck_y;
    let levels = build_ship_levels(dc.hull, deck_y, state.top_y);
    if levels.levels.is_empty() {
        return;
    }

    // Deck material (planks/stairs for the floor + connector).
    let deck_mat = ctx
        .palette
        .get_material(dc.ship_palette.role(ShipPart::Deck))
        .expect("Deck role missing")
        .clone();

    let air = string_to_block("minecraft:air");
    let top_y = state.top_y; // the open weather deck (its hatch gets openable trapdoor lids)

    for level in &levels.levels {
        // Lay every hold-type floor (the gun deck's floor is the existing main deck).
        if level.name != "gun_deck" {
            let mut floor_rng = ctx.rng.derive();
            let mut floor = MaterialPlacer::new(Placer::new(&ctx.data.materials, &mut floor_rng), deck_mat.clone());
            for (x, &h) in level.outline.iter().enumerate() {
                for z in -h..=h {
                    floor
                        .place_block(ctx.editor, dc.placement.to_world(Point3D::new(x as i32, level.floor_y, z)), BlockForm::Block, None, None)
                        .await;
                }
            }
        }

        // The **stair companionway** serves the upper decks (gun deck + the main hold, whose ceiling
        // is at/above the main deck). **Lower holds** are reached by **mast ladders** instead (below).
        if level.ceiling_y < deck_y {
            continue;
        }

        let drop = level.ceiling_y - level.floor_y;
        match pick_connector(level, drop) {
            Connector::Stair { hx, cz } => {
                // One flight off the centreline; **larger ships get a mirrored pair** (port +
                // starboard) for a symmetric gangway.
                place_stair_flight(ctx, dc, state, level, hx, cz, top_y, &deck_mat).await;
                if dc.tier >= SizeTier::Large {
                    place_stair_flight(ctx, dc, state, level, hx, -cz, top_y, &deck_mat).await;
                }
            }
            Connector::Ladder { cx, cz } => {
                let mut placer_rng = ctx.rng.derive();
                let mut placer = MaterialPlacer::new(Placer::new(&ctx.data.materials, &mut placer_rng), deck_mat.clone());
                // Backing post (planks) one step toward the bow, so the ladder has something to
                // hang on; the ladder faces the stern (away from the post).
                let face = dc.placement.world_cardinal(ShipDir::Stern).to_string();
                let is_weather = level.ceiling_y == top_y;
                for y in (level.floor_y + 1)..level.ceiling_y {
                    placer
                        .place_block(ctx.editor, dc.placement.to_world(Point3D::new(cx + 1, y, cz)), BlockForm::Block, None, None)
                        .await;
                }
                // Opening: openable trapdoor lid on the weather deck, else a plain hole.
                if is_weather {
                    if let Some(td) = string_to_block(&format!(
                        "minecraft:oak_trapdoor[half=top,open=false,facing={face}]"
                    )) {
                        ctx.editor.place_block_forced(&td, dc.placement.to_world(Point3D::new(cx, level.ceiling_y, cz))).await;
                    }
                } else if let Some(a) = &air {
                    ctx.editor.place_block_forced(a, dc.placement.to_world(Point3D::new(cx, level.ceiling_y, cz))).await;
                }
                state.hatch_cells.push(Point3D::new(cx, level.ceiling_y, cz));
                // Ladder up to just under the opening.
                for y in (level.floor_y + 1)..level.ceiling_y {
                    if let Some(b) = string_to_block(&format!("minecraft:ladder[facing={face}]")) {
                        ctx.editor.place_block_forced(&b, dc.placement.to_world(Point3D::new(cx, y, cz))).await;
                    }
                }
                state.hatch_cells.push(Point3D::new(cx, level.floor_y, cz));
            }
        }
    }

    // --- Mast ladders to the lower holds ---------------------------------------------------------
    // The hull's **lower holds** (below the main hold) are reached by **ladders on the masts**: the
    // keel-stepped masts run down the centreline through every level, so a ladder on a mast's **aft
    // face** spans them. A 1-cell hole is cut in each hold floor at the ladder so you can climb
    // through. Only built when there's more than one hold level.
    let hold_floors: Vec<i32> = {
        let mut f: Vec<i32> = levels.levels.iter().filter(|l| l.name.contains("hold")).map(|l| l.floor_y).collect();
        f.sort_unstable();
        f
    };
    if hold_floors.len() >= 2 {
        let (lowest, main_hold) = (hold_floors[0], *hold_floors.last().unwrap());
        // Climb facing the stern: the ladder's back (opposite `facing`) is the +x neighbour = the
        // mast, so it hangs on the mast's aft face at `z = 0`, one station aft (`base_x - 1`).
        let face = dc.placement.world_cardinal(ShipDir::Stern).to_string();
        let masts: Vec<i32> = state.masts.as_ref().map(|m| m.masts.iter().map(|mm| mm.base_x).collect()).unwrap_or_default();
        for mx in masts {
            let lx = mx - 1;
            // Cut a hole in each hold floor at the ladder cell so it's a continuous shaft.
            if let Some(a) = &air {
                for &fy in &hold_floors {
                    ctx.editor.place_block_forced(a, dc.placement.to_world(Point3D::new(lx, fy, 0))).await;
                    state.hatch_cells.push(Point3D::new(lx, fy, 0));
                }
            }
            // Ladder from just above the lowest floor up to the main hold floor (step-off points).
            for y in (lowest + 1)..=main_hold {
                if let Some(b) = string_to_block(&format!("minecraft:ladder[facing={face}]")) {
                    ctx.editor.place_block_forced(&b, dc.placement.to_world(Point3D::new(lx, y, 0))).await;
                }
            }
        }
    }
}

/// Build one **stair flight** down lane `cz`: a right-side-up lead-in at the deck level, a hatch
/// opening (trapdoor lids on the weather deck, opening to the side), and the descending steps —
/// **bottom step skipped** (you step straight onto the deck/laid floor) with an **upside-down stair
/// beneath each step** for a solid, sloped underside (gangway thickness).
#[allow(clippy::too_many_arguments)]
async fn place_stair_flight(
    ctx: &mut ShipCtx<'_>,
    dc: &DeckContext<'_>,
    state: &mut DeckState,
    level: &ShipLevel,
    hx: i32,
    cz: i32,
    top_y: i32,
    deck_mat: &MaterialId,
) {
    let place = dc.placement;
    let face = place.world_cardinal(ShipDir::Stern).to_string();
    // The trapdoor lid opens **to the side** (a beam direction) so it swings clear of the fore-aft
    // stairway instead of standing up in the path.
    let lid_face = place.world_cardinal(ShipDir::Starboard).to_string();
    let is_weather = level.ceiling_y == top_y;
    let air = string_to_block("minecraft:air");

    let mut rng = ctx.rng.derive();
    let mut placer = MaterialPlacer::new(Placer::new(&ctx.data.materials, &mut rng), deck_mat.clone());

    let rsu: HashMap<String, String> = HashMap::from([("facing".to_string(), face.clone())]);
    // The underside upside-down stairs face the **opposite** way (toward the bow) so their slope
    // mirrors the steps above into a clean, continuous sloped underside.
    let under_face = place.world_cardinal(ShipDir::Bow).to_string();
    let under: HashMap<String, String> =
        HashMap::from([("facing".to_string(), under_face), ("half".to_string(), "top".to_string())]);

    // Lead-in stair at the deck level, one station back toward the approach (right-side-up so it
    // steps down toward the hatch), with an upside-down stair beneath for underside thickness.
    placer
        .place_block_forced(ctx.editor, place.to_world(Point3D::new(hx - 1, level.ceiling_y, cz)), BlockForm::Stairs, Some(&rsu), None)
        .await;
    state.hatch_cells.push(Point3D::new(hx - 1, level.floor_y, cz));
    if level.ceiling_y - 1 > level.floor_y {
        placer
            .place_block_forced(ctx.editor, place.to_world(Point3D::new(hx - 1, level.ceiling_y - 1, cz)), BlockForm::Stairs, Some(&under), None)
            .await;
    }

    // Hatch opening over the top three steps (headroom to walk down).
    for x in [hx, hx + 1, hx + 2] {
        if is_weather {
            if let Some(td) = string_to_block(&format!(
                "minecraft:oak_trapdoor[half=top,open=false,facing={lid_face}]"
            )) {
                ctx.editor.place_block_forced(&td, place.to_world(Point3D::new(x, level.ceiling_y, cz))).await;
            }
        } else if let Some(a) = &air {
            ctx.editor.place_block_forced(a, place.to_world(Point3D::new(x, level.ceiling_y, cz))).await;
        }
        state.hatch_cells.push(Point3D::new(x, level.ceiling_y, cz));
    }

    // Steps from just under the deck down to **one above** the floor; the bottom step is skipped
    // (you step onto the deck/laid floor). Each gets an upside-down stair beneath for thickness.
    let drop = level.ceiling_y - level.floor_y;
    for i in 0..drop {
        let (x, y) = (hx + i, level.ceiling_y - 1 - i);
        if y <= level.floor_y {
            break;
        }
        placer
            .place_block_forced(ctx.editor, place.to_world(Point3D::new(x, y, cz)), BlockForm::Stairs, Some(&rsu), None)
            .await;
        state.hatch_cells.push(Point3D::new(x, level.floor_y, cz));
        if y - 1 > level.floor_y {
            placer
                .place_block_forced(ctx.editor, place.to_world(Point3D::new(x, y - 1, cz)), BlockForm::Stairs, Some(&under), None)
                .await;
        }
    }
}

/// Pick a connector: a stair flight if a clear straight run of `drop + 2` stations exists off the
/// centreline, else a ladder. Both sit at `z = cz != 0` to clear the keel-stepped masts.
fn pick_connector(level: &ShipLevel, drop: i32) -> Connector {
    let max_half = level.outline.iter().copied().max().unwrap_or(0);
    // Off-centre lane: as wide as the level allows, capped (and never 0 → clears the masts).
    let cz = (max_half / 2).clamp(1, 3);
    let need = drop + 2; // entry + steps + headroom run
    let n = level.outline.len() as i32;
    // Search for the run whose start is closest to mid-length.
    let mid = n / 2;
    let mut best: Option<(i32, i32)> = None; // (distance-to-mid, hx)
    for hx in 0..(n - need) {
        let ok = (hx..hx + need).all(|x| level.outline[x as usize] >= cz);
        if ok {
            let d = (hx + need / 2 - mid).abs();
            if best.map_or(true, |(bd, _)| d < bd) {
                best = Some((d, hx));
            }
        }
    }
    match best {
        // Start the flight one station in, so there's a landing edge to step from.
        Some((_, hx)) => Connector::Stair { hx: hx + 1, cz },
        None => {
            // Ladder: any off-centre station with deck (prefer mid-length).
            let cx = (0..n)
                .filter(|&x| level.outline[x as usize] >= cz)
                .min_by_key(|&x| (x - mid).abs())
                .unwrap_or(mid);
            Connector::Ladder { cx, cz }
        }
    }
}
