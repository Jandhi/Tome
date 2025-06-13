use crate::{generator::nbts::Rotation, geometry::Point3D};


#[derive(Debug, Clone)]
pub struct Transform {
    pub position : Point3D,
    pub rotation : Rotation,
}

impl Transform {
    pub fn new(position: Point3D, rotation: Rotation) -> Self {
        Self { position, rotation }
    }

    pub fn apply(&self, point: Point3D) -> Point3D {
        match self.rotation {
            Rotation::None => point + self.position,
            Rotation::Once => Point3D::new(point.z, point.y, -point.x) + self.position,
            Rotation::Twice => Point3D::new(-point.x, point.y, -point.z) + self.position,
            Rotation::Thrice => Point3D::new(-point.z, point.y, point.x) + self.position,
        }
    }

    pub fn shift(&mut self, offset: Point3D) {
        self.position += offset;
    }

    pub fn rotate(&mut self, amount : i32) {
        let current : i32 = self.rotation.into();
        self.rotation = Rotation::from((current + amount).rem_euclid(4));
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::new(Point3D::default(), Rotation::None)
    }
}

impl From<Point3D> for Transform {
    fn from(position: Point3D) -> Self {
        Self::new(position, Rotation::None)
    }
}

impl From<Rotation> for Transform {
    fn from(rotation: Rotation) -> Self {
        Self::new(Point3D::default(), rotation)
    }
}