//! Core furniture-placement enums shared across the furnish submodules and the
//! `data` layer that deserializes furniture definitions.

use serde_derive::Deserialize;

/// What constraint a furniture block imposes on its floor cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellConstraint {
    Wall,
    BlockedReachable,
    /// Cell must be Empty before placement, kept walkable + unplaceable after.
    /// Used to reserve approach / clearance space without blocking foot traffic
    /// (e.g. the cell behind the reading_nook chair so the player can sit).
    EmptyReachable,
    None,
}

/// How a block's "facing" state relates to the wall direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FacingMode {
    #[default]
    None,
    AwayFromWall,
    TowardWall,
    Perpendicular,
}

/// Which vertical layer a block occupies at its (x,z) coordinate.
/// `Both` is for blocks that should reserve both layer slots — e.g. a wall
/// banner that wants to keep a hanging lantern from being placed directly
/// above it, even when the banner itself only places a single block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BlockLayer {
    #[default]
    Ground,
    Ceiling,
    Both,
}

impl BlockLayer {
    pub fn occupies_ground(self) -> bool { matches!(self, BlockLayer::Ground | BlockLayer::Both) }
    pub fn occupies_ceiling(self) -> bool { matches!(self, BlockLayer::Ceiling | BlockLayer::Both) }
}
