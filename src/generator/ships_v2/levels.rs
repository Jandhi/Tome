//! Stage 3 · **Interior levels** — pure geometry enumerating the ship's stacked enclosed spaces,
//! the shared prerequisite for inter-level connections (stairs/ladders) and furnishing.
//!
//! Each [`ShipLevel`] is a horizontal slab of interior: a `floor_y`, a `ceiling_y`, and an
//! `outline` (half-beam per station) — the footprint a furnishable rect / bulkhead is carved from.
//! Built from the existing hull/deck models, no placement.
//!
//! Levels, bottom → top:
//!   - **Hold** — inside the hull below the main deck. The hull bottom is curved/narrow, so the
//!     floor is laid on the flat part (`HOLD_KEEL_CLEARANCE` above the keel, but never more than
//!     `HOLD_MAX_HEIGHT` below the deck); ceiling = the main-deck slabs.
//!   - **Gun deck** ('tween) — the space between the main deck and a **raised additional deck**
//!     (only present on ships that have one). Floor = main deck, ceiling = the additional-deck floor.
//!
//! The open weather deck on top is *not* a level here — it carries masts/helm/rigging, not rooms.

use super::hull::HullModel;
use super::tuning::{HOLD_KEEL_CLEARANCE, HOLD_MAX_HEIGHT, LEVEL_MIN_HEADROOM};

/// One enclosed horizontal interior space (local frame). `outline[x]` is the interior half-beam at
/// station `x` on the **floor** — the footprint to furnish / divide with bulkheads.
#[derive(Debug, Clone)]
pub struct ShipLevel {
    /// Stable key (also the furniture room-list key once Stage-3 furnishing lands).
    pub name: &'static str,
    /// Local Y the floor is laid at (planks go here; the room stands `floor_y + 1 ..= ceiling_y - 1`).
    pub floor_y: i32,
    /// Local Y of the ceiling (the deck above).
    pub ceiling_y: i32,
    /// Interior half-beam per station at the floor level (`length` entries; `0` = no deck there).
    pub outline: Vec<i32>,
}

impl ShipLevel {
    /// Standable headroom (`ceiling_y - floor_y`).
    pub fn headroom(&self) -> i32 {
        self.ceiling_y - self.floor_y
    }
}

/// The ship's enclosed interior levels, bottom → top.
#[derive(Debug, Clone)]
pub struct ShipLevels {
    pub levels: Vec<ShipLevel>,
}

/// Interior **half-beam per station** at local height `y` — the widest `|z|` of any interior cell
/// in that layer, per station. `0` where the layer has no interior.
fn half_beam_at(hull: &HullModel, y: i32) -> Vec<i32> {
    let length = hull.top_half.len();
    let mut out = vec![0; length];
    for c in &hull.interior {
        if c.y == y && c.x >= 0 && (c.x as usize) < length {
            out[c.x as usize] = out[c.x as usize].max(c.z.abs());
        }
    }
    out
}

/// Enumerate the interior levels from the hull + deck geometry.
///
/// - `deck_y` — the main-deck floor (`DeckModel::deck_y`, also the hull's open top).
/// - `top_y` — the **topmost open weather deck** floor (`DeckState::top_y`). When a raised
///   additional deck exists, `top_y > deck_y` and the gap is the gun deck.
pub fn build_ship_levels(hull: &HullModel, deck_y: i32, top_y: i32) -> ShipLevels {
    let mut levels = Vec::new();

    // **Stacked hull levels** from the main deck down toward the keel: the **hold** just under the
    // deck, then **lower holds** below it for as long as the hull stays deep + wide enough (a deep
    // hull gets a multi-level cargo hold). Each is `HOLD_MAX_HEIGHT` tall (the last is whatever
    // remains above `HOLD_KEEL_CLEARANCE`).
    let base = HOLD_KEEL_CLEARANCE;
    let mut ceiling = deck_y;
    let mut idx = 0;
    while ceiling - base >= LEVEL_MIN_HEADROOM {
        let h = HOLD_MAX_HEIGHT.min(ceiling - base);
        let floor_y = ceiling - h;
        let outline = half_beam_at(hull, floor_y + 1);
        if !outline.iter().any(|&v| v >= 1) {
            break; // hull too narrow this far down — stop stacking
        }
        levels.push(ShipLevel {
            name: if idx == 0 { "hold" } else { "lower_hold" },
            floor_y,
            ceiling_y: ceiling,
            outline,
        });
        ceiling = floor_y;
        idx += 1;
    }

    // Gun deck ('tween): only when a raised additional deck sits above the main deck.
    if top_y - deck_y >= LEVEL_MIN_HEADROOM {
        levels.push(ShipLevel {
            name: "gun_deck",
            floor_y: deck_y,
            ceiling_y: top_y,
            // The tween walls follow the hull's waterline outline (the additional-deck base).
            outline: hull.top_half.clone(),
        });
    }

    ShipLevels { levels }
}

/// Compact per-level ASCII plan dump (top-down, `X` across → bow, `Z` down) for diagnostics — one
/// block per interior cell of the level's floor footprint. Returns a string (written by tests).
pub fn render_levels_ascii(levels: &ShipLevels) -> String {
    let mut s = String::new();
    for lvl in &levels.levels {
        let max_h = lvl.outline.iter().copied().max().unwrap_or(0);
        s.push_str(&format!(
            "--- {} (floor_y={}, ceiling_y={}, headroom={}) ---\n",
            lvl.name, lvl.floor_y, lvl.ceiling_y, lvl.headroom()
        ));
        for z in -max_h..=max_h {
            let row: String = lvl
                .outline
                .iter()
                .map(|&h| if h >= 1 && z.abs() <= h { '#' } else { '.' })
                .collect();
            s.push_str(&row);
            s.push('\n');
        }
        s.push('\n');
    }
    s
}
