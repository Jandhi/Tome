//! Top-level ship pipeline. `ShipCtx` bundles the four cross-cutting params
//! (editor / data / palette / rng), exactly like `buildings_v2::BuildCtx`.
//! `build_ship` runs the Phase 1 sequence: dimensions → hull model → plank →
//! deck → invariants.

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::Palette;
use crate::geometry::{Point2D, Point3D};
use crate::noise::RNG;

use super::dimensions::{self, ShipDimensions};
use super::hull::{self, HullModel};
use super::rig::{self, RigModel};
use super::{HullShape, Placement, RigPlan, ShipClass, ShipContext, fittings, superstructure};

/// Shared context threaded through every ship stage.
pub struct ShipCtx<'a> {
    pub editor: &'a mut Editor,
    pub data: &'a LoadedData,
    pub palette: &'a Palette,
    pub rng: &'a mut RNG,
}

impl<'a> ShipCtx<'a> {
    pub fn new(
        editor: &'a mut Editor,
        data: &'a LoadedData,
        palette: &'a Palette,
        rng: &'a mut RNG,
    ) -> Self {
        Self { editor, data, palette, rng }
    }
}

/// Everything `build_ship` produces. The ship is already placed in the editor by
/// the time it returns; callers own the final `editor.flush_buffer()`.
pub struct ShipOutput {
    pub dims: ShipDimensions,
    pub hull_model: HullModel,
    pub placement: Placement,
    pub class: ShipClass,
    pub hull_shape: HullShape,
    pub rig_plan: RigPlan,
    /// The rig that was raised, if any (`None` for oared boats).
    pub rig: Option<RigModel>,
}

/// Build a ship floating at `anchor` (the stern keel point in world X/Z) on the
/// sea surface given by `context.waterline_y`. The keel is dropped so the
/// waterline lands where expected for the hull's freeboard.
pub async fn build_ship(
    ctx: &mut ShipCtx<'_>,
    context: &ShipContext,
    anchor: Point2D,
) -> Result<ShipOutput, String> {
    // 1. Dimensions.
    let dims = dimensions::resolve(context.class, ctx.rng);

    // 2. Hull model (pure geometry, incl. the hold_volume).
    let mut hull_model = hull::build_model(context.hull_shape, dims);

    // Rigged ships get an accessible hold (hatch + ladder); oared boats stay
    // sealed. Record the hatch on the model so the invariants can validate it.
    let rigged = !matches!(context.rig_plan, RigPlan::Oars);
    let hatch = if rigged { fittings::plan_hatch(&hull_model, dims.length) } else { None };
    hull_model.hatch = hatch;

    // Keel world Y so the local waterline coincides with context.waterline_y.
    let keel_y = context.waterline_y - hull_model.waterline_y;
    let placement = Placement::new(Point3D::new(anchor.x, keel_y, anchor.y), context.heading);

    // 3. Plank the hull shell, lay the keel, and stand the internal rib posts.
    hull::plank::plank_hull(ctx.editor, ctx.data, ctx.palette, ctx.rng, &hull_model, &placement).await;
    hull::plank::place_keel(ctx.editor, ctx.data, ctx.palette, ctx.rng, &hull_model, &placement).await;
    hull::plank::place_frames(ctx.editor, ctx.data, ctx.palette, ctx.rng, &hull_model, &placement).await;

    // 4. Deck + bulwark.
    hull::deck::place_deck(ctx.editor, ctx.data, ctx.palette, ctx.rng, &hull_model, &placement).await;

    // 5. Raised aft quarterdeck (larger classes).
    let castle = superstructure::maybe_quarterdeck(
        ctx.editor, ctx.data, ctx.palette, ctx.rng, &hull_model, &placement, context.class,
    ).await;

    // 6. Rig (mast, sail, rigging) for rigged ships.
    let rig = if rigged {
        let rig_model = rig::build_plan(context.rig_plan, &hull_model, &dims);
        rig::raise(ctx.editor, ctx.data, ctx.palette, ctx.rng, &rig_model, &placement).await;
        rig::check_rig_invariants(&hull_model, &rig_model)?;
        Some(rig_model)
    } else {
        None
    };

    // 7. Fittings: rudder, hatch/ladder, helm + lantern + flag (on the
    // quarterdeck when present).
    if rigged {
        fittings::place_fittings(ctx.editor, ctx.data, ctx.palette, ctx.rng, &hull_model, hatch, castle, &placement).await;
    }

    // 8. Hull invariants (validates the hatch-over-hold, symmetry, watertightness).
    hull::check_ship_invariants(&hull_model)?;

    Ok(ShipOutput {
        dims,
        hull_model,
        placement,
        class: context.class,
        hull_shape: context.hull_shape,
        rig_plan: context.rig_plan,
        rig,
    })
}
