use crate::{generator::materials::MaterialId, geometry::Point3D};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathPriority {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone)]
pub struct Path {
    points : Vec<Point3D>,
    width : u32,
    material : MaterialId,
    priority : PathPriority,
}

impl Path {
    pub fn new(points: Vec<Point3D>, width: u32, material: MaterialId, priority: PathPriority) -> Self {
        Self {
            points,
            width,
            material,
            priority,
        }
    }

    pub fn points(&self) -> &[Point3D] {
        &self.points
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn material(&self) -> &MaterialId {
        &self.material
    }

    pub fn priority(&self) -> PathPriority {
        self.priority
    }
}