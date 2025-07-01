use std::collections::HashSet;

use crate::{generator::{buildings::{stairs::StairPlacement, Grid}, nbts::Rotation}, geometry::{Cardinal, Point2D, Point3D}};
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildingShape {
    cells : Vec<Point3D>,
    stairs : Option<Vec<StairPlacement>>,
    doors : Option<Vec<WallPlacement>>, // The primary door is assumed to be facing south at (0,0,0)
    windows : Option<Vec<WallPlacement>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WallPlacement {
    pub cell: Point3D,
    pub direction: Cardinal,
}

impl BuildingShape {
    pub fn new(cells: Vec<Point3D>, stairs : Option<Vec<StairPlacement>>, doors : Option<Vec<WallPlacement>>) -> Self {
        Self { cells, stairs, doors, windows: None }
    }

    pub fn has_cell(&self, cell: Point3D) -> bool {
        self.cells.contains(&cell)
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
    
    pub fn windows_mut(&mut self) -> &mut Option<Vec<WallPlacement>> {
        &mut self.windows
    }

    pub fn windows(&self) -> Option<&[WallPlacement]> {
        self.windows.as_deref()
    }

    pub fn has_door(&self, cell: Point3D, direction: Cardinal) -> bool {
        if let Some(doors) = &self.doors {
            doors.iter().any(|door| door.cell == cell && door.direction == direction)
        } else {
            false
        }
    }

    pub fn rotate(&mut self, rotation : Rotation) {
        for cell in &mut self.cells {
            *cell = rotation.apply_to_point(*cell);
        }
        if let Some(stairs) = &mut self.stairs {
            for stair in stairs {
                stair.cell = rotation.apply_to_point(stair.cell);
                stair.direction = rotation.apply_to_cardinal(stair.direction);
            }
        }
        if let Some(doors) = &mut self.doors {
            for door in doors {
                door.cell = rotation.apply_to_point(door.cell);
                door.direction = rotation.apply_to_cardinal(door.direction);
            }
        }
        if let Some(windows) = &mut self.windows {
            for window in windows {
                window.cell = rotation.apply_to_point(window.cell);
                window.direction = rotation.apply_to_cardinal(window.direction);
            }
        }
    }
}