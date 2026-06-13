use crate::{generator::materials::MaterialId, geometry::Point3D};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathType {
    Pavement,
    Road,
}

/// Road hierarchy tier, named after the standard urban-planning hierarchy.
/// Each tier maps to a road width in `routing.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathPriority {
    /// Local roads / alleys — narrowest, the minor connections within a block.
    Low,
    /// Collectors / secondary roads — gate spurs that feed onto the arterials.
    Medium,
    /// Arterials / main roads — the MST backbone, widest.
    High,
}

#[derive(Debug, Clone)]
pub struct Path {
    points : Vec<Point3D>,
    width : u32,
    material : MaterialId,
    priority : PathPriority,
    /// Which named road this segment belongs to. Several graph edges that run
    /// straight through junctions are grouped into one road (stroke); `None`
    /// until the network's road-grouping pass assigns it.
    road_id : Option<u32>,
}

impl Path {
    pub fn new(points: Vec<Point3D>, width: u32, material: MaterialId, priority: PathPriority) -> Self {
        Self {
            points,
            width,
            material,
            priority,
            road_id: None,
        }
    }

    pub fn road_id(&self) -> Option<u32> {
        self.road_id
    }

    pub fn set_road_id(&mut self, id: u32) {
        self.road_id = Some(id);
    }

    pub fn points(&self) -> &[Point3D] {
        &self.points
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn material(&self) -> &MaterialId {
        &self.material
    }

    pub fn priority(&self) -> PathPriority {
        self.priority
    }
}