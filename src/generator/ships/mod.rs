//! Procedural ship generator (Phase 1).
//!
//! Mirrors the `buildings_v2` philosophy: pure-geometry models produced first,
//! block placement after. A ship is the composition of three orthogonal choices —
//! a [`ShipClass`] (size envelope), a [`HullShape`], and a [`RigPlan`] — so the
//! same size can carry different hulls and rigs.
//!
//! **Phase 1 scope:** size classes + dimensions, the hull model (incl. the empty
//! `hold_volume`), and a single [`HullShape::RowboatHull`] planked + decked on a
//! water flatworld, with an ASCII diagnostic and an offline test. No rig, empty
//! (sealed) hold. Rig, fittings, and below-deck interiors are designed in
//! `docs/plans/ship-builder.md` and land in later phases.

pub mod dimensions;
pub mod hull;
pub mod rig;
pub mod fittings;
pub mod superstructure;
pub mod pipeline;
pub mod blueprint;

#[cfg(test)]
mod test;

pub use dimensions::ShipDimensions;
pub use hull::HullModel;
pub use rig::RigModel;
pub use pipeline::{ShipCtx, ShipOutput, build_ship};

use crate::geometry::{Cardinal, Point3D};
use crate::noise::RNG;

/// Size envelope for a ship. Drives length / beam / depth and mast count via
/// [`dimensions::resolve`]. The ship analogue of `buildings_v2::SizeClass`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShipClass {
    Rowboat,
    Sloop,
    Cog,
    Caravel,
    Galleon,
}

/// Cross-section family for the hull. Selected independently of size; the model
/// builder ([`hull::build_model`]) dispatches on it. Phase 1 implements
/// `RowboatHull`; the rest fall back to it until Phase 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HullShape {
    /// Tiny, near-flat-bottomed, double-ended.
    RowboatHull,
    /// Round bilge, full beam amidships (Phase 3).
    RoundCog,
    /// Fine V-bottom entry with pronounced sheer (Phase 3).
    SleekCaravel,
    /// Shallow, symmetric, low freeboard (Phase 3).
    Longship,
}

/// Sail/oar plan. Not built in Phase 1 (every ship is currently bare-hulled),
/// but carried through so the type surface is stable for Phase 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigPlan {
    Oars,
    SingleMast,
    TwoMast,
    ThreeMast,
}

impl ShipClass {
    /// Hull shapes that suit this class. Mirrors `Culture::roof_styles()` — keeps
    /// random selection from pairing, say, a galleon with a rowboat hull. Every
    /// [`HullShape`] is reachable through some class.
    pub fn hull_shapes(&self) -> Vec<HullShape> {
        match self {
            ShipClass::Rowboat => vec![HullShape::RowboatHull],
            ShipClass::Sloop => vec![HullShape::SleekCaravel, HullShape::Longship],
            ShipClass::Cog => vec![HullShape::RoundCog],
            ShipClass::Caravel => vec![HullShape::SleekCaravel, HullShape::RoundCog],
            ShipClass::Galleon => vec![HullShape::RoundCog, HullShape::SleekCaravel],
        }
    }

    /// Rig plans that suit this class.
    pub fn rig_plans(&self) -> Vec<RigPlan> {
        match self {
            ShipClass::Rowboat => vec![RigPlan::Oars],
            ShipClass::Sloop => vec![RigPlan::SingleMast],
            ShipClass::Cog => vec![RigPlan::SingleMast],
            ShipClass::Caravel => vec![RigPlan::TwoMast],
            ShipClass::Galleon => vec![RigPlan::ThreeMast],
        }
    }

    /// Pick a valid `(hull, rig)` combination for this class.
    pub fn pick_combo(&self, rng: &mut RNG) -> (HullShape, RigPlan) {
        let hulls = self.hull_shapes();
        let rigs = self.rig_plans();
        let hull = hulls[rng.rand_i32_range(0, hulls.len() as i32) as usize];
        let rig = rigs[rng.rand_i32_range(0, rigs.len() as i32) as usize];
        (hull, rig)
    }
}

/// All ship classes, smallest to largest. Handy for variety tests and selection.
pub const SHIP_CLASSES: [ShipClass; 5] = [
    ShipClass::Rowboat,
    ShipClass::Sloop,
    ShipClass::Cog,
    ShipClass::Caravel,
    ShipClass::Galleon,
];

/// Default palette id for ships (see `data/palettes/ships/`).
pub fn default_ship_palette() -> crate::generator::materials::PaletteId {
    "ship_oak".into()
}

/// Per-ship choices threaded through the pipeline. The ship analogue of
/// `BuildingContext`.
#[derive(Debug, Clone, Copy)]
pub struct ShipContext {
    pub class: ShipClass,
    pub hull_shape: HullShape,
    pub rig_plan: RigPlan,
    /// Bow direction. Phase 1 supports the four cardinal headings only.
    pub heading: Cardinal,
    /// World Y of the sea surface the ship floats on. The keel is derived from
    /// this and the dimensions' freeboard, so the waterline sits where expected.
    pub waterline_y: i32,
}

impl ShipContext {
    pub fn new(class: ShipClass, hull_shape: HullShape, rig_plan: RigPlan, heading: Cardinal, waterline_y: i32) -> Self {
        Self { class, hull_shape, rig_plan, heading, waterline_y }
    }
}

/// A direction in the ship's own frame, independent of world heading. Used to
/// orient stairs/slabs on the hull so they rotate correctly with the ship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShipDir {
    /// +x local (toward the bow).
    Bow,
    /// -x local (toward the stern).
    Stern,
    /// +z local (starboard).
    Starboard,
    /// -z local (port).
    Port,
}

impl ShipDir {
    pub fn opposite(self) -> Self {
        match self {
            ShipDir::Bow => ShipDir::Stern,
            ShipDir::Stern => ShipDir::Bow,
            ShipDir::Starboard => ShipDir::Port,
            ShipDir::Port => ShipDir::Starboard,
        }
    }
}

/// Transform from the ship's local build frame to world space.
///
/// Local frame: `x` runs the length (stern at 0, bow toward `+x`), `z` is the
/// signed offset across the beam (centerline at 0, symmetric), `y` is up from the
/// keel (0). Rotation is a single cardinal mapping — the bow follows `heading`,
/// the starboard side follows `heading.rotate_right()`. Keeping all hull/rig math
/// in this frame means symmetry is "negate z" and rotation is this one transform.
#[derive(Debug, Clone, Copy)]
pub struct Placement {
    /// World position of the local origin (stern keel point, on the centerline).
    pub origin: Point3D,
    pub heading: Cardinal,
}

impl Placement {
    pub fn new(origin: Point3D, heading: Cardinal) -> Self {
        Self { origin, heading }
    }

    /// World cardinal a ship-local direction points to under this heading.
    pub fn world_cardinal(&self, dir: ShipDir) -> Cardinal {
        match dir {
            ShipDir::Bow => self.heading,
            ShipDir::Stern => self.heading.opposite(),
            ShipDir::Starboard => self.heading.rotate_right(),
            ShipDir::Port => self.heading.rotate_left(),
        }
    }

    /// Map a local cell `(x = length, y = up, z = beam offset)` to world space.
    pub fn to_world(&self, local: Point3D) -> Point3D {
        let fwd: Point3D = self.heading.into();
        let right: Point3D = self.heading.rotate_right().into();
        // Heading vectors are horizontal, so y comes straight from local.y.
        Point3D::new(
            self.origin.x + fwd.x * local.x + right.x * local.z,
            self.origin.y + local.y,
            self.origin.z + fwd.z * local.x + right.z * local.z,
        )
    }
}
