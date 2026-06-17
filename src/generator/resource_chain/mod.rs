mod types;
mod registry;
mod production_painter;
mod production_area;
#[cfg(test)]
mod tests;

pub use types::{ResourceDef, RecipeDef, ParcelResourceAssignment};
pub use registry::{ResourceRegistry, ResolvedChains, NearMiss, ChainSelection, ProductionPlan, SettlementProductionResult};
pub use production_painter::{ProductionPainter, ProductionPaintersFile};
pub use production_area::{paint_production_area, paint_production_area_for, EDGE_BUFFER};
