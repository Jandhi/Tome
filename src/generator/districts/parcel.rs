use std::{collections::{HashMap, HashSet}, hash::Hash};

use log::info;

use crate::{editor::{Editor, World}, geometry::{Point2D, Point3D, Rect2D, CARDINALS_2D}, noise::{Seed, RNG}};

use super::{adjacency::{analyze_adjacency, AdjacencyAnalyzeable}, analysis::analyze_parcel, constants::{CHUNK_SIZE, NUM_RECENTER, SPAWN_PARCELS_MIN_DISTANCE, SPAWN_PARCELS_RETRIES}, data::{ParcelData, HasParcelData}, merge::merge_down, classification::{classify_parcels, classify_districts}, footprint::{regularize_urban_footprint, reconcile_districts_to_footprint}, ParcelAnalysis, District, DistrictID};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParcelID(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParcelType {
    Unknown, // placeholder for unclassified parcels
    Urban,
    Rural,
    /// A district dominated by water (lake / ocean / wide river). Excluded from
    /// urban and rural building placement; consumed by the ship-placement pass.
    Water,
    OffLimits,
}


#[derive(Debug, Clone)]
pub struct Parcel {
    pub id: ParcelID,
    pub data : ParcelData<ParcelID>,
}

impl HasParcelData<'_, ParcelID> for Parcel {
    fn data(&self) -> &ParcelData<ParcelID> {
        &self.data
    }
}

impl Parcel {
    pub fn id(&self) -> ParcelID {
        self.id
    }

   // Note: Methods that mutate self may need further lifetime or trait bound adjustments.
    // For now, keep them as is, but you may need to adjust them if you implement this trait.

    fn add_point(&mut self, point: Point3D) {
        self.data.points.insert(point);
        self.data.points_2d.insert(point.drop_y());
        self.data.sum = self.sum() + point;
    }

    fn set_to_border_parcel(&mut self) {
        self.data.is_border = true;
    }

    pub fn get_adjacency_ratio(&mut self, id: ParcelID) -> f32 {
        let count = self.data.parcel_adjacency.get(&id).cloned().unwrap_or(0);
        count as f32 / self.data.adjacencies_count as f32   
    }
}

impl AdjacencyAnalyzeable<ParcelID> for Parcel {
    fn increment_adjacency(&mut self, id: Option<ParcelID>) {
        self.data.adjacencies_count += 1;
        if let Some(id) = id {
            *self.data.parcel_adjacency.entry(id).or_insert(0) += 1;
        }
    }

    fn add_edge(&mut self, point: Point3D) {
        self.data.edges.insert(point);
    }
}

pub async fn generate_parcels(seed : Seed, editor : &mut Editor) {
    info!("Generating parcels with seed: {:?}", seed);

    let parcels = spawn_parcels(seed, editor.world_mut());
    for parcel in parcels.iter() {
        let x = parcel.data.origin.x as usize;
        let z = parcel.data.origin.z as usize;
        editor.world_mut().parcel_map[x][z] = Some(parcel.id);
    }

    let mut parcels : HashMap<ParcelID, Parcel> = parcels.into_iter()
        .map(|parcel| (parcel.id, parcel))
        .collect();

    

    info!("Bubbling out parcels...");
    bubble_out(&mut parcels, editor.world_mut());
    
    info!("Re-centering parcels...");
    for _ in 0..NUM_RECENTER {
        recenter_parcels(editor.world_mut(), &mut parcels);
    }

    info!("Analyzing adjacency of parcels...");
    {
        let world = editor.world_mut();
        analyze_adjacency(&mut parcels, world.get_height_map(), &world.parcel_map, &world.world_rect_2d(), false);
    }
    
    info!("Creating districts...");
    // TODO: super parcels
    let mut district_id_counter = 0;
    let mut districts : HashMap<DistrictID, District> = HashMap::new();
    let mut parcel_analysis_data : HashMap<ParcelID, ParcelAnalysis> = HashMap::new();

    for parcel in parcels.values() {
        info!("Analyzing parcel {}", parcel.id.0);
        let analysis = analyze_parcel(parcel.data(), editor).await;
        parcel_analysis_data.insert(parcel.id, analysis);
    }

    for parcel in parcels.values_mut() {
        let id = DistrictID(district_id_counter);
        district_id_counter += 1;
        let mut district = District::new(id);
        district.add_parcel(&parcel, editor.world_mut());
        districts.insert(district.id(), district);
    }

    // Get Parcel Analysis Data
    let mut district_analysis_data : HashMap<DistrictID, ParcelAnalysis> = HashMap::new();
    for parcel in districts.values() {
        info!("Analyzing parcel {}", parcel.id().0);
        district_analysis_data.insert(parcel.id(), analyze_parcel(parcel.data(), editor).await);
    }

    //parcel classification
    classify_parcels(&mut parcels, &parcel_analysis_data);

    {
        let world = editor.world_mut();
        analyze_adjacency(&mut districts, world.get_height_map(), &world.district_map, &world.world_rect_2d(), true);
    }
    info!("Merging down districts...");
    merge_down(&mut districts, &parcels, &mut district_analysis_data, editor).await;
    {
        let world = editor.world_mut();
        analyze_adjacency(&mut districts, world.get_height_map(), &world.district_map, &world.world_rect_2d(),false);
    }

    editor.world_mut().parcels = parcels;
    editor.world_mut().districts = districts;

    // district classification
    let world = editor.world_mut();
    classify_districts(&mut world.districts, &mut world.parcels, &district_analysis_data);

    // Regularize the urban footprint (smooth the wall outline) and re-vote each
    // district's urban/rural classification against it so every downstream consumer
    // stays consistent with "inside the wall".
    let raw_urban = world.get_urban_points();
    let footprint = regularize_urban_footprint(world, &raw_urban);
    reconcile_districts_to_footprint(&mut world.districts, &footprint);
    world.urban_footprint = Some(footprint);

    info!("Parcels generated successfully");

    //prune urban chokepoints??

    // Set the parcel and district analysis data in the world
    editor.world_mut().parcel_analysis_data = parcel_analysis_data;
    editor.world_mut().district_analysis_data = district_analysis_data;
}

