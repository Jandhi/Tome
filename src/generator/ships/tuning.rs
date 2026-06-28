//! Ships · **central tuning surface** — every hand-tunable constant in one place.
//!
//! These are the knobs we iterate on in the build→screenshot→correct loop. Each module
//! imports the ones it needs from here (`use super::tuning::*` / `use
//! super::super::tuning::*`) rather than defining its own, so all the dials live
//! together and are easy to find, compare, and adjust.
//!
//! ## How to tune
//!
//! 1. Change a value here.
//! 2. `cargo test ships:: -- --skip _live` — fast offline pre-check; the offline test
//!    writes ASCII diagnostics to `output/ships/*.txt` (keel side profile, hull plan +
//!    cross-section, bowsprit spar profiles). Read those first — they catch most shape
//!    mistakes without a server.
//! 3. `cargo test build_ship_live -- --nocapture` — builds against the live GDMC
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
//! **When you add a new tunable constant anywhere in `ships`, define it here** (in the
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
/// via `ShipSpec::with_beam_ratio`.
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
// Masts (`additions/masts.rs`)
// ===========================================================================

/// Main-mast height as a fraction of hull length — the tallest mast stands ~this · length
/// above the keel ("as tall as the hull length"). Secondary masts scale down from it.
pub const MAST_HEIGHT_FACTOR: f32 = 1.0;

/// Default forward lean of the masts: blocks of `+x` (toward the bow) per block of
/// height. `0.0` = straight (vertical) masts — the current default; leaning is a future
/// feature, available per-ship via `ShipSpec::with_mast_lean`.
pub const MAST_LEAN: f32 = 0.0;

// --- Spars (yards / top fences / aft stays) --------------------------------

/// Lowest (widest) yard's half-width as a fraction of hull length — the biggest sail
/// span. Upper yards narrow gently (see [`MAST_YARD_NARROW_MAX`]). Larger = longer yards.
pub const MAST_YARD_HALF_FRACTION: f32 = 0.18;
/// Minimum sail height (the smallest a sail can be — used for the lowest yard's sail
/// down to the deck and as a floor on any gap).
pub const MAST_SAIL_TOP_HEIGHT: i32 = 4;
/// Spacing (blocks) between the U-shaped droops along a wide furled sail.
pub const MAST_SAIL_FURL_U_STEP: i32 = 4;
/// Mast-height thresholds at which the **top yard** drops an extra block below the
/// masthead: above the first it sits 1 lower, above the second 2 lower (leaving more bare
/// mast / room for a topgallant above it on taller masts).
pub const MAST_TOP_YARD_DROP_H1: i32 = 20;
pub const MAST_TOP_YARD_DROP_H2: i32 = 35;
/// Roughly one yard per this many blocks of usable mast span (foot base → top yard) —
/// drives how many yards a mast gets (then clamped by [`MAST_MAX_YARDS`]). Larger = fewer
/// yards / stays, so each sail is taller.
pub const MAST_YARD_SPAN_PER_SAIL: i32 = 11;
/// Hard cap on yards per mast.
pub const MAST_MAX_YARDS: i32 = 4;
/// Gap growth going down: each lower sail's gap is weighted `1 + i·growth`, so sails get
/// bigger toward the deck while the yards stay spread across the span ("semi-even,
/// respecting sizes").
pub const MAST_SAIL_GROWTH: f32 = 0.4;
/// Most an upper yard narrows relative to the bottom yard (so they shrink with their
/// sails, "but not by too much").
pub const MAST_YARD_NARROW_MAX: i32 = 3;
/// Forward (`+x`, toward the bow) offset of the yards from the mast centreline, in blocks
/// — so the yard (and its sail) sits just ahead of the mast rather than through it.
pub const MAST_YARD_FORWARD: i32 = 1;
/// Fence blocks stacked on top of each mast (the straight finial spar). `MAST_TOP_FENCE_MULTI`
/// applies on **2+ mast ships** (a taller finial reads better with the mast-to-mast stays).
pub const MAST_TOP_FENCE: i32 = 2;
pub const MAST_TOP_FENCE_MULTI: i32 = 3;
/// On 2+ mast ships, a standing-rigging **stay** connects each masthead to the next. Drawn this
/// many blocks tall (centred on the masthead-to-masthead line); `1` = a single clean line.
pub const MAST_STAY_THICK: i32 = 1;

