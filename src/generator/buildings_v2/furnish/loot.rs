//! Loot rolling: turns a `LootTable` into an SNBT `{Items:[...]}` payload for
//! container blocks (chests, barrels, furnaces).

use crate::noise::RNG;

use super::data::{self, LootTable};

/// Default container slot capacity (chest, barrel). Overridable per-table.
const DEFAULT_LOOT_CAPACITY: i32 = 27;

/// Roll a weighted pick from a list of loot items.
fn pick_weighted_item<'a>(items: &'a [data::LootItem], rng: &mut RNG) -> Option<&'a data::LootItem> {
    if items.is_empty() { return None; }
    let total: f32 = items.iter().map(|i| i.weight.max(0.0)).sum();
    if total <= 0.0 { return None; }
    let mut r = (rng.rand_i32(100_000) as f32 / 100_000.0) * total;
    for it in items {
        let w = it.weight.max(0.0);
        if r < w { return Some(it); }
        r -= w;
    }
    items.last()
}

/// Roll an inclusive [min, max] range safely when min == max.
fn roll_range_inclusive(range: [i32; 2], rng: &mut RNG) -> i32 {
    let (lo, hi) = (range[0].min(range[1]), range[0].max(range[1]));
    if lo == hi { lo } else { rng.rand_i32_range(lo, hi + 1) }
}

/// Roll an SNBT `{Items:[...]}` payload for a container from a loot table.
pub(super) fn roll_loot_snbt(table: &LootTable, rng: &mut RNG) -> String {
    let mut entries: Vec<(i32, String, i32)> = Vec::new();

    if !table.fixed.is_empty() {
        // Fixed strategy: furnace/smoker style, each slot rolled independently.
        for fs in &table.fixed {
            let chance = fs.chance.clamp(0.0, 1.0);
            if chance < 1.0 {
                let roll = rng.rand_i32(100_000) as f32 / 100_000.0;
                if roll >= chance { continue; }
            }
            if let Some(item) = pick_weighted_item(&fs.items, rng) {
                let count = roll_range_inclusive(item.count, rng).max(1);
                entries.push((fs.slot, item.id.clone(), count));
            }
        }
    } else if !table.items.is_empty() {
        // Random strategy: roll N stacks into distinct random slot indices.
        let count_range = table.count.unwrap_or([1, 3]);
        let n = roll_range_inclusive(count_range, rng).max(0) as usize;
        let capacity = table.capacity.unwrap_or(DEFAULT_LOOT_CAPACITY).max(1);
        let mut slot_pool: Vec<i32> = (0..capacity).collect();
        let take = n.min(slot_pool.len());
        for _ in 0..take {
            let idx = rng.rand_i32(slot_pool.len() as i32) as usize;
            let slot = slot_pool.swap_remove(idx);
            if let Some(item) = pick_weighted_item(&table.items, rng) {
                let count = roll_range_inclusive(item.count, rng).max(1);
                entries.push((slot, item.id.clone(), count));
            }
        }
    }

    let parts: Vec<String> = entries
        .iter()
        .map(|(slot, id, count)| format!("{{Slot:{}b,id:\"{}\",Count:{}b}}", slot, id, count))
        .collect();
    format!("{{Items:[{}]}}", parts.join(","))
}
