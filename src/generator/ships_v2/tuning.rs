//! Ships v2 · **central tuning surface** — every hand-tunable constant in one place.
//!
//! These are the knobs we iterate on in the build→screenshot→correct loop. Each module
//! imports the ones it needs from here (`use super::tuning::*` / `use
//! super::super::tuning::*`) rather than defining its own, so all the dials live
//! together and are easy to find, compare, and adjust.
//!
//! ## How to tune
//!
//! 1. Change a value here.
//! 2. `cargo test ships_v2:: -- --skip _live` — fast offline pre-check; the offline test
//!    writes ASCII diagnostics to `output/ships_v2/*.txt` (keel side profile, hull plan +
//!    cross-section, bowsprit spar profiles). Read those first — they catch most shape
//!    mistakes without a server.
//! 3. `cargo test build_ship_v2_live -- --nocapture` — builds against the live GDMC
//!    server for a screenshot. Correct from what you see; repeat.
//!
//! ## "Flip candidates"
//!
//! Minecraft stair/slab orientation has a notorious facing/half ambiguity that's hard to
//! predict from code — the constants tagged **flip candidate** below are the ones to
//! invert first if a screenshot shows a bevel/step on the wrong face. They're isolated
//! here precisely so the fix is a one-line change.
//!
//! ## Convention — keep this file current
//!
//! **When you add a new tunable constant anywhere in `ships_v2`, define it here** (in the
//! right section, with a doc comment covering what it does and which way to push it) and
//! reference it from the module. If a knob lives elsewhere because it can't be a plain
//! `const` (e.g. a value baked into a `match` arm or a `let`), note it under
//! [§ Not-yet-centralized](#not-yet-centralized) below so the inventory stays honest.

use super::ShipDir;

// ===========================================================================
// Sizing
// ===========================================================================

/// Default length : beam ratio. Max beam ≈ `length / ratio`. Lower = stouter/wider hull
/// (≈2.7 is the tutorial's stout look); higher = sleeker/narrower. Overridable per ship
/// via `ShipV2Spec::with_beam_ratio`.
pub const DEFAULT_BEAM_RATIO: f32 = 2.7;

// ===========================================================================
// Keel (`keel.rs`)
// ===========================================================================

/// Bow-stem stair facing. **Flip candidate** — upside-down bow stairs whose full side
/// faces down-slope (toward the stern) so the solid top continues the keel line and the
/// notch rounds the underside. Swap if a screenshot shows the notch on the wrong face.
pub const BOW_RAKE_STAIR_FACE: ShipDir = ShipDir::Stern;

/// Exponent of the parabolic bow-stem curve `y = depth · t^p`. `2.0` = a classic parabola
/// (tangent to the flat run, steepening to the stem). Lower = gentler sweep; higher =
/// sharper, more upright stem.
pub const BOW_CURVE_POW: f32 = 2.0;

// ===========================================================================
// Hull (`hull.rs`)
// ===========================================================================

/// Vertical bilge-flare exponent: half-beam = max · (y/depth)^p. `< 1` → a rounded bilge
/// that widens quickly off the keel then eases toward the waterline; `> 1` → a slacker,
/// more V-shaped bilge.
pub const HULL_BILGE_POW: f32 = 0.7;

/// Teardrop **stern** taper exponent for the plan shape `s^STERN · (1−s)^BOW`. Lower =
/// blunter/wider stern; higher = finer. The widest beam sits at `STERN/(STERN+BOW)`.
pub const STERN_TAPER: f32 = 0.85;

/// Teardrop **bow** taper exponent (see [`STERN_TAPER`]). Lower = fuller/rounder bow;
/// higher = finer/sharper entry.
pub const BOW_TAPER: f32 = 0.65;

/// Whether the bilge-flare bevel stair faces **outboard** (curve on the outside
/// underside). The forced-solid backing block seals the interior either way, so this only
/// tunes the exterior look. **Flip candidate** — invert if the screenshot shows the bevel
/// curving the wrong way.
pub const HULL_BEVEL_FACE_OUTBOARD: bool = false;

// ===========================================================================
// Rudder (`rudder.rs`)
// ===========================================================================

/// Leading edge of the fin in local x (a 1-block fence gap sits aft of the post at
/// `x = -1`). More negative = the fin sits further aft.
pub const FIN_LEAD_X: i32 = -2;

