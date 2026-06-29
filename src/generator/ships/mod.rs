//! Procedural ship generator (see `docs/plans/ship-builder.md`).
//!
//! Self-contained geometry — the only shared harness is [`Placement`] / [`ShipDir`]
//! (local→world transform + ship-relative facings), the materials system, and the
//! offline/live [`Editor`].
//!
//! Local frame: `x` = stern(0) → bow(+x), `z` = beam (centerline 0), `y` = up from
//! the keel bottom (0).
//!
//! Pipeline: keel → hull shell → rudder → deck → deck additions (railing, bowsprit,
//! masts/sails, helm, companionways) → interior levels + furnishing.

pub mod palette;
pub mod additions;
pub mod keel;
pub mod hull;
pub mod rudder;
pub mod deck;
pub mod levels;
pub mod interior;
pub mod blueprint;
pub mod tuning;
pub mod fleet;
pub mod crew;

#[cfg(test)]
mod test;

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::Palette;
use crate::geometry::{Cardinal, Point2D, Point3D};
use crate::noise::RNG;

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

pub use additions::{DeckAddition, RiggingMaterial, SailBillow, SailState, SizeTier};
pub use hull::HullShape;
use additions::bowsprit::BowspritModel;
use additions::masts::MastModel;
use additions::railing::RailingModel;
use deck::DeckModel;
use hull::HullModel;
use keel::KeelModel;
use palette::ShipPalette;
use rudder::RudderModel;

/// Default length:beam ratio. Defined in [`tuning`]; re-exported here so the existing
/// `ships::DEFAULT_BEAM_RATIO` path keeps working.
pub use tuning::DEFAULT_BEAM_RATIO;

/// Cross-cutting context threaded through every ship stage.
pub struct ShipCtx<'a> {
    pub editor: &'a mut Editor,
    pub data: &'a LoadedData,
    pub palette: &'a Palette,
    pub rng: &'a mut RNG,
}

impl<'a> ShipCtx<'a> {
    pub fn new(editor: &'a mut Editor, data: &'a LoadedData, palette: &'a Palette, rng: &'a mut RNG) -> Self {
        Self { editor, data, palette, rng }
    }
}

/// Per-ship inputs. `length` is passed in (chosen per ship class upstream); the
/// keel derives its depth / rake / post from it. The vertical footing is decided
/// automatically from the terrain at the anchor (land vs water).
#[derive(Debug, Clone, Copy)]
pub struct ShipSpec {
    /// Bow direction (cardinal headings only).
    pub heading: Cardinal,
    /// Tip-to-tip keel length.
    pub length: i32,
    /// Length:beam ratio — max hull beam = `length / beam_ratio`.
    pub beam_ratio: f32,
    /// Plan-view hull shape.
    pub hull_shape: HullShape,
    /// Forward mast rake — blocks of `+x` (toward the bow) per block of mast height.
    /// `0.0` = perfectly vertical masts.
    pub mast_lean: f32,
    /// How the sails are rendered (none / furled / full).
    pub sail_state: SailState,
    /// Wind strength — deepest billow (blocks) of a deployed `Full` sail. `0.0` = flat.
    pub wind: f32,
    /// What thin rigging lines (jib forestay + hangers) are built from. `None` = roll per
    /// ship by chance ([`RiggingMaterial::pick`]); `Some` forces it.
    pub rigging: Option<RiggingMaterial>,
}

impl ShipSpec {
    /// Build a spec with default beam ratio ([`DEFAULT_BEAM_RATIO`]) and a
    /// teardrop hull.
    pub fn new(heading: Cardinal, length: i32) -> Self {
        Self {
            heading,
            length,
            beam_ratio: DEFAULT_BEAM_RATIO,
            hull_shape: HullShape::Teardrop,
            mast_lean: tuning::MAST_LEAN,
            sail_state: SailState::Full,
            wind: tuning::SAIL_WIND,
            rigging: None,
        }
    }

    /// Force the rigging-line material (chain / fence). Leave unset to roll per ship.
    pub fn with_rigging(mut self, rigging: RiggingMaterial) -> Self {
        self.rigging = Some(rigging);
        self
    }