/// Minimum **clear deck blocks between the helm's wheel and the stern railing** — the helm sits
/// halfway between the aftmost mast and the stern, but never closer than this to the stern rail.
pub const HELM_STERN_CLEARANCE: i32 = 2;

// --- Interior levels (Stage 3: hold / gun deck) ----------------------------

/// The **hold** floor is laid at least this many blocks **above the keel bottom** (`y = 0`), so it
/// sits on the flat part of the hull rather than in the narrow keel.
pub const HOLD_KEEL_CLEARANCE: i32 = 2;
/// Maximum **hold height** (floor → main-deck ceiling). Deeper hulls keep the floor this far below
/// the deck (the extra depth below is bilge, not a room) so the hold doesn't become a tall shaft.
pub const HOLD_MAX_HEIGHT: i32 = 4;
/// A level needs at least this much **headroom** (`ceiling_y - floor_y`) to count as a room.
pub const LEVEL_MIN_HEADROOM: i32 = 2;

// --- Masthead flags (wool pennants) ----------------------------------------

/// Shortest / longest masthead pennant, in wool blocks streaming aft. Each mast rolls
/// a length in `[FLAG_MIN_LEN, FLAG_MAX_LEN]`.
pub const FLAG_MIN_LEN: i32 = 4;
pub const FLAG_MAX_LEN: i32 = 7;
/// Vertical column height at the **hoist** (staff edge), tapering to 1 at the fly tip —
/// a small pennant body. `1` = a flat 1-tall streamer (rippled in `y`/`z`); larger = a
/// taller flag.
pub const FLAG_HOIST_HEIGHT: i32 = 1;
/// Peak vertical (`y`) ripple amplitude at the free (fly) end — the flag whips up/down.
/// Amplitude grows from 0 at the pinned hoist to this at the fly. Larger = floppier.
pub const FLAG_WAVE_AMP_Y: f32 = 1.6;
/// Peak sideways (`z`) ripple amplitude at the fly end — so the flag is **not a flat
/// plane** but a 3-D flapping ribbon (wind comes at a slight angle). Larger = more sway.
pub const FLAG_WAVE_AMP_Z: f32 = 1.6;
/// Radians of wave per block along the flag for the vertical (`y`) ripple. ~`1.2` gives
/// roughly one S-curve over a 5-block pennant. Larger = tighter ripples.
pub const FLAG_WAVE_FREQ_Y: f32 = 1.2;
/// Radians of wave per block for the sideways (`z`) ripple — deliberately **different**
/// from [`FLAG_WAVE_FREQ_Y`] so the two waves don't lock into a rigid helix.
pub const FLAG_WAVE_FREQ_Z: f32 = 0.9;
/// Phase offset (radians) of the `z` ripple relative to the `y` ripple (≈ a quarter
/// wave) — staggers the two so the ribbon curves organically.
pub const FLAG_WAVE_Z_PHASE: f32 = 1.6;
/// Heraldic wool colours a ship's pennants are drawn from (one rolled per ship, so a
/// vessel flies its own colours). Hardcoded like the quartz sails until a palette role
/// exists for cloth.
pub const FLAG_COLORS: &[&str] =
    &["red", "white", "blue", "yellow", "black", "light_blue", "green", "orange"];

// --- Square sails (deployed / billowing) -----------------------------------

