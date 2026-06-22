//! Sail surface. Phase 2 uses plain white wool; dyed/heraldic sails (and a
//! palette `sail` role) are a Phase 3 concern, so the block is hardcoded here for
//! now with that follow-up understood.

use crate::editor::Editor;
use crate::minecraft::Block;

use super::super::Placement;
use super::RigModel;

const SAIL_BLOCK: &str = "minecraft:white_wool";

/// Fill the planned sail cells with wool.
pub async fn place_sails(editor: &Editor, rig: &RigModel, placement: &Placement) {
    let sail: Block = SAIL_BLOCK.into();
    for &cell in &rig.sail_cells {
        editor.place_block_forced(&sail, placement.to_world(cell)).await;
    }
}
