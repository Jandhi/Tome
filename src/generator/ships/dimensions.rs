//! Size-class → dimension envelopes. The ship analogue of
//! `SizeClass::target_area_*`: a `match` returning ranges, sampled per ship.

use crate::noise::RNG;

use super::ShipClass;

/// Resolved hull dimensions in the local build frame (blocks).
#[derive(Debug, Clone, Copy)]
pub struct ShipDimensions {
    /// Length along local X (stern→bow).
    pub length: i32,
    /// Maximum beam (width across) amidships. Forced odd so there is a single
    /// centerline column and the hull is symmetric about z = 0.
    pub beam: i32,
    /// Keel-to-deck height. The deck surface sits at local y = `depth`.
    pub depth: i32,
    /// Deck height above the waterline. Keel floats at `waterline - (depth - freeboard)`.
    pub freeboard: i32,
    /// Number of masts the rig stage may raise (unused in Phase 1).
    pub masts: u32,
}

impl ShipClass {
    /// `(min, max)` inclusive length range. Sized off the reference NBT ships
    /// (a cog/caravel sits around 24–30 long × 9 beam × 8 deep).
    pub fn length_range(&self) -> (i32, i32) {
        match self {
            ShipClass::Rowboat => (7, 10),
            ShipClass::Sloop => (14, 18),
            ShipClass::Cog => (20, 26),
            ShipClass::Caravel => (26, 32),
            ShipClass::Galleon => (34, 44),
        }
    }

    /// `(min, max)` inclusive beam range (before the odd-width adjustment).
    pub fn beam_range(&self) -> (i32, i32) {
        match self {
            ShipClass::Rowboat => (3, 3),
            ShipClass::Sloop => (4, 5),
            ShipClass::Cog => (6, 8),
            ShipClass::Caravel => (8, 9),
            ShipClass::Galleon => (10, 13),
        }
    }

    /// Keel-to-deck depth.
    pub fn depth(&self) -> i32 {
        match self {
            ShipClass::Rowboat => 2,
            ShipClass::Sloop => 4,
            ShipClass::Cog => 5,
            ShipClass::Caravel => 7,
            ShipClass::Galleon => 8,
        }
    }

    /// Deck height above the waterline.
    pub fn freeboard(&self) -> i32 {
        match self {
            ShipClass::Rowboat => 1,
            ShipClass::Sloop => 2,
            ShipClass::Cog => 2,
            ShipClass::Caravel => 3,
            ShipClass::Galleon => 3,
        }
    }

    /// Mast count for the rig stage (Phase 1 ignores this).
    pub fn masts(&self) -> u32 {
        match self {
            ShipClass::Rowboat => 0,
            ShipClass::Sloop => 1,
            ShipClass::Cog => 1,
            ShipClass::Caravel => 2,
            ShipClass::Galleon => 3,
        }
    }
}

/// Sample concrete dimensions for `class`. Deterministic for a given RNG state.
pub fn resolve(class: ShipClass, rng: &mut RNG) -> ShipDimensions {
    let (lmin, lmax) = class.length_range();
    let (bmin, bmax) = class.beam_range();

    // `rand_i32_range` is exclusive of `max`, so add 1 for an inclusive sample.
    let length = rng.rand_i32_range(lmin, lmax + 1);
    let mut beam = rng.rand_i32_range(bmin, bmax + 1);
    // Force odd so there's a centerline column (keeps the hull symmetric).
    beam |= 1;

    ShipDimensions {
        length,
        beam,
        depth: class.depth(),
        freeboard: class.freeboard(),
        masts: class.masts(),
    }
}