/// Default **wind strength** = the deepest billow (blocks the belly bulges past the yard)
/// for a deployed square sail. `0.0` = a flat sheet; larger = a fuller, more curved sail.
/// Overridable per ship via `ShipSpec::with_wind`.
pub const SAIL_WIND: f32 = 2.0;
/// Which way a filled sail bellies — `Bow` (driven by a following wind, the usual set) or
/// `Stern`. Local fore/aft displacement, **flip candidate** if the curve faces the wrong way.
pub const SAIL_BILLOW_DIR: ShipDir = ShipDir::Bow;
/// Vertical belly fullness exponent. The bulge down the sail follows `sin(π·t)^p` (`t` =
/// 0 at the head/yard, 1 at the foot) — pinned at **both** the head and the foot, deepest
/// mid-height. `p < 1` fills the belly out over more of the height (a fuller sail); `p > 1`
/// concentrates it in a tighter mid-band.
pub const SAIL_BELLY_POW: f32 = 0.8;
/// Open-air gap (blocks) the lowest sail's foot leaves **above the deck railing** — the
/// user's "2–3 above deck". The railing height (`BULWARK_HEIGHT` + fence) is added on top
/// of this at build time so the foot clears the rail, not just the deck floor. Bumped by 1
/// for a wide (big) sail — see `SAIL_BIG_HALF_WIDTH`.
pub const SAIL_FOOT_CLEARANCE: i32 = 2;
/// Yard half-width at/above which the lowest sail counts as "big" and gets +1 foot clearance.
pub const SAIL_BIG_HALF_WIDTH: i32 = 6;
/// Block a deployed sail's canvas is built from (white cloth). Hardcoded like the furled
/// quartz until a cloth palette role exists.
pub const SAIL_BLOCK: &str = "white_wool";

// Attempt 2 — `SailBillow::Curtain` (flat rows, the whole length curves, sides included).

/// Curtain vertical-profile exponent: depth ∝ `sin(π·t)^p` down the height. `< 1` makes the
/// belly fill out fast and flatten through the middle, with the curve concentrated near the
/// **head/foot** (the "more drastic at top/bottom, tapers into the middle" look). The
/// no-holes relaxation then caps any step at 1 (so the very ends read as a ~45° sweep).
pub const SAIL_CURTAIN_CURVE_POW: f32 = 0.6;
/// Extra billow depth (blocks) per block of yard half-width **beyond** `SAIL_BIG_HALF_WIDTH`,
/// so larger sails curve slightly deeper. `0.0` = every curtain sail the same depth as `wind`.
pub const SAIL_CURTAIN_SIZE_GAIN: f32 = 0.35;

// Attempt 3 — `SailBillow::Combined` (domed centre-weighting + curtain's curving sides).

/// Per-ship chance (percent) of each deployed-sail billow shape, rolled once per ship.
/// `Combined` then `Curtain` then the remainder (`Domed`): 50 / 35 / 15.
pub const SAIL_BILLOW_COMBINED_CHANCE: i32 = 50;
pub const SAIL_BILLOW_CURTAIN_CHANCE: i32 = 35;

/// How much the luff **sides** billow on the combined sail, `0`–`1`: the across-width factor
/// runs from this at the edges to `1` at the centre. `0` = sides pinned flat (pure `Domed`);
/// `1` = sides as deep as the centre (pure `Curtain`). The relaxation leaves the sides free,
/// so they curve to this fraction of the centre depth.
pub const SAIL_COMBINED_EDGE: f32 = 0.5;

/// Wind multiplier for the spanker relative to the square sails (`SAIL_WIND`). Spankers get
/// quite tall/large, so they billow a bit deeper to read as curved. `1.0` = same as the
/// square sails.
pub const SAIL_SPANKER_WIND_FACTOR: f32 = 1.6;
/// How far (blocks) the spanker's **foot lifts up** in the centre — the bottom edge arcs
/// upward off the boom (0 at the two corners), showing the wind pushing the sail up. `0` =
/// a straight foot along the boom.
pub const SAIL_SPANKER_FOOT_LIFT: i32 = 2;

// --- Rigging lines (jib forestay + hangers; later shrouds/stays) -----------

