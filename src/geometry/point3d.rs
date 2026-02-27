use std::ops::{Add, AddAssign, DivAssign, MulAssign, Neg, Sub, SubAssign};
use serde_derive::{Deserialize, Serialize};
use std::ops::{Mul, Div};

use crate::geometry::Cardinal;

use super::Point2D;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Point3D {
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub z: i32,
}

pub const UP : Point3D = Point3D { x: 0, y: 1, z: 0 };
pub const DOWN : Point3D = Point3D { x: 0, y: -1, z: 0 };
pub const LEFT : Point3D = Point3D { x: -1, y: 0, z: 0 };
pub const RIGHT : Point3D = Point3D { x: 1, y: 0, z: 0 };
pub const FORWARD : Point3D = Point3D { x: 0, y: 0, z: 1 };
pub const BACK : Point3D = Point3D { x: 0, y: 0, z: -1 };
pub const NORTH : Point3D = Point3D { x: 0, y: 0, z: -1 };
pub const EAST  : Point3D = Point3D { x: 1, y: 0, z: 0 };
pub const SOUTH : Point3D = Point3D { x: 0, y: 0, z: 1 };
pub const WEST  : Point3D = Point3D { x: -1, y: 0, z: 0 };

// Aliases for axis directions
pub const X_PLUS: Point3D = Point3D { x: 1, y: 0, z: 0 };
pub const X_MINUS: Point3D = Point3D { x: -1, y: 0, z: 0 };
pub const Y_PLUS: Point3D = Point3D { x: 0, y: 1, z: 0 };
pub const Y_MINUS: Point3D = Point3D { x: 0, y: -1, z: 0 };
pub const Z_PLUS: Point3D = Point3D { x: 0, y: 0, z: 1 };
pub const Z_MINUS: Point3D = Point3D { x: 0, y: 0, z: -1 };

pub const CARDINALS : [Point3D; 4] = [
    LEFT,
    RIGHT,
    FORWARD,
    BACK,
];

