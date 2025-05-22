use std::ops::{Add, AddAssign, DivAssign, MulAssign, Sub, SubAssign};
use serde_derive::{Deserialize, Serialize};
use std::ops::{Mul, Div};

use super::Point2D;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Point3D {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

pub const UP : Point3D = Point3D { x: 0, y: 1, z: 0 };
pub const DOWN : Point3D = Point3D { x: 0, y: -1, z: 0 };
pub const LEFT : Point3D = Point3D { x: -1, y: 0, z: 0 };
pub const RIGHT : Point3D = Point3D { x: 1, y: 0, z: 0 };
pub const FORWARD : Point3D = Point3D { x: 0, y: 0, z: 1 };
pub const BACK : Point3D = Point3D { x: 0, y: 0, z: -1 };

pub const CARDINALS : [Point3D; 4] = [
    LEFT,
    RIGHT,
    FORWARD,
    BACK,
];

impl Default for Point3D {
    fn default() -> Self {
        Point3D { x: 0, y: 0, z: 0 }
    }
}

impl Point3D {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Point3D { x, y, z }
    }

    pub fn distance(&self, other: &Point3D) -> f64 {
        let dx = (self.x - other.x).pow(2) as f64;
        let dy = (self.y - other.y).pow(2) as f64;
        let dz = (self.z - other.z).pow(2) as f64;
        (dx + dy + dz).sqrt()
    }

    pub fn distance_squared(&self, other: &Point3D) -> i32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        dx * dx + dy * dy + dz * dz
    }

    pub fn drop_y(&self) -> Point2D {
        Point2D { x: self.x, y: self.z }
    }

    pub fn without_y(&self) -> Point3D {
        Point3D { x: self.x, y: 0, z: self.z }
    }
}

impl Add for Point3D {
    type Output = Point3D;

    fn add(self, other: Point3D) -> Point3D {
        Point3D {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }
}

impl AddAssign for Point3D {
    fn add_assign(&mut self, other: Point3D) {
        self.x += other.x;
        self.y += other.y;
        self.z += other.z;
    }
}

impl Sub for Point3D {
    type Output = Point3D;

    fn sub(self, other: Point3D) -> Point3D {
        Point3D {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }
}

impl SubAssign for Point3D {
    fn sub_assign(&mut self, other: Point3D) {
        self.x -= other.x;
        self.y -= other.y;
        self.z -= other.z;
    }
}
impl MulAssign<i32> for Point3D {
    fn mul_assign(&mut self, scalar: i32) {
        self.x *= scalar;
        self.y *= scalar;
        self.z *= scalar;
    }
}

impl DivAssign<i32> for Point3D {
    fn div_assign(&mut self, scalar: i32) {
        self.x /= scalar;
        self.y /= scalar;
        self.z /= scalar;
    }
}
impl Mul<i32> for Point3D {
    type Output = Point3D;

    fn mul(self, scalar: i32) -> Point3D {
        Point3D {
            x: self.x * scalar,
            y: self.y * scalar,
            z: self.z * scalar,
        }
    }
}

impl Div<i32> for Point3D {
    type Output = Point3D;

    fn div(self, scalar: i32) -> Point3D {
        Point3D {
            x: self.x / scalar,
            y: self.y / scalar,
            z: self.z / scalar,
        }
    }
}

