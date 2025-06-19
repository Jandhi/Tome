use crate::generator::{buildings::{shape::BuildingShape, BuildingID, Grid}, materials::{Palette, PaletteId}, style::Style};

#[derive(Debug, Clone)]
pub struct BuildingData {
    pub id : BuildingID,
    pub grid : Grid,
    pub shape : BuildingShape,
    pub palette : PaletteId,
    pub style : Style,
}