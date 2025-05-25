use std::collections::{HashMap, HashSet};

use crate::{geometry::{Point2D, Point3D}, minecraft::{Biome, BlockID}};

use super::{adjacency::AdjacencyAnalyzeable, District, DistrictID};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SuperDistrictID(pub usize);

#[derive(Debug, Clone)]
pub struct SuperDistrict {
    id : SuperDistrictID,
    districts: HashSet<DistrictID>,
    is_border: bool,
    points: HashSet<Point3D>,
    points_2d: HashSet<Point2D>,
    edges : HashSet<Point3D>,
    sum: Point3D,
    district_adjacency : HashMap<SuperDistrictID, u32>,
    adjacencies_count : u32,

    roughness: f32,
    water_percentage: f32,
    forested_percentage: f32,
    surface_block_count: HashMap<BlockID, u32>,
    biome_count: HashMap<Biome, u32>,
    gradient: f32,
}

impl SuperDistrict {
    pub fn new(id : SuperDistrictID) -> Self {
        SuperDistrict {
            id,
            districts: HashSet::new(),
            is_border: false,
            points: HashSet::new(),
            points_2d: HashSet::new(),
            edges: HashSet::new(),
            sum: Point3D::default(),
            district_adjacency: HashMap::new(),
            adjacencies_count: 0, 
            roughness: 0.0,
            water_percentage: 0.0,
            forested_percentage: 0.0,
            surface_block_count: HashMap::new(),
            biome_count: HashMap::new(),
            gradient: 0.0,
        }
    }

    pub fn add_district(&mut self, district: &District) {
        self.districts.insert(district.id());

        for point in district.points() {
            self.points.insert(*point);
            self.points_2d.insert(point.drop_y());
        }

        self.sum += district.sum();
    }

    pub fn id(&self) -> SuperDistrictID {
        self.id
    }

    pub fn points_2d(&self) -> &HashSet<Point2D> {
        &self.points_2d
    }
}

impl AdjacencyAnalyzeable<SuperDistrictID> for SuperDistrict {
    fn increment_adjacency(&mut self, id: Option<SuperDistrictID>) {
        self.adjacencies_count += 1;
        if let Some(id) = id {
            *self.district_adjacency.entry(id).or_insert(0) += 1;
        }
    }

    fn add_edge(&mut self, point: Point3D) {
        self.edges.insert(point);
    }
}