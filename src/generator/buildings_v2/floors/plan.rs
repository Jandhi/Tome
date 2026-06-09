//! Floor/stair data types: `Stairwell`, the `StairKind` discriminant, and the
//! `FloorPlan` that records stair footprints, landings, tops, and head-clearance
//! cells for the rooms/furnish passes.

use std::collections::HashSet;

use crate::geometry::{Cardinal, Point2D};

/// Whether a stairwell is a straight run or a compact spiral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StairKind {
    Straight,
    Spiral,
    LShaped,
    /// A 1x1 vertical ladder. Fallback used only when no stair candidate keeps
    /// the interior-door approach lanes clear (or none fits at all): a ladder
    /// occupies a single cell, so it can never run along an approach lane the
    /// way a flush stair does. Its single `positions` cell is walkable
    /// (climb-through) on both the floor it starts on and the floor above.
    Ladder,
}

/// A stairwell connecting one floor to the floor above.
#[derive(Debug, Clone)]
pub struct Stairwell {
    /// The (x,z) positions occupied by the stairwell.
    /// Straight: position 0 is the landing, 1..=run are steps.
    /// Spiral: 4 cells in CW rotation order, each one step higher.
    pub positions: Vec<Point2D>,
    /// Floor index this stairwell starts on (goes up to floor + 1).
    pub floor: u32,
    /// Direction the stairs ascend toward (straight) or initial facing (spiral).
    pub direction: Cardinal,
    /// Stair type.
    pub kind: StairKind,
}

/// Result of floor/stair placement, consumed by the interior module.
pub struct FloorPlan {
    pub stairwells: Vec<Stairwell>,
    /// Bottom-of-stair landing cells: (floor, x, z). For straight stairs this is
    /// the flat landing cell at position 0. For spiral / L-shaped stairs there is
    /// no flat landing in the stair footprint, so this is the cell directly in
    /// front of the lowest step (one cell back from positions[0] in the direction
    /// opposite the ascent), where the player stands before stepping up.
    pub stair_bottoms: HashSet<(u32, i32, i32)>,
    /// Top-of-stair cells: (floor+1, x, z) for the last position of each stairwell.
    pub stair_tops: HashSet<(u32, i32, i32)>,
    /// All (floor+1, x, z) cells in the air column directly above any stair
    /// position. Even though stair blocks live on `floor`, the player ascends
    /// THROUGH the air at floor+1 — furniture placed at any of these cells on
    /// floor+1 would land in the player's head clearance during ascent.
    /// Includes stair_tops; non-top entries are mid-stair cells.
    pub stair_air_above: HashSet<(u32, i32, i32)>,
}

impl FloorPlan {
    pub fn new(stairwells: Vec<Stairwell>) -> Self {
        let mut stair_bottoms: HashSet<(u32, i32, i32)> = stairwells.iter()
            .filter_map(|sw| sw.positions.first().map(|p| (sw.floor, p.x, p.y)))
            .collect();
        // Spiral / L-shaped stairs have a stair block at positions[0], not a flat
        // landing — also reserve the cell in front of it so furniture can't block
        // the entry. The "front" is opposite the ascent direction (i.e. opposite
        // the second-lowest step from the lowest step).
        for sw in &stairwells {
            if let Some(approach) = sw.bottom_approach() {
                stair_bottoms.insert((sw.floor, approach.x, approach.y));
            }
        }
        let mut stair_tops: HashSet<(u32, i32, i32)> = stairwells.iter()
            .filter_map(|sw| sw.positions.last().map(|p| (sw.floor + 1, p.x, p.y)))
            .collect();
        for sw in &stairwells {
            if let Some(approach) = sw.top_approach() {
                stair_tops.insert((sw.floor + 1, approach.x, approach.y));
            }
        }
        let stair_air_above: HashSet<(u32, i32, i32)> = stairwells.iter()
            .flat_map(|sw| sw.positions.iter().map(move |p| (sw.floor + 1, p.x, p.y)))
            .collect();
        Self { stairwells, stair_bottoms, stair_tops, stair_air_above }
    }

    pub fn stairwells_on_floor(&self, floor: u32) -> Vec<&Stairwell> {
        self.stairwells.iter().filter(|s| s.floor == floor).collect()
    }

    /// All (x, z) cells occupied by the physical stair blocks of stairwells
    /// that START on the given floor. A cell returned here should be marked
    /// `Blocked` on that floor (except cells called out in `stair_bottoms`,
    /// which stay `BlockedReachable` so the approach/landing remains
    /// adjacent to walkable neighbors).
    ///
    /// Stairs that start on a different floor do **not** contribute — they
    /// have no physical presence on this floor. This is what distinguishes
    /// the main stair's cells on floor 0 from the attic stair's cells,
    /// which only exist on floor 1.
    pub fn stair_cells_on_floor(&self, floor: u32) -> HashSet<(i32, i32)> {
        self.stairwells.iter()
            .filter(|sw| sw.floor == floor)
            .flat_map(|sw| sw.positions.iter().map(|p| (p.x, p.y)))
            .collect()
    }
}

impl Stairwell {
    /// The cell on the lower floor where the player stands before stepping onto
    /// the lowest stair block. Straight stairs already have a flat landing at
    /// positions[0], so they return None. Spiral and L-shaped stairs need an
    /// extra cell — the one adjacent to positions[0] in the direction opposite
    /// the ascent (i.e. opposite positions[1]).
    pub fn bottom_approach(&self) -> Option<Point2D> {
        // Straight stairs board from their flat landing; a ladder is climbed in
        // place. Neither needs a reserved approach cell.
        if matches!(self.kind, StairKind::Straight | StairKind::Ladder) {
            return None;
        }
        let p0 = *self.positions.first()?;
        let back: Point2D = (-self.direction).into();
        Some(p0 + back)
    }

    pub fn top_approach(&self) -> Option<Point2D> {
        let len = self.positions.len();
        if len < 2 { return None; }
        let prev = self.positions[len - 2];
        let last = self.positions[len - 1];
        let exit_dir = last - prev;
        Some(last + exit_dir)
    }
}
