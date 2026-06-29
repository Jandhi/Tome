//! Ship crew — seat sailor + captain NPCs on an afloat ship's weather deck.
//!
//! Ships are unmanned geometry; this turns a finished [`ShipOutput`] into a set of
//! [`AnchorScene`]s (the shared NPC-placement unit), which the settlement layer staffs with
//! the town roster exactly like plaza vendors or industry workers. The captain takes the
//! helm; sailors stand watch along the deck, off the centreline (clear of masts, the helm,
//! and hatches). Land "hulk" ships are bare wrecks and get no crew.
//!
//! Looks, dialogue, and rank are data-driven: the `sailors` / `captains` fixtures in
//! `npcs.yaml` supply the skin pool (rolled per crew member), the job label (which doubles
//! as the dialogue key), and the captain's `title`. Crew *counts* live in [`tuning`]
//! ([`CREW_SAILORS_SMALL`] …). Names come from the roster the settlement layer hands the
//! scenes.

use std::collections::HashSet;

use crate::editor::Editor;
use crate::generator::buildings_v2::Culture;
use crate::generator::data::LoadedData;
use crate::generator::population::{
    build_roster, populate_npcs, yaw_toward, AnchorScene, Fixture, IdAllocator, NpcData,
};
use crate::geometry::Point3D;
use crate::noise::RNG;

use super::additions::SizeTier;
use super::tuning::{CREW_SAILORS_HUGE, CREW_SAILORS_LARGE, CREW_SAILORS_MEDIUM, CREW_SAILORS_SMALL};
use super::ShipOutput;

/// Sailors (*beyond* the lone captain) a ship carries, by size tier. The dial lives on the
/// central tuning surface ([`tuning`](super::tuning)).
fn sailor_count(tier: SizeTier) -> usize {
    match tier {
        SizeTier::Small => CREW_SAILORS_SMALL,
        SizeTier::Medium => CREW_SAILORS_MEDIUM,
        SizeTier::Large => CREW_SAILORS_LARGE,
        SizeTier::Huge => CREW_SAILORS_HUGE,
    }
}

/// Build the crew [`AnchorScene`]s for one ship: a captain at the helm and a tier-scaled
/// gang of sailors spread along the weather deck. Empty for land hulks (`!on_water`). Looks
/// and rank are rolled from the `sailors` / `captains` fixtures; the employment label doubles
/// as the dialogue key. `rng` should be derived per ship so the crew is deterministic per seed.
pub fn crew_scenes(out: &ShipOutput, npc_data: &NpcData, rng: &mut RNG) -> Vec<AnchorScene> {
    if !out.on_water {
        return Vec::new(); // land hulks stay crewless
    }

    let place = &out.placement;
    let feet_y = out.weather_deck_y + 1; // stand on top of the deck floor

    // Seat one Worker NPC standing on local `cell`, facing local `face`, drawn from `fixture`
    // (skin pool + employment/dialogue label + optional rank). Captures only the immutable
    // placement, so it never conflicts with the `rng` / `scenes` borrows at the call sites.
    let seat = |cell: (i32, i32), face: (i32, i32), fixture: &Fixture, rng: &mut RNG| {
        let feet = place.to_world(Point3D::new(cell.0, feet_y, cell.1));
        let look_at = place.to_world(Point3D::new(face.0, feet_y, face.1));
        let look = *rng.choose(&fixture.looks);
        AnchorScene::worker_titled(
            feet,
            yaw_toward(feet, look_at),
            look,
            &fixture.employment,
            fixture.title.clone(),
        )
    };

    let mut scenes: Vec<AnchorScene> = Vec::new();

    // Cells crew never stand on: companionway hatches (any height), mast columns, and the
    // bulwark + fence rail. The railing follows the **perimeter** of the deck outline, which
    // wraps *inboard* as the hull tapers at the bow/stern — so an inset `|z| <= half - 1`
    // alone still clips it there. Exclude its actual cells outright (`ShipOutput::railing`).
    let hatches: HashSet<(i32, i32)> = out.hatch_cells.iter().map(|c| (c.x, c.z)).collect();
    let mast_xs: HashSet<i32> =
        out.masts.as_ref().map(|m| m.base_xs().into_iter().collect()).unwrap_or_default();
    let rail: HashSet<(i32, i32)> = out
        .railing
        .as_ref()
        .map(|r| r.bulwark.iter().map(|c| (c.x, c.z)).collect())
        .unwrap_or_default();

    // --- Captain at the helm, facing the bow over the wheel. ---
    let helm_cell = out.helm_stand.map(|s| (s.x, s.z));
    if let Some((hx, hz)) = helm_cell {
        scenes.push(seat((hx, hz), (hx + 1, hz), &npc_data.captains, rng));
    }

    // --- Sailors spread along the deck, off the centreline. ---
    // Candidates: interior deck cells at |z| >= 1 (the centreline carries masts / helm /
    // hatches), skipping mast columns, hatch cells, the railing, and the captain's cell.
    // Each (x, z) is pushed once, ordered bow→stern, so a deterministic stride spreads the gang.
    let mut candidates: Vec<(i32, i32)> = Vec::new();
    for (x, &half) in out.top_outline.iter().enumerate() {
        let x = x as i32;
        if mast_xs.contains(&x) {
            continue;
        }
        for z in 1..=half {
            for &cell in &[(x, z), (x, -z)] {
                if !rail.contains(&cell) && !hatches.contains(&cell) && Some(cell) != helm_cell {
                    candidates.push(cell);
                }
            }
        }
    }

    let want = sailor_count(out.tier).min(candidates.len());
    if want > 0 {
        // Even stride across the unique candidates; `(want - 1) * stride < len` always, so
        // every index is in bounds and each sailor lands on a distinct cell.
        let stride = (candidates.len() / want).max(1);
        for k in 0..want {
            let cell = candidates[k * stride];
            scenes.push(seat(cell, (cell.0, 0), &npc_data.sailors, rng)); // face inboard
        }
    }

    scenes
}

/// Staff crew [`AnchorScene`]s (from [`crew_scenes`]) onto a live world from a fresh roster —
/// the shared fixture path ([`build_roster`] + [`populate_npcs`]). Both the settlement
/// pipeline and the live ship test go through here so they exercise the same code. `id_alloc`
/// is the town-wide allocator, so crew ids never collide with residents'. No-op offline
/// (entities don't spawn), returning `0`.
pub async fn staff_crew(
    editor: &Editor,
    crew: Vec<AnchorScene>,
    culture: Culture,
    data: &LoadedData,
    id_alloc: &mut IdAllocator,
    rng: &mut RNG,
) -> anyhow::Result<usize> {
    if crew.is_empty() {
        return Ok(0);
    }
    let budget = crew.len();
    let roster = build_roster(budget, 0, culture, &data.npc_data, id_alloc, &mut rng.derive());
    populate_npcs(editor, crew, roster, budget, &data.npc_data, rng).await
}
