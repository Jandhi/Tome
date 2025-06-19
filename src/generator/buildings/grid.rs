use std::collections::HashMap;

use crate::{editor::Editor, generator::{data::LoadedData, materials::{Material, MaterialId, Palette, PaletteId, Placer}, nbts::{place_nbt, place_structure, NBTMeta, Rotation, Structure, Transform}}, geometry::{Cardinal, Point3D, Rect2D, Rect3D}};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    pub origin : Point3D,
    pub cell_size : Point3D,
}

pub const DEFAULT_GRID_CELL_SIZE: Point3D = Point3D::new(7, 5, 7);

impl Grid {
    pub fn new(origin : Point3D) -> Self {
        Grid {
            origin,
            cell_size: DEFAULT_GRID_CELL_SIZE,
        }
    }

    pub fn grid_to_world(&self, point : Point3D) -> Point3D {
        Point3D {
            x: point.x * (self.cell_size.x - 1) + self.origin.x,
            y: point.y * (self.cell_size.y - 1) + self.origin.y,
            z: point.z * (self.cell_size.z - 1) + self.origin.z,
        }
    }

    pub fn world_to_grid(&self, point : Point3D) -> Point3D {
        Point3D {
            x: (point.x - self.origin.x) / (self.cell_size.x - 1),
            y: (point.y - self.origin.y) / (self.cell_size.y - 1),
            z: (point.z - self.origin.z) / (self.cell_size.z - 1),
        }
    }

    pub fn grid_to_local(&self, point: Point3D) -> Point3D {
        Point3D {
            x: point.x * (self.cell_size.x - 1),
            y: point.y * (self.cell_size.y - 1),
            z: point.z * (self.cell_size.z - 1),
        }
    }

    pub fn local_to_grid(&self, point: Point3D) -> Point3D {
        Point3D {
            x: point.x / (self.cell_size.x - 1),
            y: point.y / (self.cell_size.y - 1),
            z: point.z / (self.cell_size.z - 1),
        }
    }

    pub fn local_to_world(&self, point: Point3D) -> Point3D {
        point + self.origin
    }

    pub fn world_to_local(&self, point: Point3D) -> Point3D {
        point - self.origin
    }

    pub async fn build_structure<'materials>(&self, editor: &mut Editor, placer : &mut Placer<'materials>, structure: &Structure, grid_coordinate: Point3D, direction : Cardinal, data : &LoadedData, palette: &PaletteId) -> anyhow::Result<()> {
        let origin = self.grid_to_world(grid_coordinate);

        let rotation: Rotation = Rotation::from(structure.facing) - Rotation::from(direction);

        
        let mut transform = match rotation {
            Rotation::None => origin.into(),
            Rotation::Once => Transform::new(origin + Point3D { x: 0, y: 0, z: self.cell_size.z - 1 }, Rotation::Once),
            Rotation::Twice => Transform::new(origin + Point3D { x: self.cell_size.x - 1, y: 0, z: self.cell_size.z - 1 }, Rotation::Twice),
            Rotation::Thrice => Transform::new(origin + Point3D { x: self.cell_size.x - 1, y: 0, z: 0 }, Rotation::Thrice),
        };

        // Shift the transform to account for the structure's origin
        transform.shift(rotation.apply_to_point(-structure.origin));

        place_nbt(&structure.meta, transform, editor, placer, data, &structure.palette, &palette, None, None).await
    }

    pub async fn build_nbt<'materials>(&self, editor : &mut Editor, placer : &mut Placer<'materials>, nbt : &NBTMeta, grid_coordinate : Point3D, rotation : Rotation, data : &LoadedData, input_palette: &PaletteId, output_palette: &PaletteId) -> anyhow::Result<()> {
        let origin = self.grid_to_world(grid_coordinate);
        
        let transform = match rotation {
            Rotation::None => origin.into(),
            Rotation::Once => Transform::new(origin + Point3D { x: 0, y: 0, z: self.cell_size.z - 1 }, Rotation::Once),
            Rotation::Twice => Transform::new(origin + Point3D { x: self.cell_size.x - 1, y: 0, z: self.cell_size.z - 1 }, Rotation::Twice),
            Rotation::Thrice => Transform::new(origin + Point3D { x: self.cell_size.x - 1, y: 0, z: 0 }, Rotation::Thrice),
        };

        place_nbt(nbt, transform, editor, placer, data, input_palette, output_palette, None, None).await
    }

    pub fn get_door_world_position(&self, grid_coordinate: Point3D, direction : Cardinal) -> Point3D {
        let offset = self.grid_to_world(grid_coordinate);
        match direction {
            Cardinal::North => offset + Point3D { x: self.cell_size.x / 2, y: 0, z: 0 },
            Cardinal::West => offset + Point3D { x: 0, y: 0, z: self.cell_size.z / 2 },
            Cardinal::South => offset + Point3D { x: self.cell_size.x / 2, y: 0, z: self.cell_size.z - 1 },
            Cardinal::East => offset + Point3D { x: self.cell_size.x - 1, y: 0, z: self.cell_size.z / 2 },
        }
    }

    pub fn get_cell_rect(&self, grid_coordinate : Point3D) -> Rect3D {
        let local = self.grid_to_local(grid_coordinate);
        Rect3D {
            origin: local,
            size: self.cell_size
        }
    }

    pub fn get_cell_rect2d(&self, grid_coordinate : Point3D) -> Rect2D {
        self.get_cell_rect(grid_coordinate).drop_y()
    }
}