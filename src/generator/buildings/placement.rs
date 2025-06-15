use crate::{editor::Editor, generator::{buildings::{shape::BuildingShape, BuildingData, Grid}, materials::PaletteId, BuildClaim}, noise::RNG};

use super::BuildingID;



fn place_building(editor : &mut Editor, shape : BuildingShape, grid : Grid, rng : &RNG, palette : &PaletteId) {
    let data = BuildingData {
        id: BuildingID(editor.world_mut().buildings.len()),
        grid,
        shape,
        palette: palette.clone(),
    };
    
    for point in data.shape.get_footprint(&data.grid) {
        editor.world_mut().claim(point, BuildClaim::Building(data.id));
    }

    

    editor.world_mut().buildings.push(data);
}