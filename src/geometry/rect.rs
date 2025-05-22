use serde_derive::{Deserialize, Serialize};

use super::{Point2D, Point3D};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct Rect3D {
    pub origin : Point3D,
    pub size : Point3D,
}

impl Rect3D {
    pub fn from_points(point1 : Point3D, point2: Point3D) -> Self {
        let origin = Point3D {
            x: point1.x.min(point2.x),
            y: point1.y.min(point2.y),
            z: point1.z.min(point2.z),
        };
        let size = Point3D {
            x: (point1.x - point2.x).abs(),
            y: (point1.y - point2.y).abs(),
            z: (point1.z - point2.z).abs(),
        };
        Rect3D { origin, size }
    }

    pub fn contains(&self, point : Point3D) -> bool {
        point.x >= self.origin.x && point.x < self.origin.x + self.size.x &&
        point.y >= self.origin.y && point.y < self.origin.y + self.size.y &&
        point.z >= self.origin.z && point.z < self.origin.z + self.size.z
    }

    pub fn volume(&self) -> i32 {
        self.size.x * self.size.y * self.size.z
    }

    pub fn length(&self) -> i32 {
        self.size.x
    }

    pub fn width(&self) -> i32 {
        self.size.z
    }

    pub fn height(&self) -> i32 {
        self.size.y
    }

    pub fn far_point(&self) -> Point3D {
        Point3D {
            x: self.origin.x + self.size.x,
            y: self.origin.y + self.size.y,
            z: self.origin.z + self.size.z,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]

pub struct Rect2D {
    pub origin : Point2D,
    pub size : Point2D,
}

impl Rect2D {
    pub fn new(origin : Point2D, size: Point2D) -> Self {
        Rect2D { origin, size }
    }

    pub fn from_points(point1 : Point2D, point2: Point2D) -> Self {
        let origin = Point2D {
            x: point1.x.min(point2.x),
            y: point1.y.min(point2.y),
        };
        let size = Point2D {
            x: (point1.x - point2.x).abs(),
            y: (point1.y - point2.y).abs(),
        };
        Rect2D { origin, size }
    }

    pub fn area(&self) -> i32 {
        self.size.x * self.size.y
    }

    pub fn length(&self) -> i32 {
        self.size.x
    }

    pub fn width(&self) -> i32 {
        self.size.y
    }

    pub fn contains(&self, point : Point2D) -> bool {
        point.x >= self.origin.x && point.x < self.origin.x + self.size.x &&
        point.y >= self.origin.y && point.y < self.origin.y + self.size.y
    }

    pub fn far_point(&self) -> Point2D {
        Point2D {
            x: self.origin.x + self.size.x,
            y: self.origin.y + self.size.y,
        }
    }
}