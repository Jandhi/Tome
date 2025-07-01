mod wall;
mod test;
mod set;

pub use wall::{WallComponent, WallType, VerticalWallPosition, HorizontalWallPosition, build_walls};
pub use set::{WallSet, WallSetId};