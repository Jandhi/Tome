use std::ops::{Add, Neg, Sub};

use crate::{geometry::{Cardinal, Point3D}, minecraft::Block};


// Rotation is always in the clockwise direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    None,
    Once,
    Twice,
    Thrice,
}

impl Rotation {
    pub fn apply_to_point(&self, point: Point3D) -> Point3D {
        match self {
            Rotation::None => point,
            Rotation::Once => Point3D::new(point.z, point.y, -point.x),
            Rotation::Twice => Point3D::new(-point.x, point.y, -point.z),
            Rotation::Thrice => Point3D::new(-point.z, point.y, point.x),
        }
    }

    pub fn apply_to_cardinal(&self, cardinal : Cardinal) -> Cardinal {
        match self {
            Rotation::None => cardinal,
            Rotation::Once => match cardinal {
                Cardinal::North => Cardinal::East,
                Cardinal::East => Cardinal::South,
                Cardinal::South => Cardinal::West,
                Cardinal::West => Cardinal::North,
            },
            Rotation::Twice => match cardinal {
                Cardinal::North => Cardinal::South,
                Cardinal::East => Cardinal::West,
                Cardinal::South => Cardinal::North,
                Cardinal::West => Cardinal::East,
            },
            Rotation::Thrice => match cardinal {
                Cardinal::North => Cardinal::West,
                Cardinal::East => Cardinal::North,
                Cardinal::South => Cardinal::East,
                Cardinal::West => Cardinal::South,
            },
        }
    }

    pub fn apply_to_block(&self, mut block : Block) -> Block {
        if block.state.is_none() {
            return block; // No state to apply rotation to
        }

        if let Some(state) = block.state.as_mut() {
            let keys: Vec<_> = state.keys().cloned().collect();
            for key in keys {
                if let Some(cardinal) = Cardinal::from_string(state.get(&key).expect("Key should exist in state")) {
                    let new_cardinal = self.apply_to_cardinal(cardinal);
                    state.insert(key, new_cardinal.to_string());
                } else if key == "axis" && (self == &Rotation::Once || self == &Rotation::Thrice) {
                    let axis = state.get(&key).expect("Key should exist in state");
                    state.insert(key.clone(), match axis.as_str() {
                        "x" => "z".to_string(),
                        "y" => "y".to_string(),
                        "z" => "x".to_string(),
                        _ => axis.to_string(), // Keep the same if not x, y, or z
                    });
                } else if key == "rotation" {
                    let rotation_value = state.get(&key).expect("Key should exist in state");
                    let rotation: i32 = rotation_value.parse().unwrap_or(0);
                    let new_rotation = (rotation + match self {
                        Rotation::None => 0,
                        Rotation::Once => 4,
                        Rotation::Twice => 8,
                        Rotation::Thrice => 12,                    
                    }).rem_euclid(4);
                    state.insert(key.clone(), new_rotation.to_string());
                }
            }
        }

        block
    }
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

impl From<Cardinal> for Rotation {
    fn from(cardinal: Cardinal) -> Self {
        match cardinal {
            Cardinal::North => Rotation::None,
            Cardinal::East => Rotation::Once,
            Cardinal::South => Rotation::Twice,
            Cardinal::West => Rotation::Thrice,
        }
    }
}

impl Neg for Rotation {
    type Output = Rotation;

    fn neg(self) -> Rotation {
        match self {
            Rotation::None => Rotation::None,
            Rotation::Once => Rotation::Thrice,
            Rotation::Twice => Rotation::Twice,
            Rotation::Thrice => Rotation::Once,
        }
    }
}