/// Per-ship chance (percent) that thin rigging lines are **chain** rather than **fence**.
/// `0` = always fence, `100` = always chain. (Chains appear not to survive on the current
/// live server, so fence is the safe default for now — see `RiggingMaterial`.)
pub const RIGGING_CHAIN_CHANCE: i32 = 0;

// --- Jib (triangular headsail, bowsprit → foremast) ------------------------

/// Per-ship chance (percent) of a jib, by size tier — **rises with ship size**. A jib needs
/// a bowsprit (Medium+), so `Small` is effectively 0.
pub const JIB_CHANCE_MEDIUM: i32 = 55;
pub const JIB_CHANCE_LARGE: i32 = 80;
pub const JIB_CHANCE_HUGE: i32 = 100;
/// Wind multiplier for the jib billow relative to the square sails (`SAIL_WIND`).
pub const SAIL_JIB_WIND_FACTOR: f32 = 1.3;
/// Blocks the jib's foot (the A→B edge) sits **above** the bowsprit, so the sail's bottom
/// line floats clear of the spar (not embedded in it). The gap between the foot and the spar
/// top is bridged by **hanger ties** (`JIB_FOOT_HANGER_COUNT` of them), tying the canvas down
/// to the bowsprit like the references. `3` leaves ~2 blocks of visible hang.
pub const SAIL_JIB_FOOT_RAISE: i32 = 2;

/// How many **hanger ties** drop from the jib foot to the bowsprit — *not* one per column.
/// `2` ties the two ends (the forward tack + the inboard end), per the references.
pub const JIB_FOOT_HANGER_COUNT: usize = 2;

/// Blocks of **pure rigging** (chain/fence, no canvas) between the foremast head and the jib's
/// head: the sail's top corner stops this far below the masthead and a forestay line bridges the
/// gap, so the jib doesn't read as solid sail jammed into the masthead/square sails.
pub const JIB_HEAD_RIGGING: i32 = 4;

/// How far the jib's **foot** and **leech** edges bow **outward** (a roach), as a fraction of the
/// edge length, so the sail isn't a rigid triangle. The **luff** (forestay edge, sail head → forward
/// bowsprit tip) stays straight. Capped by `JIB_CURVE_MAX` blocks.
pub const JIB_CURVE_FRAC: f32 = 0.13;
pub const JIB_CURVE_MAX: f32 = 3.0;

// --- Spanker (gaff + boom on the aftmost mast) -----------------------------

/// Chance (percent, 0–100) that the aftmost mast carries a spanker (gaff + boom), rolled
/// once per ship. `100` = always, `0` = never.
pub const MAST_SPANKER_CHANCE: i32 = 50;
/// Boom (lower, near-horizontal spar) length aft of the mast, as a fraction of hull
/// length. The spanker is a fore-and-aft sail in the centreline (z=0) plane.
pub const MAST_SPANKER_BOOM_FRACTION: f32 = 0.30;
/// Boom height above the weather deck (kept at least a block up off the deck).
pub const MAST_SPANKER_BOOM_CLEARANCE: i32 = 3;
/// Gaff throat height up the mast (above the boom), as a fraction of the mast's height
/// above the boom — where the rising gaff attaches.
pub const MAST_SPANKER_LUFF_FRACTION: f32 = 0.45;
/// Gaff run aft as a fraction of the boom length. The gaff rises at a fixed **45°**
/// (1 up per block aft), built with double-stairs (stairs on both sides) for a smooth
/// diagonal; this sets how far aft (= how high) it goes.
pub const MAST_SPANKER_GAFF_RUN_FRACTION: f32 = 0.8;
/// Gaff double-stair facing (the gaff rises aft). **Flip candidate.**
pub const MAST_GAFF_STAIR_FACE: ShipDir = ShipDir::Stern;

// --- Crow's nests (mast platforms) -----------------------------------------

