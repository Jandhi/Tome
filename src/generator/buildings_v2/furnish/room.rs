//! Room furnishing driver: resolves rooms.yaml entries to candidate items,
//! places required then optional furniture (packing fill-threshold rooms), and
//! walks every room in a building.

use std::collections::{HashMap, HashSet};

use crate::editor::Editor;
use crate::generator::materials::{Material, MaterialId, Palette};
use crate::geometry::Rect2D;
use crate::noise::RNG;

use super::block::swap_block_for_palette;
use super::data::{Furniture, LootTable, RoomFurnitureList};
use super::loot::roll_loot_snbt;
use super::placement::{
    RoofClearance, WallSlot, interior_rect, is_ceiling_item, needs_wall, try_place_at_wall_slot,
    try_place_ceiling, try_place_freestanding, wall_slots,
};
use super::super::frame::Frame;
use super::super::pipeline::BuildCtx;
use super::super::roof::heightmap::RoofHeightmap;
use super::super::rooms::{CellState, ConstraintMap, PlacedFurniture, Room, RoomPlan, RoomRole};

/// Default fraction of interior cells filled before stopping optional placement.
/// Room types with `fill_threshold` set in rooms.yaml override this.
pub(super) const DEFAULT_FILL_THRESHOLD: f32 = 0.75;

/// Try to place a single furniture item. Returns the occupied cells if placed.
pub(super) async fn try_place_item(
    editor: &Editor,
    item: &Furniture,
    interior: &Rect2D,
    constraints: &mut ConstraintMap,
    slots: &[WallSlot],
    open_cells: &[(i32, i32)],
    floor_y: i32,
    ceiling_y: i32,
    roof_clearance: Option<&RoofClearance<'_>>,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    loot_tables: &HashMap<String, LootTable>,
    rng: &mut RNG,
) -> Option<Vec<(i32, i32)>> {
    let result = if is_ceiling_item(item) {
        try_place_ceiling(item, interior, constraints, ceiling_y)
    } else if needs_wall(item) {
        let mut found = None;
        for slot in slots {
            if let Some(r) = try_place_at_wall_slot(item, slot, interior, constraints, floor_y, roof_clearance) {
                found = Some(r);
                break;
            }
        }
        found
    } else {
        try_place_freestanding(item, interior, constraints, floor_y, open_cells, roof_clearance)
    };

    if let Some(placement) = result {
        let mut cells = Vec::new();
        for &cell in &placement.new_blocked { constraints.set(cell, CellState::Blocked); }
        for &cell in &placement.new_reserved { constraints.set(cell, CellState::BlockedReachable); }
        for &cell in &placement.new_empty_reachable { constraints.set(cell, CellState::UnblockedReachable); }
        for rb in &placement.blocks {
            if rb.place {
                let mut block = swap_block_for_palette(rb.block.clone(), rb.swap, palette, materials, rng);
                if let Some(loot_name) = &rb.loot {
                    if let Some(table) = loot_tables.get(loot_name) {
                        block.data = Some(roll_loot_snbt(table, rng));
                    }
                }
                editor.place_block(&block, rb.world_pos).await;
            }
            if rb.layer.occupies_ceiling() {
                constraints.set_ceiling(rb.cell);
            }
            if rb.layer.occupies_ground() {
                let state = if rb.walkable { CellState::UnblockedReachable } else { CellState::Blocked };
                constraints.set(rb.cell, state);
                cells.push(rb.cell);
            }
        }
        // Wall-constraint cells that don't have an explicit block still belong
        // to the item — e.g. the bed head, which Minecraft auto-generates from
        // the foot. Add them to the returned cells so the blueprint shows them.
        for &cell in &placement.new_blocked {
            if !cells.contains(&cell) {
                cells.push(cell);
            }
        }
        Some(cells)
    } else {
        None
    }
}


/// Every tag that identifies an item — its own name (implicit self-tag)
/// plus any explicit tags declared in YAML.
fn item_tags<'a>(name: &'a str, item: &'a Furniture) -> impl Iterator<Item = &'a str> {
    std::iter::once(name).chain(item.tags.iter().map(String::as_str))
}

/// Resolve a rooms.yaml entry (like `bed` or `chair`) to every eligible
/// furniture item. Candidates match by name or explicit tag, pass the
/// room-area gates, aren't ceiling items in an attic, and — if unique —
/// don't share any tag with an already-placed unique item.
pub(super) fn resolve_candidates<'a>(
    entry: &str,
    items: &'a HashMap<String, Furniture>,
    room_area: i32,
    is_attic: bool,
    placed_tags: &HashSet<String>,
    rng: &mut RNG,
) -> Vec<(&'a String, &'a Furniture)> {
    let mut out: Vec<(&String, &Furniture)> = items.iter()
        .filter(|(name, item)| {
            name.as_str() == entry || item.tags.iter().any(|t| t == entry)
        })
        .filter(|(_, item)| {
            item.min_room_area.map_or(true, |min| room_area >= min)
                && item.max_room_area.map_or(true, |max| room_area <= max)
        })
        .filter(|(_, item)| !(is_attic && is_ceiling_item(item)))
        .filter(|(name, item)| {
            !item.unique || !item_tags(name, item).any(|t| placed_tags.contains(t))
        })
        .collect();
    // HashMap iteration order is non-deterministic — sort before shuffling
    // so the RNG draw is the only source of randomness.
    out.sort_by(|a, b| a.0.cmp(b.0));
    weighted_shuffle(&mut out, rng, |(_, item)| item.weight);
    out
}

