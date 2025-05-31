use std::{collections::{HashMap, HashSet}, hash::Hash};

use log::info;

use crate::{editor::World, geometry::{Point2D, Point3D, Rect2D, CARDINALS}, noise::{Seed, RNG}};

use super::{adjacency::{analyze_adjacency, AdjacencyAnalyzeable}, analysis::analyze_district, constants::{CHUNK_SIZE, NUM_RECENTER, SPAWN_DISTRICTS_MIN_DISTANCE, SPAWN_DISTRICTS_RETRIES}, data::{DistrictData, HasDistrictData}, merge::merge_down, DistrictAnalysis, SuperDistrict, SuperDistrictID};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DistrictID(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DistrictType {
    Urban,
    Rural,
    OffLimits,
}


#[derive(Debug, Clone)]
pub struct District {
    id: DistrictID,
    data : DistrictData<DistrictID>,
}

impl HasDistrictData<'_, DistrictID> for District {
    fn data(&self) -> &DistrictData<DistrictID> {
        &self.data
    }
}

impl District {
    pub fn id(&self) -> DistrictID {
        self.id
    }

   // Note: Methods that mutate self may need further lifetime or trait bound adjustments.
    // For now, keep them as is, but you may need to adjust them if you implement this trait.

    fn add_point(&mut self, point: Point3D) {
        self.data.points.insert(point);
        self.data.points_2d.insert(point.drop_y());
        self.data.sum = self.sum() + point;
    }

    fn set_to_border_district(&mut self) {
        self.data.is_border = true;
    }
}

impl AdjacencyAnalyzeable<DistrictID> for District {
    fn increment_adjacency(&mut self, id: Option<DistrictID>) {
        self.data.adjacencies_count += 1;
        if let Some(id) = id {
            *self.data.district_adjacency.entry(id).or_insert(0) += 1;
        }
    }

    fn add_edge(&mut self, point: Point3D) {
        self.data.edges.insert(point);
    }
}

pub async fn generate_districts(seed : Seed, world : &mut World) {
    info!("Generating districts with seed: {:?}", seed);

    let districts = spawn_districts(seed, world);

    for district in districts.iter() {
        let x = district.data.origin.x as usize;
        let z = district.data.origin.z as usize;
        world.district_map[x][z] = Some(district.id);
    }

    let mut districts : HashMap<DistrictID, District> = districts.into_iter()
        .map(|district| (district.id, district))
        .collect();

    info!("Bubbling out districts...");
    bubble_out(&mut districts, world);
    
    info!("Re-centering districts...");
    for _ in 0..NUM_RECENTER {
        recenter_districts(world, &mut districts);
    }

    info!("Analyzing adjacency of districts...");
    analyze_adjacency(&mut districts, world.get_height_map(), &world.district_map, &world.world_rect_2d());
    
    info!("Creating superdistricts...");
    // TODO: super districts
    let mut super_district_id_counter = 0;
    let mut super_districts : HashMap<SuperDistrictID, SuperDistrict> = HashMap::new();
    for district in districts.values_mut() {
        let id = SuperDistrictID(super_district_id_counter);
        super_district_id_counter += 1;
        let mut super_district = SuperDistrict::new(id);
        super_district.add_district(&district, world);
        super_districts.insert(super_district.id(), super_district);
    }

    // Get District Analysis Data
    let mut district_analysis_data : HashMap<SuperDistrictID, DistrictAnalysis> = HashMap::new();
    for district in super_districts.values() {
        info!("Analyzing district {}", district.id().0);
        district_analysis_data.insert(district.id(), analyze_district(district.data(), world).await);
    }

    info!("Merging down superdistricts...");
    analyze_adjacency(&mut super_districts, world.get_height_map(), &world.super_district_map, &world.world_rect_2d());
    merge_down(&mut super_districts, &districts, &mut district_analysis_data, world).await;
    analyze_adjacency(&mut super_districts, world.get_height_map(), &world.super_district_map, &world.world_rect_2d());

    world.districts = districts;
    info!("Districts generated successfully");
}

fn bubble_out(districts : &mut HashMap<DistrictID, District>, world : &mut World) {
    let mut queue : Vec<Point3D> = districts.iter().map(|(_, district)| district.data.origin).collect::<Vec<_>>();
    let mut visited : HashSet<Point3D> = queue.iter().cloned().collect();

    while queue.len() > 0 {
        let next = queue.remove(0);

        let current_district = world.district_map[next.x as usize][next.z as usize].expect("Every explored tile should have a district");

        for neighbour in CARDINALS.iter().map(|c| *c + next) {
            if visited.contains(&neighbour) {
                continue;
            }

            if !world.build_area.contains(world.build_area.origin.without_y() + neighbour) {
                districts.get_mut(&current_district).expect(&format!("No district found with id {}", current_district.0)).set_to_border_district();
                continue;
            }
        
            visited.insert(neighbour);
            queue.push(neighbour);
            world.district_map[neighbour.x as usize][neighbour.z as usize] = Some(current_district);
            districts.get_mut(&current_district)
                .expect(&format!("No district found with id {}", current_district.0))
                .add_point(neighbour);
        }
    }
}

fn recenter_districts(world : &mut World, districts : &mut HashMap<DistrictID, District>) {
    world.district_map = vec![vec![None; world.build_area.size.z as usize]; world.build_area.size.x as usize];
        
    for district in districts.values_mut() {
        district.data.origin = world.add_height(district.average().drop_y());
        district.data.points.clear();
        district.data.points_2d.clear();
        district.data.sum = Point3D::default();
        district.data.is_border = false;
        district.add_point(district.data.origin);

        world.district_map[district.data.origin.x as usize][district.data.origin.z as usize] = Some(district.id);
    }

    bubble_out(districts, world);
}



fn spawn_districts(seed : Seed, world : &mut World) -> Vec<District> {
    let mut rng = RNG::from_seed_and_string(seed, "spawn_districts");

    let mut rects : Vec<Rect2D> = vec![];

    for i in 0..(world.build_area.size.x / CHUNK_SIZE) * (world.build_area.size.z / CHUNK_SIZE) {
       let x = i % (world.build_area.size.x / CHUNK_SIZE);
       let z = i / (world.build_area.size.x / CHUNK_SIZE);
       let rect = Rect2D::new(
           Point2D::new(x * CHUNK_SIZE, z * CHUNK_SIZE),
           Point2D::new(CHUNK_SIZE, CHUNK_SIZE)
       );
       rects.push(rect);
    }

    let mut points : Vec<Point3D> = vec![];

    for rect in rects.iter() {
        let mut trials = 0;

        while trials < SPAWN_DISTRICTS_RETRIES {
            trials += 1;

            let trial_point = world.add_height(rng.rand_point2d(rect.size) + rect.origin);

            if points.iter().all(|p| p.distance_squared(&trial_point) > SPAWN_DISTRICTS_MIN_DISTANCE * SPAWN_DISTRICTS_MIN_DISTANCE) {
                points.push(trial_point);
                break;
            }
        }
    }

    let mut id = 0;

    points.iter().map(|p| {
        let district = District {
            id: DistrictID(id),
            data: DistrictData::new(*p),
        };
        id += 1;
        district
    }).collect()
}



