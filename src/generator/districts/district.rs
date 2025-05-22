use std::collections::{HashMap, HashSet};
use crate::{editor::World, geometry::{Point2D, Point3D, Rect2D, CARDINALS}, minecraft::BlockID, noise::{Seed, RNG}};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DistrictID(pub usize);

#[derive(Debug)]
pub struct District {
    id: DistrictID,
    origin: Point3D,
    is_border: bool,
    points: HashSet<Point3D>,
    points_2d: HashSet<Point2D>,
    sum: Point3D,

    roughness: f32,
    water_percentage: f32,
    forested_percentage: f32,
    surface_block_count: HashMap<BlockID, i32>,
    biome_count: HashMap<BlockID, i32>,
}

impl District {
    pub fn new(id: DistrictID, origin: Point3D) -> Self {
        let mut district = District {
            id,
            origin,
            is_border: false,
            points: HashSet::new(),
            points_2d: HashSet::new(),
            sum: Default::default(),

            roughness: 0.0,
            water_percentage: 0.0,
            forested_percentage: 0.0,
            surface_block_count: HashMap::new(),
            biome_count: HashMap::new(),
        };
        district.add_point(origin);
        district
    }

    pub fn add_point(&mut self, point: Point3D) {
        self.points.insert(point);
        self.points_2d.insert(point.drop_y());
        self.sum = self.sum + point;
    }

    pub fn set_to_border_district(&mut self) {
        self.is_border = true;
    }

    pub fn id(&self) -> DistrictID {
        self.id
    }

    pub fn origin(&self) -> Point3D {
        self.origin
    }

    pub fn is_border(&self) -> bool {
        self.is_border
    }

    pub fn points(&self) -> &HashSet<Point3D> {
        &self.points
    }

    pub fn points_2d(&self) -> &HashSet<Point2D> {
        &self.points_2d
    }

    pub fn sum(&self) -> Point3D {
        self.sum
    }

    pub fn average(&self) -> Point3D {
        if self.points.len() == 0 {
            return Point3D::default();
        }
        self.sum / (self.points.len() as i32)
    }
} 

const CHUNK_SIZE: i32 = 16;
const RETRIES: i32 = 10;
const MIN_DISTANCE : i32 = 5;

pub fn generate_districts(seed : Seed, world : &mut World) -> Vec<District> {
    let mut districts = spawn_districts(seed, world);

    let mut district_map : Vec<Vec<Option<DistrictID>>> = vec![vec![None; world.build_area.size.z as usize]; world.build_area.size.x as usize]; 

    for district in districts.iter() {
        let x = district.origin.x as usize;
        let z = district.origin.z as usize;
        district_map[x][z] = Some(district.id);
    }

    let mut districts_dict : HashMap<DistrictID, &mut District> = districts.iter_mut()
        .map(|d| (d.id, d))
        .collect();

    bubble_out(&mut districts_dict, world);

    districts
}

fn bubble_out(districts : &mut HashMap<DistrictID, &mut District>, world : &mut World) {
    let mut queue : Vec<Point3D> = districts.iter().map(|(_, d)| d.origin).collect::<Vec<_>>();
    let mut visited : HashSet<Point3D> = queue.iter().cloned().collect();

    while queue.len() > 0 {
        let next = queue.remove(0);

        for neighbour in CARDINALS.iter().map(|c| *c + next) {
            if visited.contains(&neighbour) {
                continue;
            }

            let current_district = world.district_map[next.x as usize][next.z as usize].expect("Every explored tile should have a district");

            if !world.build_area.contains(world.build_area.origin.without_y() + neighbour) {
                districts.get_mut(&current_district).expect("Every explored tile should have a district").set_to_border_district();
                continue;
            }
        
            visited.insert(neighbour);
            queue.push(neighbour);
            world.district_map[neighbour.x as usize][neighbour.z as usize] = Some(current_district);
        }   
    }
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

        while trials < RETRIES {
            trials += 1;

            let trial_point = world.add_height(rng.rand_point2d(rect.size) + rect.origin);

            if points.iter().all(|p| p.distance_squared(&trial_point) > MIN_DISTANCE * MIN_DISTANCE) {
                points.push(trial_point);
                break;
            }
        }
    }

    let mut id = 0;

    points.iter().map(|p| {
        let district = District::new(DistrictID(id), *p);
        id += 1;
        district
    }).collect()
}