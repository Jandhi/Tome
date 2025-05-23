use std::collections::{HashMap, HashSet};

use log::{error, warn};

use crate::{editor::World, geometry::{Point2D, Point3D, Rect2D, CARDINALS, CARDINALS_2D, X_PLUS_2D, Y_PLUS_2D}, minecraft::{Biome, BlockID}, noise::{Seed, RNG}};

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
    origin: Point3D,
    is_border: bool,
    points: HashSet<Point3D>,
    points_2d: HashSet<Point2D>,
    edges : HashSet<Point3D>,
    sum: Point3D,
    district_type: DistrictType,
    district_adjacency : HashMap<DistrictID, u32>,
    adjacencies_count : u32,

    roughness: f32,
    water_percentage: f32,
    forested_percentage: f32,
    surface_block_count: HashMap<BlockID, u32>,
    biome_count: HashMap<Biome, u32>,
    gradient: f32,
}

impl District {
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

    pub fn district_type(&self) -> DistrictType {
        self.district_type
    }

    pub fn roughness(&self) -> f32 {
        self.roughness
    }

    pub fn water_percentage(&self) -> f32 {
        self.water_percentage
    }

    pub fn forested_percentage(&self) -> f32 {
        self.forested_percentage
    }

    pub fn surface_block_count(&self) -> &HashMap<BlockID, u32> {
        &self.surface_block_count
    }

    pub fn biome_count(&self) -> &HashMap<Biome, u32> {
        &self.biome_count
    }

    pub fn gradient(&self) -> f32 {
        self.gradient
    }    

    fn add_point(&mut self, point: Point3D) {
        self.points.insert(point);
        self.points_2d.insert(point.drop_y());
        self.sum = self.sum + point;
    }

    fn set_to_border_district(&mut self) {
        self.is_border = true;
    }

    fn average(&self) -> Point3D {
        if self.points.len() == 0 {
            return Point3D::default();
        }
        self.sum / (self.points.len() as i32)
    }
} 

const CHUNK_SIZE: i32 = 16;
const RETRIES: i32 = 10;
const MIN_DISTANCE : i32 = 5;
const NUM_RECENTER : i32 = 2;
const TARGET_DISTRICT_AMOUNT : u32 = 16; 

pub async fn generate_districts(seed : Seed, world : &mut World) {
    let districts = spawn_districts(seed, world);

    for district in districts.iter() {
        let x = district.origin.x as usize;
        let z = district.origin.z as usize;
        world.district_map[x][z] = Some(district.id);
    }

    let mut districts : HashMap<DistrictID, District> = districts.into_iter()
        .map(|d| (d.id, d))
        .collect();

    bubble_out(&mut districts, world);
    
    for _ in 0..NUM_RECENTER {
        recenter_districts(world, &mut districts);
    }

    establish_adjacency(&mut districts, world);
    
    // TODO: super districts
    let mut super_district_id_counter = districts.len();
    let mut super_districts : HashMap<DistrictID, District> = HashMap::new();
    for district in districts.values_mut() {
        analyze_district(district, world).await;
        let mut super_district = district.clone();
        super_district.id = DistrictID(super_district_id_counter);
        super_district_id_counter += 1;
        
        for point in super_district.points_2d.iter() {
            world.super_district_map[point.x as usize][point.y as usize] = Some(super_district.id);
        }

        super_districts.insert(super_district.id, super_district);
    }

    // establish super adjacency
    // merge down
    // remeasure adjacency

    // prune urban chokepoints

    world.districts = districts;
}

