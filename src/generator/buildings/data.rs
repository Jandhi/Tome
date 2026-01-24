use crate::generator::{buildings::{shape::BuildingShape, BuildingID, Grid}, materials::Palette, style::Style};

#[derive(Debug, Clone)]
pub struct BuildingData {
    pub id : BuildingID,
    pub grid : Grid,
    pub shape : BuildingShape,
    pub palette : Palette,
    pub style : Style,
}