fn bubble_out(parcels : &mut HashMap<ParcelID, Parcel>, world : &mut World) { // this is broken
    let mut queue : Vec<Point2D> = parcels.iter().map(|(_, parcel)| parcel.data.origin.drop_y()).collect::<Vec<_>>();
    let mut visited : HashSet<Point2D> = queue.iter().cloned().collect();

    while queue.len() > 0 {
        let next = queue.remove(0);

        let current_parcel = world.parcel_map[next.x as usize][next.y as usize].expect("Every explored tile should have a parcel");

        for neighbour in CARDINALS_2D.iter().map(|c| *c + next) {
            if visited.contains(&neighbour) {
                continue;
            }

            if !world.is_in_bounds_2d(neighbour) {
                info!("Skipping {:?} because it is out of bounds", neighbour);
                parcels.get_mut(&current_parcel).expect("Every explored tile should have a parcel").set_to_border_parcel();
                continue;
            }

            let Some(neighbour_point) = world.add_non_tree_height(neighbour) else { continue; };
            visited.insert(neighbour);
            queue.push(neighbour);
            world.parcel_map[neighbour.x as usize][neighbour.y as usize] = Some(current_parcel);
            parcels.get_mut(&current_parcel)
                .expect(&format!("No parcel found with id {}", current_parcel.0))
                .add_point(neighbour_point);
        }
    }
}

fn recenter_parcels(world : &mut World, parcels : &mut HashMap<ParcelID, Parcel>) {
    world.parcel_map = vec![vec![None; world.build_area.size.z as usize]; world.build_area.size.x as usize];
        
    for parcel in parcels.values_mut() {
        let Some(origin) = world.add_non_tree_height(parcel.average().drop_y()) else { continue; };
        parcel.data.origin = origin;
        parcel.data.points.clear();
        parcel.data.points_2d.clear();
        parcel.data.sum = Point3D::default();
        parcel.data.is_border = false;
        parcel.add_point(parcel.data.origin);

        world.parcel_map[parcel.data.origin.x as usize][parcel.data.origin.z as usize] = Some(parcel.id);
    }

    bubble_out(parcels, world);
}



fn spawn_parcels(seed : Seed, world : &mut World) -> Vec<Parcel> {
    let mut rng = RNG::from_seed_and_string(seed, "spawn_parcels");

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

        while trials < SPAWN_PARCELS_RETRIES {
            trials += 1;

            let Some(trial_point) = world.add_non_tree_height(rng.rand_point2d(rect.size) + rect.origin) else { continue; };//fix to use non tree height

            if points.iter().all(|p| p.distance_squared(trial_point) > SPAWN_PARCELS_MIN_DISTANCE * SPAWN_PARCELS_MIN_DISTANCE) {
                points.push(trial_point);
                break;
            }
        }
    }

    let mut id = 0;

    points.iter().map(|p| {
        let parcel = Parcel {
            id: ParcelID(id),
            data: ParcelData::new(*p),
        };
        id += 1;
        parcel
    }).collect()
}



