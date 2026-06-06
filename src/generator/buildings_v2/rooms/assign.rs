//! Room type and role assignment. Picks a `RoomType` per (rect, floor) biased
//! by size class and a per-building `RoomBudget`, assigns ground-floor roles,
//! and resolves attic types once the roof/ladders are in place.

use crate::geometry::Rect2D;
use crate::noise::RNG;

use super::super::footprint::SizeClass;
use super::super::frame::Frame;
use super::super::{FloorType, RoomType};
use super::plan::{RoomPlan, RoomRole};

/// Compute wing size rank: maps each rect index to its rank among wings by area (0 = largest).
pub(super) fn wing_ranks(frame: &Frame) -> Vec<usize> {
    let rects = frame.footprint().rects();
    let mut wing_indices: Vec<usize> = (1..rects.len()).collect();
    wing_indices.sort_by(|&a, &b| rects[b].area().cmp(&rects[a].area()));
    let mut ranks = vec![0usize; rects.len()];
    for (rank, &idx) in wing_indices.iter().enumerate() {
        ranks[idx] = rank;
    }
    ranks
}

/// Assign a bedroom type if budget allows, otherwise a non-bedroom fallback.
fn try_bedroom(budget: &mut RoomBudget, rng: &mut RNG, room_type: RoomType) -> RoomType {
    if budget.needs_bedroom() {
        budget.add_bedroom();
        room_type
    } else if rng.chance(1, 2) {
        RoomType::Study
    } else {
        RoomType::Storage
    }
}

/// Pick a room type for a non-attic room based on size class, floor, and rect index.
pub(super) fn pick_room_type(
    size_class: SizeClass,
    floor: u32,
    rect_idx: usize,
    frame: &Frame,
    wing_rank: &[usize],
    rng: &mut RNG,
    budget: &mut RoomBudget,
) -> RoomType {
    match size_class {
        SizeClass::Cottage => {
            if rect_idx == 0 {
                RoomType::Common
            } else {
                try_bedroom(budget, rng, RoomType::Bedroom)
            }
        }
        SizeClass::House => {
            let num_rects = frame.rect_count();
            if frame.max_floors() == 1 {
                if num_rects == 1 {
                    RoomType::Common
                } else if rect_idx == 0 {
                    RoomType::Hearth
                } else {
                    try_bedroom(budget, rng, RoomType::Bedroom)
                }
            } else if floor == 0 {
                if rect_idx == 0 { RoomType::Hearth } else { RoomType::Storage }
            } else {
                try_bedroom(budget, rng, RoomType::Bedroom)
            }
        }
        SizeClass::Hall => {
            let ground_seq = [RoomType::Kitchen, RoomType::Pantry, RoomType::Storage];
            if floor == 0 {
                if rect_idx == 0 { RoomType::GreatRoom }
                else { *ground_seq.get(wing_rank[rect_idx]).unwrap_or(&RoomType::Storage) }
            } else if rect_idx == 0 {
                if budget.needs_bedroom() {
                    budget.add_bedroom();
                    // MultiBedroom counts as 2 toward the budget
                    budget.add_bedroom();
                    RoomType::MultiBedroom
                } else {
                    RoomType::Study
                }
            } else {
                let rank = wing_rank[rect_idx];
                if rank == 0 {
                    try_bedroom(budget, rng, RoomType::MasterBedroom)
                } else {
                    try_bedroom(budget, rng, RoomType::Bedroom)
                }
            }
        }
        SizeClass::Manor => {
            if floor == 0 {
                if rect_idx == 0 {
                    RoomType::Hearth
                } else if !budget.dining && rng.chance(1, 2) {
                    budget.dining = true;
                    RoomType::Dining
                } else {
                    RoomType::Storage
                }
            } else {
                if budget.bedrooms == 0 && budget.needs_bedroom() {
                    budget.add_bedroom();
                    RoomType::Bedroom
                } else if !budget.library && rng.chance(1, 5) {
                    budget.library = true;
                    RoomType::Library
                } else if !budget.studio && rng.chance(1, 5) {
                    budget.studio = true;
                    RoomType::Studio
                } else if !budget.armory && rng.chance(1, 5) {
                    budget.armory = true;
                    RoomType::Armory
                } else if !budget.study && rng.chance(1, 4) {
                    budget.study = true;
                    RoomType::Study
                } else {
                    try_bedroom(budget, rng, RoomType::Bedroom)
                }
            }
        }
    }
}

/// Tracks bedroom count and unique room assignments across the building.
pub(super) struct RoomBudget {
    bedrooms: u32,
    target_bedrooms: u32,
    dining: bool,
    study: bool,
    library: bool,
    studio: bool,
    armory: bool,
}

impl RoomBudget {
    pub(super) fn new(size_class: SizeClass, rng: &mut RNG) -> Self {
        let target = rng.rand_i32_range(
            size_class.min_bedrooms() as i32,
            size_class.max_bedrooms() as i32 + 1,
        ) as u32;
        Self {
            bedrooms: 0,
            target_bedrooms: target,
            dining: false,
            study: false,
            library: false,
            studio: false,
            armory: false,
        }
    }