/// Place furniture in a single room.
pub(super) async fn furnish_room(
    editor: &Editor,
    room: &mut Room,
    frame: &Frame,
    room_list: &RoomFurnitureList,
    items: &HashMap<String, Furniture>,
    roof_heightmap: Option<&RoofHeightmap>,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    loot_tables: &HashMap<String, LootTable>,
    rng: &mut RNG,
) {
    let interior = match interior_rect(room) {
        Some(r) => r,
        Option::None => return,
    };

    let floor_y = frame.floor_y(room.floor);
    let ceiling_y = if room.role == RoomRole::Attic {
        frame.roof_y(room.rect_index)
    } else {
        frame.ceiling_y(room.floor)
    };
    let mut slots = wall_slots(&interior);
    shuffle(&mut slots, rng);

    let mut open_cells: Vec<(i32, i32)> = interior.iter().map(|p| (p.x, p.y)).collect();
    shuffle(&mut open_cells, rng);

    let room_area = interior.area();
    let is_attic = room.role == RoomRole::Attic;
    // Only attics have a sloped roof above the room — flat-ceiling rooms get
    // None and skip the per-cell clearance check entirely.
    let roof_clearance: Option<RoofClearance> = if is_attic {
        roof_heightmap.map(|hm| RoofClearance { hm, roof_y: frame.roof_y(room.rect_index) })
    } else {
        None
    };
    let mut placed_tags: HashSet<String> = HashSet::new();

    for entry in &room_list.required {
        let candidates = resolve_candidates(entry, items, room_area, is_attic, &placed_tags, rng);
        for (name, item) in candidates {
            if let Some(cells) = try_place_item(
                editor, item, &interior, &mut room.constraints,
                &slots, &open_cells, floor_y, ceiling_y,
                roof_clearance.as_ref(),
                palette, materials, loot_tables, rng,
            ).await {
                if item.unique {
                    for tag in item_tags(name, item) {
                        placed_tags.insert(tag.to_string());
                    }
                }
                room.furniture.push(PlacedFurniture { name: name.clone(), cells });
                break;
            }
        }
    }

    let fill_threshold = room_list.fill_threshold.unwrap_or(DEFAULT_FILL_THRESHOLD);
    // Rooms that explicitly set a threshold (storage, pantry) run the
    // optional list in repeated passes until nothing more fits — packing
    // them until the threshold is hit or a full pass places nothing.
    let aggressive = room_list.fill_threshold.is_some();

    loop {
        if room.constraints.fill_ratio() >= fill_threshold {
            break;
        }
        let mut placed_this_pass = false;
        for entry in &room_list.optional {
            if room.constraints.fill_ratio() >= fill_threshold { break; }
            let candidates = resolve_candidates(entry, items, room_area, is_attic, &placed_tags, rng);
            for (name, item) in candidates {
                if let Some(cells) = try_place_item(
                    editor, item, &interior, &mut room.constraints,
                    &slots, &open_cells, floor_y, ceiling_y,
                    roof_clearance.as_ref(),
                    palette, materials, loot_tables, rng,
                ).await {
                    if item.unique {
                        for tag in item_tags(name, item) {
                            placed_tags.insert(tag.to_string());
                        }
                    }
                    room.furniture.push(PlacedFurniture { name: name.clone(), cells });
                    placed_this_pass = true;
                    break;
                }
            }
        }
        if !aggressive || !placed_this_pass {
            break;
        }
    }
}

/// Furnish all rooms in a building using loaded furniture data.
/// `roof_heightmaps` is indexed by rect — used only by attic rooms to clamp
/// furniture against the sloped roof above the attic floor.
pub async fn furnish_rooms(
    ctx: &mut BuildCtx<'_>,
    room_plan: &mut RoomPlan,
    frame: &Frame,
    roof_heightmaps: &[RoofHeightmap],
) {
    let editor: &Editor = &*ctx.editor;
    let palette = ctx.palette;
    let furniture_data = &ctx.data.furniture;
    let materials = &ctx.data.materials;
    let rng = &mut *ctx.rng;

    for room in &mut room_plan.rooms {
        let key = room.room_type.furniture_key();
        let room_list = match furniture_data.rooms.get(key) {
            Some(r) => r,
            None => continue,
        };
        let mut room_rng = rng.derive();
        let roof_hm = roof_heightmaps.get(room.rect_index);
        furnish_room(
            editor, room, frame, room_list, &furniture_data.items,
            roof_hm,
            palette, materials, &furniture_data.loot, &mut room_rng,
        ).await;
    }
}

pub(super) fn shuffle<T>(items: &mut [T], rng: &mut RNG) {
    for i in (1..items.len()).rev() {
        let j = rng.rand_i32_range(0, (i + 1) as i32) as usize;
        items.swap(i, j);
    }
}

/// Weighted shuffle (Efraimidis-Spirakis): assign each item a key
/// `-ln(u) / weight` and sort ascending. Higher weights → more likely
/// to land near the front. Items with weight ≤ 0 always sort to the end.
fn weighted_shuffle<T, F>(items: &mut [T], rng: &mut RNG, weight_of: F)
where
    F: Fn(&T) -> f32,
{
    let mut keys: Vec<f32> = (0..items.len())
        .map(|i| {
            let w = weight_of(&items[i]).max(0.0);
            if w <= 0.0 {
                f32::INFINITY
            } else {
                let u = ((rng.rand_i32(1_000_000) as f32 + 1.0) / 1_000_001.0).min(1.0);
                -u.ln() / w
            }
        })
        .collect();
    // In-place sort of items by paired keys: insertion sort is fine since
    // candidate lists are tiny (typically <10 entries).
    for i in 1..items.len() {
        let mut j = i;
        while j > 0 && keys[j - 1] > keys[j] {
            items.swap(j - 1, j);
            keys.swap(j - 1, j);
            j -= 1;
        }
    }
}
