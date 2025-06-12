use crate::geometry::{Point2D, Point3D, EAST, NORTH, SOUTH, WEST};
use std::ops::{Add, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    None,
    Once,
    Twice,
    Thrice,
}

impl Add for Rotation {
    type Output = Rotation;

    fn add(self, rhs: Rotation) -> Rotation {
        let lhs_val: i32 = self.into();
        let rhs_val: i32 = rhs.into();
        Rotation::from(lhs_val + rhs_val)
    }
}

impl Sub for Rotation {
    type Output = Rotation;

    fn sub(self, rhs: Rotation) -> Rotation {
        let lhs_val: i32 = self.into();
        let rhs_val: i32 = rhs.into();
        Rotation::from(lhs_val - rhs_val)
    }
}

impl From<i32> for Rotation {
    fn from(value: i32) -> Self {
        match value.rem_euclid(4) {
            0 => Rotation::None,
            1 => Rotation::Once,
            2 => Rotation::Twice,
            3 => Rotation::Thrice,
            _ => unreachable!(), // This case should never happen
        }
    }
}

impl Into<i32> for Rotation {
    fn into(self) -> i32 {
        match self {
            Rotation::None => 0,
            Rotation::Once => 1,
            Rotation::Twice => 2,
            Rotation::Thrice => 3,
        }
    }
}

impl Rotation {
    pub fn from_cardinal(point: Point3D) -> Option<Self> {
        match point {
            _ if point == NORTH => Some(Rotation::None),
            _ if point == EAST => Some(Rotation::Once),
            _ if point == SOUTH => Some(Rotation::Twice),
            _ if point == WEST => Some(Rotation::Thrice),
            _ => None,
        }
    }
}

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

    pub fn rotate(&mut self, amount : i32) {
        let current : i32 = self.rotation.into();
        self.rotation = Rotation::from((current + amount).rem_euclid(4));
    }
}

impl From<Point3D> for Transform {
    fn from(position: Point3D) -> Self {
        Self::new(position, Rotation::None)
    }
}