    pub(super) fn needs_bedroom(&self) -> bool {
        self.bedrooms < self.target_bedrooms
    }

    pub(super) fn add_bedroom(&mut self) {
        self.bedrooms += 1;
    }
}

fn is_bedroom_type(room_type: RoomType) -> bool {
    matches!(room_type, RoomType::Bedroom | RoomType::MultiBedroom | RoomType::MasterBedroom)
}

/// Assign roles to active rects on a given floor.
pub(super) fn assign_roles(
    rects: &[Rect2D],
    active_indices: &[usize],
    floor: u32,
    entry_rect: Option<usize>,
) -> Vec<(usize, RoomRole)> {
    if floor > 0 {
        return active_indices.iter().map(|&i| (i, RoomRole::Upper)).collect();
    }

    let mut assignments: Vec<(usize, RoomRole)> = Vec::new();
    let mut entry_assigned = false;

    // Entry goes to the rect containing the door
    if let Some(entry_idx) = entry_rect {
        if active_indices.contains(&entry_idx) {
            assignments.push((entry_idx, RoomRole::Entry));
            entry_assigned = true;
        }
    }

    // Main goes to the largest remaining rect
    let remaining: Vec<usize> = active_indices
        .iter()
        .filter(|&&i| !assignments.iter().any(|(ai, _)| *ai == i))
        .copied()
        .collect();

    if let Some(&main_idx) = remaining.iter().max_by_key(|&&i| rects[i].area()) {
        if !entry_assigned {
            // No door found — treat the largest room as entry
            assignments.push((main_idx, RoomRole::Entry));
        } else {
            assignments.push((main_idx, RoomRole::Main));
        }
    }

    // Rest are secondary
    for &i in active_indices {
        if !assignments.iter().any(|(ai, _)| *ai == i) {
            assignments.push((i, RoomRole::Secondary));
        }
    }

    assignments
}

/// Assign types to attic rooms using the building's bedroom budget.
/// Attics above bedrooms stay Storage (redundant sleeping space).
/// Attics above non-bedrooms may become bedrooms if the budget allows.
/// Call after `place_attic_ladders` so all attic rects are accessible.
pub fn assign_attic_types(room_plan: &mut RoomPlan, size_class: SizeClass, rng: &mut RNG) {
    // Count bedrooms already assigned to non-attic rooms
    let existing = room_plan.rooms.iter()
        .filter(|r| r.role != RoomRole::Attic && is_bedroom_type(r.room_type))
        .map(|r| if r.room_type == RoomType::MultiBedroom { 2u32 } else { 1 })
        .sum::<u32>();

    let target = rng.rand_i32_range(
        size_class.min_bedrooms() as i32,
        size_class.max_bedrooms() as i32 + 1,
    ) as u32;
    let mut remaining = target.saturating_sub(existing);

    for i in 0..room_plan.rooms.len() {
        let room = &room_plan.rooms[i];
        if room.role != RoomRole::Attic { continue; }
        let rect_idx = room.rect_index;
        let floor = room.floor;
        let below_is_bedroom = room_plan.rooms.iter()
            .find(|r| r.rect_index == rect_idx && r.floor + 1 == floor)
            .map(|r| is_bedroom_type(r.room_type))
            .unwrap_or(false);

        room_plan.rooms[i].room_type = if below_is_bedroom {
            // Attic above a bedroom — no need for another bedroom here
            RoomType::Storage
        } else if remaining > 0 {
            remaining -= 1;
            RoomType::Bedroom
        } else {
            RoomType::Storage
        };
    }
}

/// Assign all room types (non-attic + attic). Used by tests that construct
/// rooms manually without going through build_rooms.
pub fn assign_types_to_rooms(
    room_plan: &mut RoomPlan,
    frame: &Frame,
    size_class: SizeClass,
    rng: &mut RNG,
) {
    let ranks = wing_ranks(frame);
    let mut budget = RoomBudget::new(size_class, rng);

    let mut indices: Vec<usize> = (0..room_plan.rooms.len()).collect();
    indices.sort_by_key(|&i| (room_plan.rooms[i].floor, room_plan.rooms[i].rect_index));

    for &i in &indices {
        let room = &room_plan.rooms[i];
        if room.role == RoomRole::Attic { continue; }
        let room_type = pick_room_type(
            size_class, room.floor, room.rect_index,
            frame, &ranks, rng, &mut budget,
        );
        room_plan.rooms[i].room_type = room_type;
    }

    assign_attic_types(room_plan, size_class, rng);
}

/// Assign custom floor types to rooms based on their room type.
pub fn assign_room_floors(room_plan: &mut RoomPlan) {
    for room in &mut room_plan.rooms {
        room.floor_type = match room.room_type {
            RoomType::Kitchen => Some(FloorType::Kitchen),
            _ => None,
        };
    }
}
