use serde_derive::{Deserialize, Serialize};
use crate::geometry::{Point2D, Point3D, EAST, EAST_2D, NORTH, NORTH_2D, SOUTH, SOUTH_2D, WEST, WEST_2D};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cardinal {
    #[serde(rename = "north", alias = "z_minus")]
    NORTH,
    #[serde(rename = "east", alias = "x_plus")]
    EAST,
    #[serde(rename = "south", alias = "z_plus")]
    SOUTH,
    #[serde(rename = "west", alias = "x_minus")]
    WEST,
}

impl Default for Cardinal {
    fn default() -> Self {
        Cardinal::NORTH
    }
}

impl Into<Point2D> for Cardinal {
    fn into(self) -> Point2D {
        match self {
            Cardinal::NORTH => NORTH_2D,
            Cardinal::EAST  => EAST_2D,
            Cardinal::SOUTH => SOUTH_2D,
            Cardinal::WEST  => WEST_2D,
        }
    }
}

impl Into<Point3D> for Cardinal {
    fn into(self) -> Point3D {
        match self {
            Cardinal::NORTH => NORTH,
            Cardinal::EAST  => EAST,
            Cardinal::SOUTH => SOUTH,
            Cardinal::WEST  => WEST,
        }
    }
}

impl Cardinal {
    pub fn from_point(point: Point3D) -> Option<Self> {
        match point {
            _ if point == NORTH => Some(Cardinal::NORTH),
            _ if point == EAST  => Some(Cardinal::EAST),
            _ if point == SOUTH => Some(Cardinal::SOUTH),
            _ if point == WEST  => Some(Cardinal::WEST),
            _ => None,
        }
    }

    pub fn from_point_2d(point: Point2D) -> Option<Self> {
        match point {
            _ if point == NORTH_2D => Some(Cardinal::NORTH),
            _ if point == EAST_2D  => Some(Cardinal::EAST),
            _ if point == SOUTH_2D => Some(Cardinal::SOUTH),
            _ if point == WEST_2D  => Some(Cardinal::WEST),
            _ => None,
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "north" | "z_minus" => Some(Cardinal::NORTH),
            "east"  | "x_plus"  => Some(Cardinal::EAST),
            "south" | "z_plus"  => Some(Cardinal::SOUTH),
            "west"  | "x_minus" => Some(Cardinal::WEST),
            _ => None,
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            Cardinal::NORTH => "north".to_string(),
            Cardinal::EAST  => "east".to_string(),
            Cardinal::SOUTH => "south".to_string(),
            Cardinal::WEST  => "west".to_string(),
        }
    }
}