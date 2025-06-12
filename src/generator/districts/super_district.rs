use std::collections::{HashMap, HashSet};
use crate::{editor::World, geometry::Point3D};

use super::{adjacency::AdjacencyAnalyzeable, data::{DistrictData, HasDistrictData}, District, DistrictID, DistrictType};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SuperDistrictID(pub usize);

#[derive(Debug, Clone)]
pub struct SuperDistrict {
    pub id : SuperDistrictID,
    pub districts : HashSet<DistrictID>,
    pub data : DistrictData<SuperDistrictID>,
}

impl HasDistrictData<'_, SuperDistrictID> for SuperDistrict {
    fn data(&self) -> &DistrictData<SuperDistrictID> {
        &self.data
    }
}

impl SuperDistrict {
    pub fn id(&self) -> SuperDistrictID {
        self.id
    }

    pub fn new(id : SuperDistrictID) -> Self {
        SuperDistrict {
            id,
            districts: HashSet::new(),
            data: DistrictData::empty(),
        }
    }

    pub fn add_district(&mut self, district: &District, world: &mut World) {
        self.districts.insert(district.id());

        for point in district.points() {
            self.data.points.insert(*point);
            self.data.points_2d.insert(point.drop_y());
            world.super_district_map[point.x as usize][point.z as usize] = Some(self.id);
        }

        self.data.is_border = district.data.is_border();
        self.data.sum += district.sum();
    }

    pub fn add_superdistrict(&mut self, other : &SuperDistrict, districts : &HashMap<DistrictID, District>, world: &mut World) {
        for district in other.districts() {
            let district = districts.get(&district).expect(&format!("District with id {} not found", district.0));
            self.add_district(district, world);
        }

        let my_id = self.id();
        let other_id = other.id();

        for (neighbour, amt) in other.district_adjacency().iter().filter(|(id, _)| **id != my_id) {
            // Add the adjacency to the parent
            *self.data.district_adjacency.entry(*neighbour).or_insert(0) += amt;
        }

        self.data.adjacencies_count = (self.data.adjacencies_count + other.data.adjacencies_count) - self.data.district_adjacency.get(&other_id).unwrap_or(&0) - other.data.district_adjacency.get(&my_id).unwrap_or(&0);
        self.data.district_adjacency.remove(&other_id);
    }
    
    pub fn districts(&self) -> &HashSet<DistrictID> {
        &self.districts
    }

    pub fn get_adjacency_ratio(&mut self, id: SuperDistrictID) -> f32 {
        let count = self.data.district_adjacency.get(&id).cloned().unwrap_or(0);
        count as f32 / self.data.adjacencies_count as f32   
    }

    pub fn get_subtypes(&self, districts : &HashMap<DistrictID, District>) -> HashMap<DistrictType, u32> {
        let mut subtypes: HashMap<DistrictType, u32> = HashMap::new();
        for district_id in self.districts(){
            let district = districts.get(district_id).expect(&format!("District with id {} not found", district_id.0));
            let district_type = district.data.district_type;
            *subtypes.entry(district_type).or_insert(0) += 1;
        }
        subtypes
    }
}

impl AdjacencyAnalyzeable<SuperDistrictID> for SuperDistrict {
    fn increment_adjacency(&mut self, id: Option<SuperDistrictID>) {
        self.data.adjacencies_count += 1;
        if let Some(id) = id {
            *self.data.district_adjacency.entry(id).or_insert(0) += 1;
        }
    }

    fn add_edge(&mut self, point: Point3D) {
        self.data.edges.insert(point);
    }
}