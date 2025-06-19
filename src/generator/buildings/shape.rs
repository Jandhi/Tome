use std::collections::HashSet;

use crate::{generator::buildings::{stairs::StairPlacement, Grid}, geometry::{Point2D, Point3D}};
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildingShape {
    cells : Vec<Point3D>,
    stairs : Option<Vec<StairPlacement>>,
}

impl BuildingShape {
    pub fn new(cells: Vec<Point3D>, stairs : Option<Vec<StairPlacement>>) -> Self {
        Self { cells, stairs }
    }

    pub fn get_footprint(&self, grid : &Grid) -> HashSet<Point2D> {
        self.cells.iter().flat_map(|cell| {
            grid.get_cell_rect2d(*cell).iter()
        }).collect()
    }

    pub fn cells(&self) -> &[Point3D] {
        &self.cells
    }

    pub fn stairs(&self) -> Option<&[StairPlacement]> {
        self.stairs.as_deref()
    }
}