//! Stage 1 · **Rudder** — a solid raked fin hung aft of the sternpost, connected
//! along its whole height to the stern by fences. See
//! `docs/plans/ship-builder.md` (Stage 1 → Rudder).
//!
//! Local frame: the sternpost is at `x = 0`; the rudder hangs **aft** (`-x`) on the
//! centreline (`z = 0`), spanning the keel bottom (`y = 0`) up to the waterline
//! (`y = depth`). It is a **solid fin** (filled in the X–Y plane, 1 thick): a
//! vertical leading edge just aft of the post, and a **raked trailing edge** (the
//! bottom reaches further aft) smoothed with **stairs**. A **vertical line of
//! fences** (a 1-block gap) connects the whole sternpost to the fin's leading edge.

use std::collections::HashMap;

use crate::generator::materials::{MaterialPlacer, Placer};
use crate::geometry::Point3D;
use crate::minecraft::BlockForm;

use super::palette::{ShipPalette, ShipPart};
use super::tuning::{FIN_LEAD_X, RUDDER_RAKE, RUDDER_STAIR_FACE, RUDDER_STAIR_TOP};
use super::{Placement, ShipDir, ShipCtx};

/// One fin cell (full block in the body, stair on a trailing-edge step).
#[derive(Debug, Clone)]
pub struct RudderCell {
    pub local: Point3D,
    pub form: BlockForm,
    pub facing: Option<ShipDir>,
    pub top_half: bool,
}

/// Pure-geometry rudder: the solid fin cells, the fence connectors, and the
/// waterline (for waterlogging).
#[derive(Debug, Clone)]
pub struct RudderModel {
    /// Solid fin cells (blocks + trailing-edge smoothing stairs).
    pub blade: Vec<RudderCell>,
    /// Fence connectors — a vertical line linking the sternpost to the fin.
    pub fences: Vec<Point3D>,
    /// Local Y of the waterline (cells below it are waterlogged on water).
    pub waterline_y: i32,
}

/// Build the rudder for a keel of the given `depth`. The fin is filled from its
/// raked trailing edge forward to the vertical leading edge, at every level from
/// the keel bottom to the waterline; a fence at each level bridges the gap to the
/// sternpost.
pub fn build_rudder_model(depth: i32) -> RudderModel {
    let mut blade = Vec::new();

    // Top → bottom so we can detect where the trailing edge steps aft.
    let mut prev_trail: Option<i32> = None;
    for y in (0..=depth).rev() {
        let rake = (((depth - y) as f32) * RUDDER_RAKE).round() as i32;
        let x_trail = FIN_LEAD_X - rake; // furthest aft at this level
        let stepped = prev_trail.map_or(false, |pt| x_trail < pt);

        // Fill the fin body solid; the aftmost cell is a smoothing stair on a step.
        for x in x_trail..=FIN_LEAD_X {
            let cell = if x == x_trail && stepped {
                RudderCell {
                    local: Point3D::new(x, y, 0),
                    form: BlockForm::Stairs,
                    facing: Some(RUDDER_STAIR_FACE),
                    top_half: RUDDER_STAIR_TOP,
                }
            } else {
                RudderCell { local: Point3D::new(x, y, 0), form: BlockForm::Block, facing: None, top_half: false }
            };
            blade.push(cell);
        }
        prev_trail = Some(x_trail);
    }

    // A vertical line of fences (x = -1) connects the whole sternpost to the fin.
    let fences: Vec<Point3D> = (0..=depth).map(|y| Point3D::new(-1, y, 0)).collect();

    RudderModel { blade, fences, waterline_y: depth }
}

/// Place the rudder: solid fin as blocks/stairs, connection as fences. Underwater
/// cells (below the waterline) are waterlogged on water; the top is not.
pub async fn place_rudder(
    ctx: &mut ShipCtx<'_>,
    model: &RudderModel,
    placement: &Placement,
    ship_palette: &ShipPalette,
    on_water: bool,
) {
    let role = ship_palette.role(ShipPart::Rudder);
    let material = ctx
        .palette
        .get_material(role)
        .unwrap_or_else(|| panic!("ship palette role {role:?} missing from base palette"))
        .clone();

    let mut placer_rng = ctx.rng.derive();
    let mut placer = MaterialPlacer::new(Placer::new(&ctx.data.materials, &mut placer_rng), material);

    for cell in &model.blade {
        let submerged = on_water && cell.local.y < model.waterline_y;
        let state = blade_state(cell, placement, submerged);
        placer
            .place_block(ctx.editor, placement.to_world(cell.local), cell.form, state.as_ref(), None)
            .await;
    }

    for &cell in &model.fences {
        let submerged = on_water && cell.y < model.waterline_y;
        let state = submerged.then(|| HashMap::from([("waterlogged".to_string(), "true".to_string())]));
        placer
            .place_block(ctx.editor, placement.to_world(cell), BlockForm::Fence, state.as_ref(), None)
            .await;
    }
}

/// Blockstate for a fin cell: stairs need facing/half (+ waterlogged when
/// submerged); full blocks need none.
fn blade_state(cell: &RudderCell, placement: &Placement, submerged: bool) -> Option<HashMap<String, String>> {
    if cell.form != BlockForm::Stairs {
        return None;
    }
    let facing = placement.world_cardinal(cell.facing.unwrap_or(ShipDir::Bow));
    let mut state = HashMap::from([
        ("facing".to_string(), facing.to_string()),
        ("half".to_string(), if cell.top_half { "top" } else { "bottom" }.to_string()),
    ]);
    if submerged {
        state.insert("waterlogged".to_string(), "true".to_string());
    }
    Some(state)
}
