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
        let far = Point3D {
            x: point1.x.max(point2.x),
            y: point1.y.max(point2.y),
            z: point1.z.max(point2.z),
        };
        Rect3D {
            origin,
            size: Point3D {
                x: far.x - origin.x + 1,
                y: far.y - origin.y + 1,
                z: far.z - origin.z + 1,
            },
        }
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


    pub fn drop_y(&self) -> Rect2D {
        Rect2D {
            origin: self.origin.drop_y(),
            size: self.size.drop_y(),
        }
    }

    pub fn min(&self) -> Point3D {
        self.origin
    }

    pub fn max(&self) -> Point3D {
        Point3D {
            x: self.origin.x + self.size.x - 1,
            y: self.origin.y + self.size.y - 1,
            z: self.origin.z + self.size.z - 1,
        }
    }
}

// Implement an iterator over all points in the Rect3D (in x, y, z order)
pub struct Rect3DIterator {
    rect: Rect3D,
    current: Option<Point3D>,
}

impl Rect3D {
    pub fn iter(&self) -> Rect3DIterator {
        Rect3DIterator {
            rect: *self,
            current: Some(self.origin),
        }
    }
}

impl Iterator for Rect3DIterator {
    type Item = Point3D;

    fn next(&mut self) -> Option<Self::Item> {
        let size = &self.rect.size;
        let origin = &self.rect.origin;

        let current = match self.current {
            Some(p) => p,
            None => return None,
        };

        // Prepare next point
        let mut next = current;

        // Increment x
        next.x += 1;
        if next.x >= origin.x + size.x {
            next.x = origin.x;
            // Increment y
            next.y += 1;
            if next.y >= origin.y + size.y {
                next.y = origin.y;
                // Increment z
                next.z += 1;
                if next.z >= origin.z + size.z {
                    self.current = None;
                    return Some(current);
                }
            }
        }

        self.current = Some(next);

        Some(current)
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
        let far = Point2D {
            x: point1.x.max(point2.x),
            y: point1.y.max(point2.y),
        };
        Rect2D {
            origin,
            size: Point2D {
                x: far.x - origin.x + 1,
                y: far.y - origin.y + 1,
            },
        }
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

    pub fn min(&self) -> Point2D {
        self.origin
    }

    pub fn max(&self) -> Point2D {
        Point2D {
            x: self.origin.x + self.size.x - 1,
            y: self.origin.y + self.size.y - 1,
        }
    }

    pub fn on_edge(&self, point : Point2D) -> bool {
        ((point.x == self.origin.x || point.x == self.origin.x + self.size.x - 1)
            && (point.y >= self.origin.y && point.y < self.origin.y + self.size.y))
        || ((point.y == self.origin.y || point.y == self.origin.y + self.size.y - 1)
            && (point.x >= self.origin.x && point.x < self.origin.x + self.size.x))
    }

    pub fn contains(&self, point : Point2D) -> bool {
        point.x >= self.origin.x && point.x < self.origin.x + self.size.x &&
        point.y >= self.origin.y && point.y < self.origin.y + self.size.y
    }

    pub fn contains_rect(&self, other: &Rect2D) -> bool {
        self.contains(other.origin) && self.contains(other.max())
    }

    pub fn overlaps(&self, other: &Rect2D) -> bool {
        self.origin.x < other.origin.x + other.size.x
            && self.origin.x + self.size.x > other.origin.x
            && self.origin.y < other.origin.y + other.size.y
            && self.origin.y + self.size.y > other.origin.y
    }

    pub fn shrink(&self, amount: i32) -> Rect2D {
        Rect2D {
            origin: Point2D {
                x: self.origin.x + amount,
                y: self.origin.y + amount,
            },
            size: Point2D {
                x: self.size.x - amount * 2,
                y: self.size.y - amount * 2,
            },
        }
    }

    pub fn midpoint(&self) -> Point2D {
        Point2D {
            x: self.origin.x + self.size.x / 2,
            y: self.origin.y + self.size.y / 2,
        }
    }
}

impl Rect2D {
    pub fn iter(&self) -> Rect2DIterator {
        Rect2DIterator {
            rect: *self,
            current: Some(self.origin),
        }
    }
}

pub struct Rect2DIterator {
    rect: Rect2D,
    current: Option<Point2D>,
}

impl Iterator for Rect2DIterator {
    type Item = Point2D;

    fn next(&mut self) -> Option<Self::Item> {
        let size = &self.rect.size;
        let origin = &self.rect.origin;

        let current = match self.current {
            Some(p) => p,
            None => return None,
        };

        // Prepare next point
        let mut next = current;

        // Increment x
        next.x += 1;
        if next.x >= origin.x + size.x {
            next.x = origin.x;
            // Increment y
            next.y += 1;
            if next.y >= origin.y + size.y {
                self.current = None;
                return Some(current);
            }
        }

        self.current = Some(next);
        Some(current)
    }
}