/// Minimum mast height to carry any platform/nest — shorter (the smallest) masts get none.
pub const MAST_NEST_MIN_HEIGHT: i32 = 22;
/// Chance (percent) that the tallest mast gets the fenced top crow's nest, rolled once per
/// ship. `100` = always. (Intermediate platforms are unaffected.)
pub const MAST_NEST_CHANCE: i32 = 60;
/// Fenced **top** crow's-nest half-width (`2` → 5×5) — only at the tallest mast's top.
pub const MAST_NEST_HALF: i32 = 2;
/// **Intermediate** (mid-mast) platform half-width (`1` → 3×3), unfenced.
pub const MAST_NEST_PLATFORM_HALF: i32 = 1;
/// Height of the intermediate platform up the mast, as a fraction of the mast's height.
pub const MAST_NEST_HEIGHT_FRACTION: f32 = 0.70;
/// A nest must sit **more than** this many blocks from the nearest yard (so it doesn't
/// crowd a stay). The intermediate platform drops to satisfy it; the top nest pushes the
/// top yard down.
pub const MAST_NEST_YARD_GAP: i32 = 2;

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
// Ship placement (`fleet.rs`) — seating individual ships on Water districts
// ===========================================================================
//
// The pass places **one ship at a time** and repeats until a body is full; these knobs
// govern a single ship's fit (size, depth, clearance) plus the per-body guards.

/// Candidate keel lengths the fit-solver tries for a ship, **largest first** — it seats
/// the biggest ship that fits the open water (and the build-height ceiling). Spans the
/// no-rowboat size range (Small … Huge), matching `build_ship_live`.
pub const SHIP_LENGTHS: &[i32] = &[44, 38, 32, 26, 20, 14];

/// Max keel length on a **non-ocean** water district (river / lake / other) — only open
/// ocean / deep-ocean biomes get the full [`SHIP_LENGTHS`] range; smaller water keeps to
/// modest hulls. The solver picks the largest `SHIP_LENGTHS` entry not exceeding this.
pub const RIVER_MAX_LENGTH: i32 = 24;

/// Blocks of clear water a ship's keel keeps **above the seabed** — so it floats and the
/// keel never touches the bottom. Required water depth at every footprint cell is
/// `keel_depth(length) + KEEL_CLEARANCE`.
pub const KEEL_CLEARANCE: i32 = 1;

/// Extra water cells a ship keeps clear **beyond its hull edge** on every side (beam
/// sides + bow/stern), so it doesn't graze the shore or the next ship.
pub const HULL_MARGIN: i32 = 1;

/// Minimum shore distance (cells) for a candidate ship **centre** — a cheap pre-filter so
/// placement starts from genuinely open water rather than hugging the bank.
pub const MIN_CENTRE_SHORE: i32 = 4;

/// Vertical headroom (blocks) kept between the **sea surface + masts** and the build-area
/// ceiling, so a ship's length-scaled masts/flags never clip the top of the world.
pub const VERTICAL_HEADROOM: i32 = 12;

/// Rejection-sampling attempts to seat one ship (each tries a fresh centre × orientation
/// × length); after this many misses the body is treated as full.
pub const PLACE_ATTEMPTS: usize = 48;

/// Per-ship chance (percent) the sails are **furled** rather than `Full`.
pub const FURLED_CHANCE: i32 = 20;

/// A water district below this many water cells gets no ships (too small to seat one).
pub const MIN_WATER_CELLS: usize = 150;

/// Chance (percent, 0–100) that a qualifying water district gets a ship at all. `100` =
/// every water body is populated; lower leaves some empty for variety.
pub const SHIP_CHANCE_PER_DISTRICT: i32 = 50;

/// Ship wood palette ids (under `data/palettes/ships/`) the fleet pass rolls among — one
/// per ship, for hull-colour variety. Ids missing from the loaded data are skipped, so
/// trimming this list (or a palette file) degrades gracefully. Not yet style/culture-tied.
pub const SHIP_PALETTES: &[&str] = &["ship_oak", "ship_dark", "ship_spruce"];

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
