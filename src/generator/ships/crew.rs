//! Ship crew — seat sailor + captain NPCs on an afloat ship's weather deck.
//!
//! Ships are unmanned geometry; this turns a finished [`ShipOutput`] into a set of
//! [`AnchorScene`]s (the shared NPC-placement unit), which the settlement layer staffs with
//! the town roster exactly like plaza vendors or industry workers. The captain takes the
//! helm; sailors stand watch along the deck, off the centreline (clear of masts, the helm,
//! and hatches). Land "hulk" ships are bare wrecks and get no crew.
//!
//! Looks and dialogue are data-driven: the `sailors` / `captains` fixtures in `npcs.yaml`
//! supply the skin pool (rolled per crew member) and the job label (which doubles as the
//! dialogue key). Names come from the roster the settlement layer hands the scenes.

use std::collections::HashSet;

use crate::generator::population::{yaw_toward, AnchorScene, NpcData};
use crate::geometry::Point3D;
use crate::noise::RNG;

use super::additions::SizeTier;
use super::ShipOutput;

/// Sailors (*beyond* the lone captain) a ship carries, by size tier — capped at placement
/// to the deck cells that actually fit. Small boats run a skeleton crew; a great ship is
/// fully manned.
fn sailor_count(tier: SizeTier) -> usize {
    match tier {
        SizeTier::Small => 1,
        SizeTier::Medium => 2,
        SizeTier::Large => 4,
        SizeTier::Huge => 6,
    }
}

/// Build the crew [`AnchorScene`]s for one ship: a captain at the helm and a tier-scaled
/// gang of sailors spread along the weather deck. Empty for land hulks (`!on_water`). Looks
/// are rolled from the `sailors` / `captains` fixtures; the employment label doubles as the
/// dialogue key. `rng` should be derived per ship so the crew is deterministic per seed.
pub fn crew_scenes(out: &ShipOutput, npc_data: &NpcData, rng: &mut RNG) -> Vec<AnchorScene> {
    if !out.on_water {
        return Vec::new(); // land hulks stay crewless
    }

    let place = &out.placement;
    let feet_y = out.weather_deck_y + 1; // stand on top of the deck floor

    let mut scenes: Vec<AnchorScene> = Vec::new();
    let mut taken: HashSet<(i32, i32)> = HashSet::new();

    // Cells crew never stand on: companionway hatches (any height), mast columns, and the
    // bulwark + fence rail. The railing follows the **perimeter** of the deck outline, which
    // wraps *inboard* as the hull tapers at the bow/stern — so an inset `|z| <= half - 1`
    // alone still clips it there. Exclude its actual cells outright (`ShipOutput::railing`).
    let hatches: HashSet<(i32, i32)> = out.hatch_cells.iter().map(|c| (c.x, c.z)).collect();
    let mast_xs: HashSet<i32> = out
        .masts
        .as_ref()
        .map(|m| m.masts.iter().map(|mm| mm.base_x).collect())
        .unwrap_or_default();
    let rail: HashSet<(i32, i32)> = out
        .railing
        .as_ref()
        .map(|r| r.bulwark.iter().map(|c| (c.x, c.z)).collect())
        .unwrap_or_default();

    // --- Captain at the helm, facing the bow over the wheel. ---
    if let Some(stand) = out.helm_stand {
        let feet = place.to_world(Point3D::new(stand.x, feet_y, stand.z));
        let target = place.to_world(Point3D::new(stand.x + 1, feet_y, stand.z)); // one cell toward the bow
        let look = *rng.choose(&npc_data.captains.looks);
        scenes.push(AnchorScene::worker_titled(
            feet,
            yaw_toward(feet, target),
            look,
            &npc_data.captains.employment,
            Some("Captain".to_string()),
        ));
        taken.insert((stand.x, stand.z));
    }

    // --- Sailors spread along the deck, off the centreline. ---
    // Candidates: interior deck cells at |z| >= 1 (the centreline carries masts / helm /
    // hatches), skipping mast columns, hatch cells, and the railing. Ordered bow→stern so a
    // deterministic stride spreads the gang out.
    let mut candidates: Vec<(i32, i32)> = Vec::new();
    for (x, &half) in out.top_outline.iter().enumerate() {
        let x = x as i32;
        if mast_xs.contains(&x) {
            continue;
        }
        for z in 1..=half {
            for &cell in &[(x, z), (x, -z)] {
                if !rail.contains(&cell)
                    && !hatches.contains(&cell)
                    && !taken.contains(&cell)
                {
                    candidates.push(cell);
                }
            }
        }
    }

    let want = sailor_count(out.tier).min(candidates.len());
    if want > 0 {
        let stride = (candidates.len() / want).max(1);
        let mut placed = 0;
        let mut i = 0;
        while placed < want && i < candidates.len() {
            let cell = candidates[i];
            if taken.insert(cell) {
                let feet = place.to_world(Point3D::new(cell.0, feet_y, cell.1));
                // Face inboard (toward the centreline) so a watch reads as looking across the deck.
                let target = place.to_world(Point3D::new(cell.0, feet_y, 0));
                let look = *rng.choose(&npc_data.sailors.looks);
                scenes.push(AnchorScene::worker(
                    feet,
                    yaw_toward(feet, target),
                    look,
                    &npc_data.sailors.employment,
                ));
                placed += 1;
            }
            i += stride;
        }
    }

    scenes
}
