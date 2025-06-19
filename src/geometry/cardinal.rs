use std::ops::Neg;

use serde_derive::{Deserialize, Serialize};
use strum_macros::EnumIter;
use crate::geometry::{Point2D, Point3D, EAST, EAST_2D, NORTH, NORTH_2D, SOUTH, SOUTH_2D, WEST, WEST_2D};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumIter, Hash)]
pub enum Cardinal {
    #[serde(rename = "north", alias = "z_minus")]
    North,
    #[serde(rename = "east", alias = "x_plus")]
    East,
    #[serde(rename = "south", alias = "z_plus")]
    South,
    #[serde(rename = "west", alias = "x_minus")]
    West,
}

impl Default for Cardinal {
    fn default() -> Self {
        Cardinal::North
    }
}

impl Into<Point2D> for Cardinal {
    fn into(self) -> Point2D {
        match self {
            Cardinal::North => NORTH_2D,
            Cardinal::East  => EAST_2D,
            Cardinal::South => SOUTH_2D,
            Cardinal::West  => WEST_2D,
        }
    }
}

impl Into<Point3D> for Cardinal {
    fn into(self) -> Point3D {
        match self {
            Cardinal::North => NORTH,
            Cardinal::East  => EAST,
            Cardinal::South => SOUTH,
            Cardinal::West  => WEST,
        }
    }
}

impl Cardinal {
    pub fn from_point(point: Point3D) -> Option<Self> {
        match point {
            _ if point == NORTH => Some(Cardinal::North),
            _ if point == EAST  => Some(Cardinal::East),
            _ if point == SOUTH => Some(Cardinal::South),
            _ if point == WEST  => Some(Cardinal::West),
            _ => None,
        }
    }

    pub fn from_point_2d(point: Point2D) -> Option<Self> {
        match point {
            _ if point == NORTH_2D => Some(Cardinal::North),
            _ if point == EAST_2D  => Some(Cardinal::East),
            _ if point == SOUTH_2D => Some(Cardinal::South),
            _ if point == WEST_2D  => Some(Cardinal::West),
            _ => None,
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "north" | "z_minus" => Some(Cardinal::North),
            "east"  | "x_plus"  => Some(Cardinal::East),
            "south" | "z_plus"  => Some(Cardinal::South),
            "west"  | "x_minus" => Some(Cardinal::West),
            _ => None,
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            Cardinal::North => "north".to_string(),
            Cardinal::East  => "east".to_string(),
            Cardinal::South => "south".to_string(),
            Cardinal::West  => "west".to_string(),
        }
    }

    pub fn rotate_right(&self) -> Self {
        match self {
            Cardinal::North => Cardinal::East,
            Cardinal::East  => Cardinal::South,
            Cardinal::South => Cardinal::West,
            Cardinal::West  => Cardinal::North,
        }
    }

    pub fn rotate_left(&self) -> Self {
        match self {
            Cardinal::North => Cardinal::West,
            Cardinal::East  => Cardinal::North,
            Cardinal::South => Cardinal::East,
            Cardinal::West  => Cardinal::South,
        }
    }

    pub fn opposite(&self) -> Self {
        return -(*self);
    }
}

impl Neg for Cardinal {
    type Output = Self;

    fn neg(self) -> Self::Output {
        match self {
            Cardinal::North => Cardinal::South,
            Cardinal::East  => Cardinal::West,
            Cardinal::South => Cardinal::North,
            Cardinal::West  => Cardinal::East,
        }
    }
}