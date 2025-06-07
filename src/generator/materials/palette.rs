use crate::{generator::materials::MaterialId, minecraft::Color};



pub struct Palette {
    pub primary_stone : MaterialId,
    pub secondary_stone : MaterialId,
    pub primary_wood : MaterialId,
    pub secondary_wood : MaterialId,
    pub accent : MaterialId,

    pub primary_color : Color,
    pub secondary_color : Color,
}