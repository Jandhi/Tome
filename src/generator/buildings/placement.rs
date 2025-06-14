use crate::{editor::Editor, generator::{buildings::{shape::BuildingShape, BuildingData, Grid}, BuildClaim}, noise::RNG};

use super::BuildingID;



fn place_building(editor : &mut Editor, shape : BuildingShape, grid : Grid, rng : &RNG) {
    let data = BuildingData {
        id: BuildingID(editor.world().buildings.len()),
        grid,
        shape,
    };
    
    for point in data.shape.get_footprint(&data.grid) {
        editor.world().claim(point, BuildClaim::Building(data.id));
    }

    

    editor.world().buildings.push(data);
}