/// Aft rake of the rudder's trailing edge, per block of height below the waterline.
/// Larger = more steeply raked (bottom further aft).
pub const RUDDER_RAKE: f32 = 0.4;

/// Stair facing for the raked trailing edge. **Flip candidate.**
pub const RUDDER_STAIR_FACE: ShipDir = ShipDir::Bow;

/// Stair half for the trailing edge (`true` = top/upside-down). **Flip candidate** —
/// pairs with [`RUDDER_STAIR_FACE`].
pub const RUDDER_STAIR_TOP: bool = false;

// ===========================================================================
// Additional deck (`additions/additional_deck.rs`)
// ===========================================================================

/// Gun ports every this many stations along the side (≈ a 2-block gap at `3`). Larger =
/// fewer, more widely spaced ports.
pub const GUN_PORT_STEP: i32 = 3;

// ===========================================================================
// Railing (`additions/railing.rs`)
// ===========================================================================

/// Height (in blocks) of the solid bulwark course below the fence rail cap. Larger = a
/// taller solid wall before the rail.
pub const BULWARK_HEIGHT: i32 = 1;

// ===========================================================================
// Bowsprit (`additions/bowsprit.rs`)
// ===========================================================================

/// Prow-bevel stair facing (the beam ascends toward the bow). **Flip candidate.**
pub const RAKE_STAIR_FACE: ShipDir = ShipDir::Bow;

/// Forward projection of the spar past the bow, as a fraction of hull length ("a little
/// less than half" → the poking-out part is ~0.4·length). Larger = a longer spar.
pub const REACH_FRACTION: f32 = 0.4;

/// Flare exponent of the prow cross-section below the waterline (mirrors the hull's
/// rounded bilge): half-width = deck_half · (h/span)^p, tapering to 0 at the keel point.
/// `< 1` = rounder bilge; `> 1` = sharper V.
pub const PROW_FLARE_POW: f32 = 0.9;

// --- Spar rake table -------------------------------------------------------
// Each angled rake alternates `FLAT_RUN` flat blocks with a climb group of `CLIMB_RUN`
// one-level double-slab steps. Bigger FLAT_RUN = gentler; bigger CLIMB_RUN = a wider
// equal-height tread per step (the "Tiered" terrace). `REACH` scales the spar length
// (steeper rakes are shortened so they don't climb away into the sky).

/// `Gentle` rake: flat blocks between climbs (≈1 level per 3 columns).
pub const BOWSPRIT_GENTLE_FLAT_RUN: i32 = 2;
/// `Steep` rake: flat blocks between climbs (≈1 level per 2 columns).
pub const BOWSPRIT_STEEP_FLAT_RUN: i32 = 1;
/// `Tiered` rake: flat blocks between each climb group.
pub const BOWSPRIT_TIERED_FLAT_RUN: i32 = 2;
/// `Tiered` rake: equal-height double-slab steps per climb group (the tread width).
pub const BOWSPRIT_TIERED_CLIMB_RUN: i32 = 2;

/// Reach multiplier for `Straight` (horizontal — full reach).
pub const BOWSPRIT_STRAIGHT_REACH: f32 = 1.0;
/// Reach multiplier for `Gentle`.
pub const BOWSPRIT_GENTLE_REACH: f32 = 0.85;
/// Reach multiplier for `Steep` (shortened most).
pub const BOWSPRIT_STEEP_REACH: f32 = 0.6;
/// Reach multiplier for `Tiered`.
pub const BOWSPRIT_TIERED_REACH: f32 = 0.65;

// ===========================================================================
// Not-yet-centralized
// ===========================================================================
//
// Tunables that aren't plain `const`s yet (baked into `match` arms / `let`s). Pull them
// up here if they start needing frequent tuning:
//
// - `additions.rs` — `SizeTier` thresholds (`from_length`), `mast_count`, `extra_decks`;
//   the gating table (`SizeTier::has`); and `BUILD_ORDER` (the addition order — it stays
//   in `additions.rs` because it references `DeckAddition`).
// - `additions/additional_deck.rs` — level height range, tumblehome inset curve.
// - `additions/bowsprit.rs` — prow extent fractions (`prow_back ≈ 0.30·len`,
//   `ext ≈ 0.10·len`), `base_w`/taper exponents, and the spar's `x_stem - 1` start offset.
// - `keel.rs` — `depth ≈ length/5.5`, bow-rake length fraction (~0.18), stern-step counts.
