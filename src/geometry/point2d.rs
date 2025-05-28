use serde_derive::{Deserialize, Serialize};

use super::Point3D;
use std::ops::{Add, Sub, Mul, Div};
use std::ops::{AddAssign, SubAssign, MulAssign, DivAssign};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Point2D {
    pub x: i32,
    pub y: i32,
}

pub const UP_2D: Point2D = Point2D { x: 0, y: -1 };
pub const DOWN_2D: Point2D = Point2D { x: 0, y: 1 };
pub const LEFT_2D: Point2D = Point2D { x: -1, y: 0 };
pub const RIGHT_2D: Point2D = Point2D { x: 1, y: 0 };

pub const Y_PLUS_2D : Point2D = Point2D { x: 0, y: 1 };
pub const Y_MINUS_2D : Point2D = Point2D { x: 0, y: -1 };
pub const X_PLUS_2D : Point2D = Point2D { x: 1, y: 0 };
pub const X_MINUS_2D : Point2D = Point2D { x: -1, y: 0 };

pub const NORTH_2D : Point2D = Point2D { x: 0, y: -1 };
pub const SOUTH_2D : Point2D = Point2D { x: 0, y: 1 };
pub const EAST_2D : Point2D = Point2D { x: 1, y: 0 };
pub const WEST_2D : Point2D = Point2D { x: -1, y: 0 };

pub const CARDINALS_2D: [Point2D; 4] = [
    NORTH_2D,
    SOUTH_2D,
    EAST_2D,
    WEST_2D,
];

pub fn cardinal_to_str(dir: &Point2D) -> Option<String> {
    match *dir {
        NORTH_2D => Some("north".to_string()),
        SOUTH_2D => Some("south".to_string()),
        EAST_2D => Some("east".to_string()),
        WEST_2D => Some("west".to_string()),
        _ => None,
    }
}
impl Default for Point2D {
    fn default() -> Self {
        Point2D { x: 0, y: 0 }
    }
}

impl Point2D {
    pub fn new(x: i32, y: i32) -> Self {
        Point2D { x, y }
    }

    pub fn distance(&self, other: &Point2D) -> f64 {
        let dx = (self.x - other.x).pow(2) as f64;
        let dy = (self.y - other.y).pow(2) as f64;
        (dx + dy).sqrt()
    }

    pub fn distance_squared(&self, other: &Point2D) -> i32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx * dx + dy * dy
    }

    pub fn add_y(&self, y : i32) -> Point3D {
        Point3D { x: self.x, y, z: self.y }
    }

    pub fn add_height(&self, height_map : &Vec<Vec<i32>>) -> Point3D {
        let height = height_map[self.x as usize][self.y as usize];
        Point3D { x: self.x, y: height, z: self.y }
    }
}

impl Add for Point2D {
    type Output = Point2D;

    fn add(self, other: Point2D) -> Point2D {
        Point2D {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl Sub for Point2D {
    type Output = Point2D;

    fn sub(self, other: Point2D) -> Point2D {
        Point2D {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl Mul<i32> for Point2D {
    type Output = Point2D;

    fn mul(self, scalar: i32) -> Point2D {
        Point2D {
            x: self.x * scalar,
            y: self.y * scalar,
        }
    }
}

impl Div<i32> for Point2D {
    type Output = Point2D;

    fn div(self, scalar: i32) -> Point2D {
        Point2D {
            x: self.x / scalar,
            y: self.y / scalar,
        }
    }
}

impl AddAssign for Point2D {
    fn add_assign(&mut self, other: Point2D) {
        self.x += other.x;
        self.y += other.y;
    }
}

impl SubAssign for Point2D {
    fn sub_assign(&mut self, other: Point2D) {
        self.x -= other.x;
        self.y -= other.y;
    }
}

impl MulAssign<i32> for Point2D {
    fn mul_assign(&mut self, scalar: i32) {
        self.x *= scalar;
        self.y *= scalar;
    }
}

impl DivAssign<i32> for Point2D {
    fn div_assign(&mut self, scalar: i32) {
        self.x /= scalar;
        self.y /= scalar;
    }
}