use serde::{Deserialize, Serialize};
use serde_derive::{Deserialize as DeserializeMacro, Serialize as SerializeMacro};

use crate::geometry::Point3D;

#[derive(Clone, Copy, Debug)]
pub enum Coordinate {
    Absolute(i32),
    Relative(i32),
}

impl From<i32> for Coordinate {
    fn from(value: i32) -> Self {
        Coordinate::Absolute(value)
    }
}

#[derive(Clone, Copy, Debug, SerializeMacro, DeserializeMacro)]
pub struct Coordinate3D {
    pub x: Coordinate,
    pub y: Coordinate,
    pub z: Coordinate,
}

impl Coordinate3D {
    pub fn new(x: Coordinate, y: Coordinate, z: Coordinate) -> Self {
        Coordinate3D { x, y, z }
    }

    pub fn from_point(point: Point3D) -> Self {
        Coordinate3D {
            x: Coordinate::Absolute(point.x),
            y: Coordinate::Absolute(point.y),
            z: Coordinate::Absolute(point.z),
        }
    }
}

impl From<Point3D> for Coordinate3D {
    fn from(point: Point3D) -> Self {
        Coordinate3D {
            x: Coordinate::Absolute(point.x),
            y: Coordinate::Absolute(point.y),
            z: Coordinate::Absolute(point.z),
        }
    }
}

impl Into<Point3D> for Coordinate3D {
    fn into(self) -> Point3D {
        Point3D {
            x: match self.x {
                Coordinate::Absolute(value) => value,
                Coordinate::Relative(_) => panic!("Relative coordinates cannot be converted to Point3D"),
            },
            y: match self.y {
                Coordinate::Absolute(value) => value,
                Coordinate::Relative(_) => panic!("Relative coordinates cannot be converted to Point3D"),
            },
            z: match self.z {
                Coordinate::Absolute(value) => value,
                Coordinate::Relative(_) => panic!("Relative coordinates cannot be converted to Point3D"),
            },
        }
    }
}

impl Serialize for Coordinate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match *self {
            Coordinate::Absolute(value) => serializer.serialize_i32(value),
            Coordinate::Relative(value) => serializer.serialize_str(&format!("~{}", value)),
        }
    }
}

impl<'de> Deserialize<'de> for Coordinate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum CoordHelper {
            Str(String),
            Int(i32),
        }

        match CoordHelper::deserialize(deserializer)? {
            CoordHelper::Str(value) => {
                if value.starts_with('~') {
                    let relative_value = value[1..].parse::<i32>().map_err(serde::de::Error::custom)?;
                    Ok(Coordinate::Relative(relative_value))
                } else {
                    let absolute_value = value.parse::<i32>().map_err(serde::de::Error::custom)?;
                    Ok(Coordinate::Absolute(absolute_value))
                }
            }
            CoordHelper::Int(value) => Ok(Coordinate::Absolute(value)),
        }
    }
}