//! Domed roofs for square rects — the desert style's signature roof.
//!
//! Over a square rect we build a stepped dark-prismarine hemisphere: each column
//! is filled solid to its rounded height, capped with a bottom slab where the
//! height rounds to a half step. Using only full blocks + slabs keeps the dome
//! watertight by construction and gives a clean half-resolution curve. Triggered
//! per-rect from [`super::flat_roof`] whenever a rect is square (see
//! [`is_dome_eligible`]); non-square rects keep their flat slab deck + parapet.

use std::collections::HashMap;

use crate::editor::Editor;
use crate::geometry::{Point3D, Rect2D};
use crate::minecraft::Block;

/// Minimum square side that gets a dome. Below this the hemisphere is a 1–2
/// block lump not worth building; the rect keeps its flat deck instead.
pub const MIN_DOME_SIDE: i32 = 5;

/// A rect is dome-eligible when it's square and at least [`MIN_DOME_SIDE`] wide.
pub fn is_dome_eligible(rect: &Rect2D) -> bool {
    rect.length() == rect.width() && rect.length() >= MIN_DOME_SIDE
}

fn dark_prismarine() -> Block {
    Block::from_id("minecraft:dark_prismarine".into())
}

fn dark_prismarine_bottom_slab() -> Block {
    Block::new(
        "minecraft:dark_prismarine_slab".into(),
        Some(HashMap::from([("type".to_string(), "bottom".to_string())])),
        None,
    )
}

/// Build a stepped dark-prismarine hemisphere over a (square) rect. `deck_y` is
/// the wall-top level where a flat roof would lay its deck slab; a flat
/// prismarine layer sits there and the hemisphere springs one block above it.
pub(super) async fn place_dome(editor: &Editor, rect: &Rect2D, deck_y: i32) {
    let min = rect.min();
    let max = rect.max();
    let n = rect.length(); // == width(), guaranteed by is_dome_eligible
    let cx = (min.x + max.x) as f32 / 2.0;
    let cz = (min.y + max.y) as f32 / 2.0;
    let r = n as f32 / 2.0;
    let rr = r * r;

    // Distance measured from cell coord to centre, so for an odd side the centre
    // cell sits at dx = dz = 0 (apex dead-centre); for an even side the centre
    // falls on the shared edge of the two middle cells.
    let height_at = |x: i32, z: i32| -> Option<f32> {
        let dx = x as f32 - cx;
        let dz = z as f32 - cz;
        let d2 = dx * dx + dz * dz;
        if d2 > rr { None } else { Some((rr - d2).sqrt()) }
    };

    // Flat sealing layer across the whole square at wall-top: caps the top room
    // (flat roofs skip ceilings).
    for p in rect.iter() {
        editor.place_block(&dark_prismarine(), p.add_y(deck_y)).await;
    }

    // Square base course one block up — the full square (corners included) that
    // the dome sits on, so the layer directly below the curve reads square, not
    // circular. The hemisphere curve rises on top of it from base + 1.
    let base = deck_y + 1;
    for p in rect.iter() {
        editor.place_block(&dark_prismarine(), p.add_y(base)).await;
    }

    // Solid columns curving up from the square base to each cell's rounded
    // hemisphere height, capped by a bottom slab on a half step. The base course
    // already fills level `base` (k = 0), so the curve starts at k = 1; low rim
    // cells (full_count == 0) stay squared off by the base course.
    for x in min.x..=max.x {
        for z in min.y..=max.y {
            let Some(h) = height_at(x, z) else { continue };
            let half_steps = (h * 2.0).round() as i32; // height in half-blocks
            let full_count = half_steps / 2;
            let has_slab = half_steps % 2 == 1;

            for k in 1..full_count {
                editor.place_block(&dark_prismarine(), Point3D::new(x, base + k, z)).await;
            }
            if has_slab && full_count >= 1 {
                editor
                    .place_block(&dark_prismarine_bottom_slab(), Point3D::new(x, base + full_count, z))
                    .await;
            }
        }
    }
}