fn bubble_out(districts : &mut HashMap<DistrictID, District>, world : &mut World) {
    let mut queue : Vec<Point3D> = districts.iter().map(|(_, d)| d.origin).collect::<Vec<_>>();
    let mut visited : HashSet<Point3D> = queue.iter().cloned().collect();

    while queue.len() > 0 {
        let next = queue.remove(0);
        let current_district = world.district_map[next.x as usize][next.z as usize].expect("Every explored tile should have a district");

        for neighbour in CARDINALS.iter().map(|c| *c + next) {
            if visited.contains(&neighbour) {
                continue;
            }

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

fn recenter_districts(world : &mut World, districts : &mut HashMap<DistrictID, District>) {
    world.district_map = vec![vec![None; world.build_area.size.z as usize]; world.build_area.size.x as usize];
        
    for district in districts.values_mut() {
        district.origin = world.add_height(district.average().drop_y());
        district.points.clear();
        district.points_2d.clear();
        district.sum = Point3D::default();
        district.is_border = false;
        district.add_point(district.origin);
    }

    bubble_out(districts, world);
}

async fn analyze_district(district: &mut District, world: &mut World) {
    let average = district.average();
    let average_height = average.y;
    let number_of_points = district.points.len() as f32;

    let mut water_blocks = 0;
    let mut leaf_blocks = 0;
    let mut neighbour_height_sum = 0.0;
    let mut root_mean_square_height = 0.0;

    let mut biome_count: HashMap<Biome, u32> = HashMap::new();
    let mut surface_block_count: HashMap<BlockID, u32> = HashMap::new();

    let mut editor = world.get_editor();

    for point in &district.points {
        let biome = world.get_surface_biome_at(point.drop_y());
        let block = editor.get_block(*point).await;
        let is_water = block.id.is_water();
        let leaf_height = world.get_motion_blocking_height_at(point.drop_y());

        root_mean_square_height += ((point.y - average_height) as f32).powi(2);

        let height = world.get_height_at(point.drop_y());
        let average_neighbour_height = CARDINALS_2D.iter()
            .map(|cardinal| {
                let neighbour = point.drop_y() + *cardinal;
                if world.is_in_bounds_2d(neighbour) {
                    world.get_height_at(neighbour)
                } else {
                    height
                }
            })
            .sum::<i32>() as f32 / 4.0;

        neighbour_height_sum += average_neighbour_height;

        *biome_count.entry(biome).or_insert(0) += 1;
        *surface_block_count.entry(block.id).or_insert(0) += 1;

        if is_water {
            water_blocks += 1;
        }
        if point.y < leaf_height {
            leaf_blocks += 1;
        }
    }

    let num_points = if number_of_points == 0.0 { 1.0 } else { number_of_points };
    district.roughness = (root_mean_square_height / num_points).sqrt();
    district.gradient = neighbour_height_sum / num_points;
    district.water_percentage = (water_blocks as f32 / num_points) * 100.0;
    district.forested_percentage = (leaf_blocks as f32 / num_points) * 100.0;
    district.surface_block_count = surface_block_count;
    district.biome_count = biome_count;
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
        let district = District {
            id: DistrictID(id),
            origin: *p,
            is_border: false,
            points: {
                let mut set = HashSet::new();
                set.insert(*p);
                set
            },
            points_2d: {
                let mut set = HashSet::new();
                set.insert(p.drop_y());
                set
            },
            edges: HashSet::new(),
            sum: *p,
            district_type: DistrictType::Rural,
            district_adjacency: HashMap::new(),
            adjacencies_count: 0,
            roughness: 0.0,
            water_percentage: 0.0,
            forested_percentage: 0.0,
            surface_block_count: HashMap::new(),
            biome_count: HashMap::new(),
            gradient: 0.0,
        };
        id += 1;
        district
    }).collect()
}

fn establish_adjacency(districts : &mut HashMap<DistrictID, District>, world : &mut World) {
    for point in world.iter_points_2d() {
        if world.get_district_at(point).is_none() {
            warn!("No district at point {:?}. This should not be possible.", point);
            continue;
        }

        let district_id = world.get_district_at(point).expect("This should be here");
        let mut is_edge = false;
        let height = world.get_height_at(point);

        // Check near edges
        if point.x == 0 || point.y == 0 {
            let district = districts.get_mut(&district_id).expect("Could not find district with id");
            district.district_type = DistrictType::OffLimits;
        }

        for neighbour_point in [point + X_PLUS_2D, point + Y_PLUS_2D] {
            // Hit the far edge
            if !world.is_in_bounds_2d(neighbour_point) {
                is_edge = true;
                let district = districts.get_mut(&district_id).expect("Could not find district with id");
                district.district_type = DistrictType::OffLimits;
                continue;
            }
            
            // If the neighbour is empty, only increment the adjacency count
            if world.get_district_at(neighbour_point).is_none() {
                districts.get_mut(&district_id).expect("Could not find district with id").adjacencies_count += 1;
                is_edge = true;
                continue;
            }

            let neighbour_district_id = world.get_district_at(neighbour_point).expect("This should be here");

            if neighbour_district_id == district_id {
                continue;
            }

            is_edge = true;

            let neighbour_height = world.get_height_at(neighbour_point);

            // If the neighbour is not walkable from this point
            // TODO: Consider whether this is useful
            if (neighbour_height - height).abs() > 1 {
                continue;
            }

            districts.get_mut(&district_id).expect("Could not find district with id").district_adjacency
                .entry(neighbour_district_id)
                .and_modify(|e| *e += 1)
                .or_insert(1);

            districts.get_mut(&neighbour_district_id).expect("Could not find district with id").district_adjacency
                .entry(district_id)
                .and_modify(|e| *e += 1)
                .or_insert(1);

        }

        if is_edge {
            let district = districts.get_mut(&district_id).expect("Could not find district with id");
            district.edges.insert(Point3D::new(point.x, height, point.y));
        }
    }
}