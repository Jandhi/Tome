use std::collections::{HashMap, HashSet};
use crate::{editor::World, geometry::Point3D};

use super::{adjacency::AdjacencyAnalyzeable, data::{ParcelData, HasParcelData}, Parcel, ParcelID, ParcelType};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DistrictID(pub usize);

#[derive(Debug, Clone)]
pub struct District {
    pub id : DistrictID,
    pub parcels : HashSet<ParcelID>,
    pub data : ParcelData<DistrictID>,
}

impl HasParcelData<'_, DistrictID> for District {
    fn data(&self) -> &ParcelData<DistrictID> {
        &self.data
    }
}

impl District {
    pub fn id(&self) -> DistrictID {
        self.id
    }

    pub fn new(id : DistrictID) -> Self {
        District {
            id,
            parcels: HashSet::new(),
            data: ParcelData::empty(),
        }
    }

    pub fn add_parcel(&mut self, parcel: &Parcel, world: &mut World) {
        self.parcels.insert(parcel.id());

        for point in parcel.points() {
            self.data.points.insert(*point);
            self.data.points_2d.insert(point.drop_y());
            world.district_map[point.x as usize][point.z as usize] = Some(self.id);
        }

        self.data.is_border = parcel.data.is_border();
        self.data.sum += parcel.sum();
    }

    pub fn add_district(&mut self, other : &District, parcels : &HashMap<ParcelID, Parcel>, world: &mut World) {
        for parcel in other.parcels() {
            let parcel = parcels.get(&parcel).expect(&format!("Parcel with id {} not found", parcel.0));
            self.add_parcel(parcel, world);
        }

        let my_id = self.id();
        let other_id = other.id();

        for (neighbour, amt) in other.parcel_adjacency().iter().filter(|(id, _)| **id != my_id) {
            // Add the adjacency to the parent
            *self.data.parcel_adjacency.entry(*neighbour).or_insert(0) += amt;
        }

        self.data.adjacencies_count = (self.data.adjacencies_count + other.data.adjacencies_count) - self.data.parcel_adjacency.get(&other_id).unwrap_or(&0) - other.data.parcel_adjacency.get(&my_id).unwrap_or(&0);
        self.data.parcel_adjacency.remove(&other_id);
    }
    
    pub fn parcels(&self) -> &HashSet<ParcelID> {
        &self.parcels
    }

    pub fn get_adjacency_ratio(&mut self, id: DistrictID) -> f32 {
        let count = self.data.parcel_adjacency.get(&id).cloned().unwrap_or(0);
        count as f32 / self.data.adjacencies_count as f32   
    }

    pub fn get_subtypes(&self, parcels : &HashMap<ParcelID, Parcel>) -> HashMap<ParcelType, u32> {
        let mut subtypes: HashMap<ParcelType, u32> = HashMap::new();
        for parcel_id in self.parcels(){
            let parcel = parcels.get(parcel_id).expect(&format!("Parcel with id {} not found", parcel_id.0));
            let parcel_type = parcel.data.parcel_type;
            *subtypes.entry(parcel_type).or_insert(0) += 1;
        }
        subtypes
    }
}

impl AdjacencyAnalyzeable<DistrictID> for District {
    fn increment_adjacency(&mut self, id: Option<DistrictID>) {
        self.data.adjacencies_count += 1;
        if let Some(id) = id {
            *self.data.parcel_adjacency.entry(id).or_insert(0) += 1;
        }
    }

    fn add_edge(&mut self, point: Point3D) {
        self.data.edges.insert(point);
    }
}