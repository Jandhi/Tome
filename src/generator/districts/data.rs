use std::collections::{HashMap, HashSet};

use crate::geometry::{Point2D, Point3D};

use super::parcel::ParcelType;


#[derive(Debug, Clone)]
pub struct ParcelData<TID> {
    pub origin: Point3D,
    pub is_border: bool,
    pub points: HashSet<Point3D>,
    pub points_2d: HashSet<Point2D>,
    pub edges : HashSet<Point3D>,
    pub sum: Point3D,
    pub parcel_type: ParcelType,
    pub parcel_adjacency : HashMap<TID, u32>,
    pub adjacencies_count : u32,
}

impl<TID> ParcelData<TID> {
    pub fn empty() -> Self {
        ParcelData {
            origin: Point3D::default(),
            is_border: false,
            points: HashSet::new(),
            points_2d: HashSet::new(),
            edges: HashSet::new(),
            sum: Point3D::default(),
            parcel_type: ParcelType::Unknown,
            parcel_adjacency: HashMap::new(),
            adjacencies_count: 0,
        }
    }

    pub fn new(origin: Point3D) -> Self {
        let mut data = ParcelData {
            origin,
            is_border: false,
            points: HashSet::new(),
            points_2d: HashSet::new(),
            edges: HashSet::new(),
            sum: origin,
            parcel_type: ParcelType::Unknown,
            parcel_adjacency: HashMap::new(),
            adjacencies_count: 0,
        };

        data.points.insert(origin);
        data.points_2d.insert(origin.drop_y());

        data
    }
}

pub trait HasParcelData<'a, TID : 'a> {
    fn data(&'a self) -> &'a ParcelData<TID>;

    fn origin(&'a self) -> Point3D {
        self.data().origin
    }

    fn is_border(&'a self) -> bool {
        self.data().is_border
    }

    fn points(&'a self) -> &'a HashSet<Point3D> {
        &self.data().points
    }

    fn points_2d(&'a self) -> &'a HashSet<Point2D> {
        &self.data().points_2d
    }

    fn sum(&'a self) -> Point3D {
        self.data().sum
    }

    fn parcel_type(&'a self) -> ParcelType {
        self.data().parcel_type
    }   

    fn average(&'a self) -> Point3D {
        if self.data().points.len() == 0 {
            return Point3D::default();
        }
        self.data().sum / (self.data().points.len() as i32)
    }

    fn size(&'a self) -> usize {
        self.data().points.len()
    }

    fn edges(&'a self) -> &'a HashSet<Point3D> {
        &self.data().edges
    }

    fn parcel_adjacency(&'a self) -> &'a HashMap<TID, u32> {
        &self.data().parcel_adjacency
    }

    fn adjacencies_count(&'a self) -> u32 {
        self.data().adjacencies_count
    }
}

impl<'a, TID : 'a> HasParcelData<'a, TID> for ParcelData<TID> {
    fn data(&self) -> &ParcelData<TID> {
        self
    }
}