    /// Set how the sails are rendered (none / furled / full).
    pub fn with_sail_state(mut self, sail_state: SailState) -> Self {
        self.sail_state = sail_state;
        self
    }

    /// Set the wind strength — deepest billow (blocks) of a deployed `Full` sail.
    pub fn with_wind(mut self, wind: f32) -> Self {
        self.wind = wind;
        self
    }

    /// Set the length:beam ratio (lower = beamier, higher = sleeker).
    pub fn with_beam_ratio(mut self, beam_ratio: f32) -> Self {
        self.beam_ratio = beam_ratio;
        self
    }

    /// Set the plan-view hull shape.
    pub fn with_hull_shape(mut self, hull_shape: HullShape) -> Self {
        self.hull_shape = hull_shape;
        self
    }

    /// Set the forward mast rake (`0.0` = vertical masts).
    pub fn with_mast_lean(mut self, mast_lean: f32) -> Self {
        self.mast_lean = mast_lean;
        self
    }
}

/// What a ship build produces (hull + decks + rig + interior).
pub struct ShipOutput {
    pub placement: Placement,
    pub keel: KeelModel,
    pub hull: HullModel,
    pub rudder: RudderModel,
    pub deck: DeckModel,
    /// The main railing around the top weather deck (built by the additions pipeline).
    pub railing: Option<RailingModel>,
    /// The bowsprit off the bow (built by the additions pipeline).
    pub bowsprit: Option<BowspritModel>,
    /// The masts (built by the additions pipeline).
    pub masts: Option<MastModel>,
    /// Size tier derived from length — gates which deck additions are built.
    pub tier: SizeTier,
    /// Local Y of the **topmost open weather deck** (what additions/masts build against —
    /// the raised additional deck if any, else the main deck). Sails clear this.
    pub weather_deck_y: i32,
    /// Half-beam per station of the topmost weather deck (`length` entries) — the walkable
    /// deck outline the ship-crew pass seats sailors within.
    pub top_outline: Vec<i32>,
    /// Local deck cell the captain stands on at the helm (`None` if no helm fit). See
    /// [`additions::DeckState::helm_stand`].
    pub helm_stand: Option<Point3D>,
    /// Stage-3 interior levels (hold / gun deck) — the spaces connections + furnishing build into.
    pub levels: levels::ShipLevels,
    /// Local `(x, floor_y, z)` cells of the companionway hatches + stairs/ladders — kept clear by
    /// the later furnish pass.
    pub hatch_cells: Vec<Point3D>,
    /// `true` if the anchor was over water (built below the surface), `false` if on
    /// land (built resting on the ground).
    pub on_water: bool,
}

