use std::{collections::{HashMap, HashSet}, hash::Hash};

use log::info;

use crate::{editor::{Editor, World}, geometry::{Point2D, Point3D, Rect2D, CARDINALS_2D}, noise::{Seed, RNG}};

use super::{adjacency::{analyze_adjacency, AdjacencyAnalyzeable}, analysis::analyze_district, constants::{CHUNK_SIZE, NUM_RECENTER, SPAWN_DISTRICTS_MIN_DISTANCE, SPAWN_DISTRICTS_RETRIES}, data::{DistrictData, HasDistrictData}, merge::merge_down, classification::{classify_districts, classify_superdistricts}, DistrictAnalysis, SuperDistrict, SuperDistrictID};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DistrictID(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DistrictType {
    Unknown, // placeholder for unclassified districts
    Urban,
    Rural,
    OffLimits,
}


#[derive(Debug, Clone)]
pub struct District {
    pub id: DistrictID,
    pub data : DistrictData<DistrictID>,
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

    pub fn get_adjacency_ratio(&mut self, id: DistrictID) -> f32 {
        let count = self.data.district_adjacency.get(&id).cloned().unwrap_or(0);
        count as f32 / self.data.adjacencies_count as f32   
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

pub async fn generate_districts(seed : Seed, editor : &mut Editor) {
    info!("Generating districts with seed: {:?}", seed);

    let districts = spawn_districts(seed, editor.world_mut());
    for district in districts.iter() {
        let x = district.data.origin.x as usize;
        let z = district.data.origin.z as usize;
        editor.world_mut().district_map[x][z] = Some(district.id);
    }

    let mut districts : HashMap<DistrictID, District> = districts.into_iter()
        .map(|district| (district.id, district))
        .collect();

    

    info!("Bubbling out districts...");
    bubble_out(&mut districts, editor.world_mut());

    editor.world_mut().districts = districts;
    return;
    
    info!("Re-centering districts...");
    for _ in 0..NUM_RECENTER {
        recenter_districts(editor.world_mut(), &mut districts);
    }

    info!("Analyzing adjacency of districts...");
    {
        let world = editor.world_mut();
        analyze_adjacency(&mut districts, world.get_height_map(), &world.district_map, &world.world_rect_2d(), false);
    }
    
    info!("Creating superdistricts...");
    // TODO: super districts
    let mut super_district_id_counter = 0;
    let mut super_districts : HashMap<SuperDistrictID, SuperDistrict> = HashMap::new();
    let mut district_analysis_data : HashMap<DistrictID, DistrictAnalysis> = HashMap::new();

    for district in districts.values() {
        info!("Analyzing district {}", district.id.0);
        let analysis = analyze_district(district.data(), editor).await;
        district_analysis_data.insert(district.id, analysis);
    }

    for district in districts.values_mut() {
        let id = SuperDistrictID(super_district_id_counter);
        super_district_id_counter += 1;
        let mut super_district = SuperDistrict::new(id);
        super_district.add_district(&district, editor.world_mut());
        super_districts.insert(super_district.id(), super_district);
    }

    // Get District Analysis Data
    let mut superdistrict_analysis_data : HashMap<SuperDistrictID, DistrictAnalysis> = HashMap::new();
    for district in super_districts.values() {
        info!("Analyzing district {}", district.id().0);
        superdistrict_analysis_data.insert(district.id(), analyze_district(district.data(), editor).await);
    }

    //district classification
    classify_districts(&mut districts, &district_analysis_data);

    {
        let world = editor.world_mut();
        analyze_adjacency(&mut super_districts, world.get_height_map(), &world.super_district_map, &world.world_rect_2d(), true);
    }
    info!("Merging down superdistricts...");
    merge_down(&mut super_districts, &districts, &mut superdistrict_analysis_data, editor).await;
    {
        let world = editor.world_mut();
        analyze_adjacency(&mut super_districts, world.get_height_map(), &world.super_district_map, &world.world_rect_2d(),false);
    }

    editor.world_mut().districts = districts;
    editor.world_mut().super_districts = super_districts;

    // superdistrict classification
    let world = editor.world_mut();
    classify_superdistricts(&mut world.super_districts, &mut world.districts, &superdistrict_analysis_data);
    info!("Districts generated successfully");

    //prune urban chokepoints??
}

fn bubble_out(districts : &mut HashMap<DistrictID, District>, world : &mut World) { // this is broken
    let mut queue : Vec<Point2D> = districts.iter().map(|(_, district)| district.data.origin.drop_y()).collect::<Vec<_>>();
    let mut visited : HashSet<Point2D> = queue.iter().cloned().collect();

    while queue.len() > 0 {
        let next = queue.remove(0);

        println!("Bubbling out from {:?}", next);
        let current_district = world.district_map[next.x as usize][next.y as usize].expect("Every explored tile should have a district");
        println!("Current district: {:?}", current_district);

        for neighbour in CARDINALS_2D.iter().map(|c| *c + next) {
            if visited.contains(&neighbour) {
                continue;
            }

            if !world.is_in_bounds_2d(neighbour) {
                info!("Skipping {:?} because it is out of bounds", neighbour);
                districts.get_mut(&current_district).expect("Every explored tile should have a district").set_to_border_district();
                continue;
            }

            visited.insert(neighbour);
            queue.push(neighbour);
            world.district_map[neighbour.x as usize][neighbour.y as usize] = Some(current_district);
            districts.get_mut(&current_district)
                .expect(&format!("No district found with id {}", current_district.0))
                .add_point(world.add_height(neighbour));
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

            if points.iter().all(|p| p.distance_squared(trial_point) > SPAWN_DISTRICTS_MIN_DISTANCE * SPAWN_DISTRICTS_MIN_DISTANCE) {
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



