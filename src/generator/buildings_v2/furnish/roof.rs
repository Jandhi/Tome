//! Flat-roof terrace decoration.
//!
//! Points the interior furnish machinery at each flat-roof *deck*, treating the
//! parapet as the surrounding wall. The deck is a flat open rectangle, so we
//! build a fresh constraint map over it (inset one cell from the parapet),
//! reserve the roof-ladder exit, and run the `roof_terrace` furniture list
//! through the same placement/connectivity engine the rooms use.
//!
//! Runs after interior furnishing and only for `RoofStyle::Flat`. Dome rects
//! (square, ≥ `MIN_DOME_SIDE`) have no walkable deck and are skipped.

use std::collections::HashSet;

use crate::editor::Editor;
use crate::generator::population::AnchorScene;
use crate::geometry::{Point2D, Rect2D};
use crate::minecraft::Color;

use super::room::{furnish_interior, harvest_anchors};
use super::super::frame::Frame;
use super::super::pipeline::BuildCtx;
use super::super::roof::dome::is_dome_eligible;
use super::super::roof::top_floor_rects;
use super::super::rooms::{CellState, ConstraintMap};

/// The rooms.yaml entries a flat roof can be furnished as. One is chosen at
/// random per building, so neighbouring roofs read as different spaces — a
/// garden, a lounge, an open-air kitchen, etc.
const ROOF_ROOM_KEYS: [&str; 6] = [
    "roof_lounge",
    "roof_garden",
    "roof_kitchen",
    "roof_workshop",
    "roof_storage",
    "roof_sleeping",
];

/// Fabric colours a roof's textiles (awnings, canopies, carpets, bedrolls) can
/// take. One is chosen per building and swapped in for `swap: color` blocks, so
/// roofs aren't all the palette's default red — warm canvas tones with a few
/// dyed accents, all plausible for sun-bleached desert cloth.
const ROOF_FABRIC_COLORS: [Color; 8] = [
    Color::White,
    Color::Orange,
    Color::Yellow,
    Color::Red,
    Color::Brown,
    Color::LightBlue,
    Color::Cyan,
    Color::Magenta,
];

/// Decorate every flat-roof deck in a building with terrace furniture.
///
/// `roof_ladder_wall` is the parapet cell the access ladder climbs against
/// (from `place_roof_ladder`); its inward deck neighbours are kept clear so the
/// player can step off the ladder.
pub async fn decorate_rooftops(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
    roof_ladder_wall: Option<(i32, i32)>,
) -> Vec<AnchorScene> {
    let mut pick_rng = ctx.rng.derive();

    // Every flat roof gets dressed — the variety comes from the theme, the
    // fabric colour, and the deliberately low per-theme `fill_threshold`
    // (rooms.yaml) that keeps each deck sparse rather than packed.
    // Pick one rooftop theme for the whole building so its deck(s) read as a
    // single, coherent space.
    let key = ROOF_ROOM_KEYS[pick_rng.rand_i32_range(0, ROOF_ROOM_KEYS.len() as i32) as usize];
    let room_list = match ctx.data.furniture.rooms.get(key) {
        Some(list) => list,
        None => return Vec::new(),
    };

    let editor: &Editor = &*ctx.editor;
    let items = &ctx.data.furniture.items;
    let materials = &ctx.data.materials;
    let loot = &ctx.data.furniture.loot;

    // Give this building's roof textiles their own fabric colour instead of the
    // palette's default, so a street of roofs isn't all one shade.
    let fabric = ROOF_FABRIC_COLORS[pick_rng.rand_i32_range(0, ROOF_FABRIC_COLORS.len() as i32) as usize];
    let mut roof_palette = ctx.palette.clone();
    roof_palette.primary_color = Some(fabric);
    let palette = &roof_palette;

    // Terrace anchors (someone tending the roof garden, sleeping under the
    // stars, …) harvested across all decks of this building.
    let mut claimed: HashSet<(i32, i32, i32)> = HashSet::new();
    let mut scenes: Vec<AnchorScene> = Vec::new();

    let rects = top_floor_rects(frame);
    for (i, rect) in rects.iter().enumerate() {
        // Domes have no deck; flat rects do.
        if is_dome_eligible(rect) {
            continue;
        }

        // Deck interior = the rect inset one cell, so the parapet ring acts as
        // the wall that wall-anchored items lean against.
        let interior = Rect2D {
            origin: Point2D::new(rect.min().x + 1, rect.min().y + 1),
            size: Point2D::new(rect.size.x - 2, rect.size.y - 2),
        };
        if interior.size.x <= 0 || interior.size.y <= 0 {
            continue;
        }

        // Furniture stands on the deck: the deck block sits at `roof_y - 2`, so
        // its top surface (where furniture goes) is `roof_y - 1`.
        let roof_y = frame.roof_y(i);
        let floor_y = roof_y - 1;
        // No ceiling above an open terrace; canopies place their own overhead
        // blocks. Keep a nominal value so ceiling items (none in the list) would
        // land clear rather than inside the deck.
        let ceiling_y = floor_y + 4;

        let mut constraints = ConstraintMap::new(&interior);

        // Keep the ladder exit walkable: reserve the deck cells next to the
        // parapet cell the ladder climbs against.
        if let Some((wx, wz)) = roof_ladder_wall {
            for (dx, dz) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                let cell = (wx + dx, wz + dz);
                if constraints.get(cell).is_some() {
                    constraints.set(cell, CellState::UnblockedReachable);
                }
            }
        }

        let mut roof_rng = ctx.rng.derive();
        let placed = furnish_interior(
            editor,
            &interior,
            &mut constraints,
            room_list,
            items,
            floor_y,
            ceiling_y,
            None, // open sky — no roof-clearance clamp
            false,
            palette,
            materials,
            loot,
            &mut roof_rng,
        )
        .await;
        harvest_anchors(&placed, &constraints, floor_y, &mut claimed, &mut scenes);
    }

    scenes
}