/// Build a ship at `anchor` (the stern keel point, in world X/Z).
///
/// The footing adapts to the terrain at `anchor` (Stage-1 land/water rule):
/// - **water:** the keel's flat bottom (local `y = 0`) sits `depth` below the
///   surface, clamped so it never digs below the seabed (shallow water → rests on
///   the bottom);
/// - **land:** the flat bottom rests on the ground, so everything is built above.
pub async fn build_ship(
    ctx: &mut ShipCtx<'_>,
    spec: &ShipSpec,
    anchor: Point2D,
) -> ShipOutput {
    let ship_palette = ShipPalette::ship_oak_default();

    // 1. Keel model — pure geometry in the local frame.
    let keel = keel::build_keel_model(spec.length);

    // 2. Resolve the footing: world Y that the keel's flat bottom (local y=0) sits
    //    at. Heightmaps are local to the build origin, usable directly as origin.y.
    let world = ctx.editor.world();
    let on_water = world.is_water(anchor);
    let bottom_y = if on_water {
        let surface = world.get_motion_blocking_height_at(anchor).expect("ship anchor out of bounds");
        let seabed = world.get_ocean_floor_height_at(anchor).expect("ship anchor out of bounds");
        (surface - keel.depth).max(seabed)
    } else {
        world.get_height_at(anchor).expect("ship anchor out of bounds")
    };
    let placement = Placement::new(Point3D::new(anchor.x, bottom_y, anchor.y), spec.heading);

    // 3. Place the keel (waterlog stairs/slabs only when on water).
    keel::place_keel(ctx, &keel, &placement, &ship_palette, on_water).await;

    // 4. Build + place the hull shell upon the keel (blocks only for now). The hull
    //    sits on the keel's crest so the keel stays the outermost, water-touching part.
    let hull = hull::build_hull_model(
        spec.length,
        keel.depth,
        spec.beam_ratio,
        spec.hull_shape,
        &keel.top_profile(),
    );
    hull::place_hull(ctx, &hull, &placement, &ship_palette, on_water).await;

    // 5. Rudder: a raked blade hung aft of the sternpost via fences.
    let rudder = rudder::build_rudder_model(keel.depth);
    rudder::place_rudder(ctx, &rudder, &placement, &ship_palette, on_water).await;

    // 6. Deck: cap the hull's open top with a slab deck.
    let deck = deck::build_deck_model(&hull);
    deck::place_deck(ctx, &deck, &placement, &ship_palette).await;

    // 7. Deck additions — each a modular submodule, run in order. The additional deck
    //    now respects size gating (Small ships, length ≤20, skip it → just a railed
    //    main deck); the other additions still build for all sizes until their gating
    //    is wired up.
    let tier = SizeTier::from_length(spec.length);
    // Rigging-line material: forced by the spec, else rolled per ship by chance.
    let rigging = spec.rigging.unwrap_or_else(|| RiggingMaterial::pick(ctx.rng));
    // Sails are tied to the footing: a ship resting on land is a hulk with bare yards
    // (`None`), while a ship afloat always carries canvas (a water ship never goes bare —
    // a `None` spec is bumped to `Full`).
    let sail_state = if on_water {
        match spec.sail_state {
            SailState::None => SailState::Full,
            set => set,
        }
    } else {
        SailState::None
    };
    let deck_ctx = additions::DeckContext {
        placement: &placement,
        keel: &keel,
        hull: &hull,
        deck: &deck,
        ship_palette: &ship_palette,
        tier,
        on_water,
        mast_lean: spec.mast_lean,
        sail_state,
        wind: spec.wind,
        rigging,
    };
    // Running top-weather-deck state: structural additions raise it, fittings read it.
    let mut deck_state = additions::DeckState::initial(&hull, &deck);
    for &addition in &additions::BUILD_ORDER {
        if additions::DEBUG_SKIP.contains(&addition) {
            continue; // debug toggle (additions.rs::DEBUG_SKIP)
        }
        if addition == DeckAddition::AdditionalDeck && !tier.has(addition) {
            continue; // Small ships have no additional deck.
        }
        additions::build_addition(addition, ctx, &deck_ctx, &mut deck_state).await;
    }
    drop(deck_ctx);

    // The bowsprit's solid prow buries the bow gun ports — re-stamp them so they re-open.
    additions::restamp_gun_ports(ctx, &placement, &ship_palette, &deck_state).await;

    let bowsprit = deck_state.bowsprit;
    let railing = deck_state.railing;
    let masts = deck_state.masts;
    let weather_deck_y = deck_state.top_y;
    let top_outline = deck_state.top_outline;
    let helm_stand = deck_state.helm_stand;

    // Stage 3: enumerate the interior levels (hold / gun deck) from the finished hull + decks.
    let levels = levels::build_ship_levels(&hull, deck.deck_y, deck_state.top_y);
    let hatch_cells = deck_state.hatch_cells;

    // Stage 3: furnish the interior levels (reuses the buildings_v2 furnishing engine).
    let mast_xs: Vec<i32> = masts.as_ref().map(|m| m.base_xs()).unwrap_or_default();
    interior::furnish(ctx, &placement, &ship_palette, &levels, &hatch_cells, &mast_xs).await;

    ShipOutput {
        placement, keel, hull, rudder, deck, railing, bowsprit, masts, tier, weather_deck_y,
        top_outline, helm_stand, levels, hatch_cells, on_water,
    }
}