pub const ORTHOGONALS : [Point3D; 6] = [
    UP,
    DOWN,
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

impl From<[i32; 3]> for Point3D {
    fn from(arr: [i32; 3]) -> Self {
        Point3D { x: arr[0], y: arr[1], z: arr[2] }
    }
}

impl From<Cardinal> for Point3D {
    fn from(cardinal: Cardinal) -> Self {
        match cardinal {
            Cardinal::North => NORTH,
            Cardinal::East  => EAST,
            Cardinal::South => SOUTH,
            Cardinal::West  => WEST,
        }
    }
}

impl Point3D {
    pub const ZERO: Point3D = Point3D { x: 0, y: 0, z: 0 };
    pub const NEIGHBOURS_1_AWAY: [Point3D; 4] = [
        Point3D { x: 1, y: 0, z: 0 },  // EAST
        Point3D { x: -1, y: 0, z: 0 }, // WEST
        Point3D { x: 0, y: 0, z: 1 },  // SOUTH
        Point3D { x: 0, y: 0, z: -1 }, // NORTH
    ];
    pub const NEIGHBOURS_2_AWAY: [Point3D; 8] = [
        Point3D { x: 2, y: 0, z: 0 },  // EAST
        Point3D { x: -2, y: 0, z: 0 }, // WEST
        Point3D { x: 0, y: 0, z: 2 },  // SOUTH
        Point3D { x: 0, y: 0, z: -2 }, // NORTH
        Point3D { x: 1, y: 0, z: 1 },  // SOUTHEAST
        Point3D { x: -1, y: 0, z: -1 },// NORTHWEST
        Point3D { x: -1, y: 0, z: 1 }, // SOUTHWEST
        Point3D { x: 1, y: 0, z: -1 }, // NORTHEAST
    ];

    pub const NEIGHBOURS_3_AWAY : [Point3D; 12] = [
        Point3D { x: 3, y: 0, z: 0 },  // EAST
        Point3D { x: -3, y: 0, z: 0 }, // WEST
        Point3D { x: 0, y: 0, z: 3 },  // SOUTH
        Point3D { x: 0, y: 0, z: -3 }, // NORTH
        Point3D { x: 2, y: 0, z: 1 },  // SOUTHEAST
        Point3D { x: -2, y: 0, z: -1 },// NORTHWEST
        Point3D { x: -2, y: 0, z: 1 }, // SOUTHWEST
        Point3D { x: 2, y: 0, z: -1 }, // NORTHEAST
        Point3D { x: 1, y: 0, z: 2 },  // SOUTHEAST
        Point3D { x: -1, y: 0, z: -2 },// NORTHWEST
        Point3D { x: -1, y: 0, z: 2 }, // SOUTHWEST
        Point3D { x: 1, y: 0, z: -2 }, // NORTHEAST
    ];

    pub const NEIGHBOURS_4_AWAY : [Point3D; 16] = [
        Point3D { x: 4, y: 0, z: 0 },  // EAST
        Point3D { x: -4, y: 0, z: 0 }, // WEST
        Point3D { x: 0, y: 0, z: 4 },  // SOUTH
        Point3D { x: 0, y: 0, z: -4 }, // NORTH
        Point3D { x: 3, y: 0, z: 1 },  // SOUTHEAST
        Point3D { x: -3, y: 0, z: -1 },// NORTHWEST
        Point3D { x: -3, y: 0, z: 1 }, // SOUTHWEST
        Point3D { x: 3, y: 0, z: -1 }, // NORTHEAST
        Point3D { x: 2, y: 0, z: 2 },  // SOUTHEAST
        Point3D { x: -2, y: 0, z: -2 },// NORTHWEST
        Point3D { x: -2, y: 0, z: 2 }, // SOUTHWEST
        Point3D { x: 2, y: 0, z: -2 }, // NORTHEAST
        Point3D { x: 1, y: 0, z: 3 },  // SOUTHEAST
        Point3D { x: -1, y: 0, z: -3 },// NORTHWEST
        Point3D { x: -1, y: 0, z: 3 }, // SOUTHWEST
        Point3D { x: 1, y: 0, z: -3 }, // NORTHEAST
    ];

    pub const NEIGHBOURS_5_AWAY : [Point3D; 24] = [
        Point3D { x: 5, y: 0, z: 0 },   // EAST
        Point3D { x: -5, y: 0, z: 0 },  // WEST
        Point3D { x: 0, y: 0, z: 5 },   // SOUTH
        Point3D { x: 0, y: 0, z: -5 },  // NORTH
        Point3D { x: 4, y: 0, z: 1 },   // SOUTHEAST
        Point3D { x: -4, y: 0, z: -1 }, // NORTHWEST
        Point3D { x: -4, y: 0, z: 1 },  // SOUTHWEST
        Point3D { x: 4, y: 0, z: -1 },  // NORTHEAST
        Point3D { x: 3, y: 0, z: 2 },   // SOUTHEAST
        Point3D { x: -3, y: 0, z: -2 }, // NORTHWEST
        Point3D { x: -3, y: 0, z: 2 },  // SOUTHWEST
        Point3D { x: 3, y: 0, z: -2 },  // NORTHEAST
        Point3D { x: 2, y: 0, z: 3 },   // SOUTHEAST
        Point3D { x: -2, y: 0, z: -3 }, // NORTHWEST
        Point3D { x: -2, y: 0, z: 3 },  // SOUTHWEST
        Point3D { x: 2, y: 0, z: -3 },  // NORTHEAST
        Point3D { x: 1, y: 0, z: 4 },   // SOUTHEAST
        Point3D { x: -1, y: 0, z: -4 }, // NORTHWEST
        Point3D { x: -1, y: 0, z: 4 },  // SOUTHWEST
        Point3D { x: 1, y: 0, z: -4 },  // NORTHEAST
        Point3D { x: 4, y: 0, z: 2 },   // ESE
        Point3D { x: -4, y: 0, z: -2 }, // WNW
        Point3D { x: 2, y: 0, z: 4 },   // SSE
        Point3D { x: -2, y: 0, z: -4 }, // NNW
    ];

    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Point3D { x, y, z }
    }

    pub fn distance(&self, other: Point3D) -> f64 {
        let dx = (self.x - other.x).pow(2) as f64;
        let dy = (self.y - other.y).pow(2) as f64;
        let dz = (self.z - other.z).pow(2) as f64;
        (dx + dy + dz).sqrt()
    }

    pub fn distance_squared(&self, other: Point3D) -> i32 {
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

    pub fn neighbours_3d(&self) -> Vec<Point3D> {
        ORTHOGONALS.iter().map(|&d| *self + d).collect()
    }

    pub fn neighbours_2d(&self) -> Vec<Point3D> {
        CARDINALS.iter().map(|&d| *self + d).collect()
    }

    pub fn rotate_left(&self) -> Point3D {
        Point3D { x: -self.z, y: self.y, z: self.x }
    }

    pub fn rotate_right(&self) -> Point3D {
        Point3D { x: self.z, y: self.y, z: -self.x }
    }
    
    pub fn with_y(&self, height: i32) -> Point3D {
        Point3D { x: self.x, y: height, z: self.z } 
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

impl Neg for Point3D {
    type Output = Point3D;

    fn neg(self) -> Point3D {
        Point3D {
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }
}