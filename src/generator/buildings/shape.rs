use std::collections::HashSet;

use crate::{generator::buildings::Grid, geometry::{Point2D, Point3D}};
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildingShape {
    cells : Vec<Point3D>,
}

impl BuildingShape {
    pub fn get_footprint(&self, grid : &Grid) -> HashSet<Point2D> {
        self.cells.iter().flat_map(|cell| {
            grid.get_cell_rect2d(*cell).iter()
        }).collect()
    }
}