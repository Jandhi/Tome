use serde_derive::{Deserialize, Serialize};

use crate::geometry::{Point3D, Rect3D};

#[derive(Serialize, Deserialize, Debug)]
pub struct BuildAreaResponse {
    #[serde(alias = "xFrom")]
    pub x_from : i32,
    #[serde(alias = "yFrom")]
    pub y_from : i32,
    #[serde(alias = "zFrom")]
    pub z_from : i32,
    #[serde(alias = "xTo")]
    pub x_to : i32,
    #[serde(alias = "yTo")]
    pub y_to : i32,
    #[serde(alias = "zTo")]
    pub z_to : i32,
}

impl BuildAreaResponse {
    pub fn to_rect(&self) -> Rect3D {
        Rect3D::from_points(
            Point3D::new(self.x_from, self.y_from, self.z_from),
            Point3D::new(self.x_to, self.y_to, self.z_to),
        )
    }
}