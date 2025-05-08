use std::collections::{HashMap, HashSet};

use crate::{geometry::{Point2D, Point3D, Rect2D, Rect3D, CARDINALS}, noise::RNG};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DistrictID(pub usize);

#[derive(Debug)]
pub struct District {
    pub id: DistrictID,
    origin: Point3D,
    is_border : bool,
    pub points : HashSet<Point3D>,
    points_2d: HashSet<Point2D>,
    sum : Point3D,
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
} 

const CHUNK_SIZE: i32 = 16;
const RETRIES: i32 = 10;
const MIN_DISTANCE : i32 = 5;

pub fn generate_districts(seed : i32, build_rect : Rect3D, height_map : &Vec<Vec<i32>>) -> Vec<District> {
    let mut districts = spawn_districts(seed, build_rect, height_map);

    let mut district_map : Vec<Vec<Option<DistrictID>>> = vec![vec![None; build_rect.size.z as usize]; build_rect.size.x as usize]; 

    for district in districts.iter() {
        let x = district.origin.x as usize;
        let z = district.origin.z as usize;
        district_map[x][z] = Some(district.id);
    }

    let mut districts_dict : HashMap<DistrictID, &mut District> = districts.iter_mut()
        .map(|d| (d.id, d))
        .collect();

    bubble_out(&mut districts_dict, &mut district_map, build_rect);

    districts
}

fn bubble_out(districts : &mut HashMap<DistrictID, &mut District>, district_map : &mut Vec<Vec<Option<DistrictID>>>, build_rect : Rect3D) {
    let mut queue : Vec<Point3D> = districts.iter().map(|(_, d)| d.origin).collect::<Vec<_>>();
    let mut visited : HashSet<Point3D> = queue.iter().cloned().collect();

    while queue.len() > 0 {
        let next = queue.remove(0);

        for neighbour in CARDINALS.iter().map(|c| *c + next) {
            if visited.contains(&neighbour) {
                continue;
            }

            let current_district = district_map[next.x as usize][next.z as usize].expect("Every explored tile should have a district");

            if !build_rect.contains(neighbour) {
                districts.get_mut(&current_district).expect("Every explored tile should have a district").set_to_border_district();
                continue;
            }

            visited.insert(neighbour);
            district_map[neighbour.x as usize][neighbour.z as usize] = Some(current_district);
        }   
    }
}

fn spawn_districts(seed : i32, build_rect : Rect3D, height_map : &Vec<Vec<i32>>) -> Vec<District> {
    let mut rng = RNG::from_seed_and_string(0, "spawn_districts");

    let mut rects : Vec<Rect2D> = vec![];

    for i in 0..(build_rect.size.x / CHUNK_SIZE) * (build_rect.size.z / CHUNK_SIZE) {
       let x = i % (build_rect.size.x / CHUNK_SIZE);
       let z = i / (build_rect.size.x / CHUNK_SIZE);
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

            let trial_point = (rng.rand_point2D(rect.size) + rect.origin).add_height(height_map);

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