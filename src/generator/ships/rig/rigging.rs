//! Standing rigging: iron-bar shrouds/ratlines running from the deck edge up to
//! the crow's nest, narrowing as they rise (the net-like ladders sailors climb).
//! The guides call for iron bars on the steep lines.

use crate::editor::Editor;
use crate::geometry::Point3D;
use crate::minecraft::Block;

use super::super::Placement;
use super::RigModel;

/// Run a shroud line up each side of every mast, from the deck rail to the nest.
pub async fn place_rigging(editor: &Editor, rig: &RigModel, placement: &Placement) {
    let bars: Block = "minecraft:iron_bars".into();

    for mast in &rig.masts {
        let deck_y = mast.base.y;
        let base_half = mast.yards.first().map(|y| (y.half - 1).max(1)).unwrap_or(1);
        let y0 = deck_y + 1;
        let y1 = mast.nest_y;
        if y1 <= y0 {
            continue;
        }
        for y in y0..=y1 {
            let t = (y - y0) as f32 / (y1 - y0) as f32;
            // Narrow from the rail (base_half) to the nest (1).
            let z = (base_half as f32 + (1 - base_half) as f32 * t).round() as i32;
            for &side in &[-z, z] {
                let local = Point3D::new(mast.base.x, y, side);
                editor.place_block(&bars, placement.to_world(local)).await;
            }
        }
    }
}
