use crate::generator::buildings::{shape::BuildingShape, BuildingID, Grid};

#[derive(Debug, Clone)]
pub struct BuildingData {
    pub id : BuildingID,
    pub grid : Grid,
    pub shape : BuildingShape,
}