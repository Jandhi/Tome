use std::collections::{HashMap, HashSet};

use anyhow::Ok;
use fastnbt::LongArray;
use log::info;

use crate::{generator::{build_claim::BuildClaim, buildings::BuildingData, districts::{District, DistrictID, DistrictType, SuperDistrict, SuperDistrictID}}, geometry::{Point2D, Point3D, Rect2D, Rect3D}, http_mod::{GDMCHTTPProvider, HeightMapType}, minecraft::{util::point_to_chunk_coordinates, Biome, Block, BlockID, Chunk}};


use super::Editor;

const CHUNK_SIZE : i32 = 16;

#[derive(Debug)]
pub struct World {
    pub build_area : Rect3D,
    pub districts : HashMap<DistrictID, District>,
    pub super_districts : HashMap<SuperDistrictID, SuperDistrict>,
    pub district_map : Vec<Vec<Option<DistrictID>>>,
    pub super_district_map : Vec<Vec<Option<SuperDistrictID>>>,
    pub buildings : Vec<BuildingData>,

    ground_height_map : Vec<Vec<i32>>,
    ground_block_map : Vec<Vec<Block>>,
    ocean_floor_height_map : Vec<Vec<i32>>,
    ground_biome_map: Vec<Vec<Biome>>,
    motion_blocking_height_map : Vec<Vec<i32>>,
    build_claim_map : Vec<Vec<BuildClaim>>,
    pub chunks: HashMap<Point2D, Chunk>,
}

impl World {
    pub async fn new(provider: &GDMCHTTPProvider) -> anyhow::Result<Self> {
        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let (origin_x, origin_z, size_x, size_z) = (
            build_area.origin.x,
            build_area.origin.z,
            build_area.size.x,
            build_area.size.z,
        );
        let (size_x_usize, size_z_usize) = (size_x as usize, size_z as usize);

        let district_map = vec![vec![None; size_z_usize]; size_x_usize];
        let super_district_map = vec![vec![None; size_z_usize]; size_x_usize];
        
        let chunk_rect = Rect3D {
            origin: build_area.origin / CHUNK_SIZE,
            size: build_area.last() / CHUNK_SIZE - build_area.origin / CHUNK_SIZE + Point3D::new(1, 1, 1),
        };

        info!("Loading chunks...");
        let chunks = provider
            .get_chunks(
                chunk_rect.origin.x,
                chunk_rect.origin.y,
                chunk_rect.origin.z,
                chunk_rect.size.x,
                chunk_rect.size.y,
                chunk_rect.size.z,
            )
            .await?
            .into_iter()
            .map(|chunk| {
                let key = Point2D::new(chunk.x_pos, chunk.z_pos);
                (key, chunk)
            })
            .collect();

        info!("Loading heightmaps...");
        let ground_map = provider
            .get_heightmap(origin_x, origin_z, size_x, size_z, HeightMapType::MotionBlockingNoPlants)
            .await?;
        let ocean_map = provider
            .get_heightmap(origin_x, origin_z, size_x, size_z, HeightMapType::OceanFloorNoPlants)
            .await?;
        let motion_blocking_map = provider
            .get_heightmap(origin_x, origin_z, size_x, size_z, HeightMapType::MotionBlocking)
            .await?;

        let (size_x_usize, size_z_usize) = (size_x as usize, size_z as usize);

        let ground_height_map = vec![vec![0; size_z_usize]; size_x_usize];
        let ocean_floor_height_map = vec![vec![0; size_z_usize]; size_x_usize];
        let motion_blocking_height_map = vec![vec![0; size_z_usize]; size_x_usize];
        let ground_block_map = vec![vec![Block::new(BlockID::Unknown, None, None); size_z_usize]; size_x_usize];
        let build_claim_map = vec![vec![BuildClaim::None; size_z_usize]; size_x_usize];
        let ground_biome_map = vec![vec![Biome::Unknown; size_z_usize]; size_x_usize];

        let mut world = World {
            build_area,
            districts: HashMap::new(),
            super_districts: HashMap::new(),
            district_map,
            super_district_map,
            buildings: Vec::new(),
            ground_height_map,
            ocean_floor_height_map,
            motion_blocking_height_map,
            build_claim_map,
            chunks,
            ground_biome_map,
            ground_block_map,
        };

        let y_offset = build_area.origin.y;
        for x in 0..size_x_usize {
            for z in 0..size_z_usize {
                world.ground_height_map[x][z] = ground_map[x][z] - y_offset;
                world.ocean_floor_height_map[x][z] = ocean_map[x][z] - y_offset;
                world.motion_blocking_height_map[x][z] = motion_blocking_map[x][z] - y_offset;
                world.ground_block_map[x][z] = world.get_block(Point3D::new(x as i32, world.ground_height_map[x][z], z as i32)).expect("Failed to get block at point");
                world.ground_biome_map[x][z] = world.get_biome(Point3D::new(x as i32, world.ocean_floor_height_map[x][z], z as i32)).expect("Failed to get biome at point");
            }
        }

        Ok(world)
    }

    pub fn get_editor(self) -> Editor {
        Editor::new(self.build_area, self)
    }

    pub fn origin(&self) -> Point3D {
        self.build_area.origin
    }

    pub fn size(&self) -> Point3D {
        self.build_area.size
    }

    pub fn world_rect_2d(&self) -> Rect2D {
        Rect2D {
            origin: Point2D::new(0, 0),
            size: Point2D::new(self.build_area.size.x, self.build_area.size.z),
        }
    }

    pub fn iter_points_2d(&self) -> impl Iterator<Item = Point2D> {
        Rect2D{
            origin: Point2D::new(0, 0),
            size: Point2D::new(self.build_area.size.x, self.build_area.size.z),
        }.iter()
    }

    pub fn get_height_at(&self, point : Point2D) -> i32 {
        self.ground_height_map[point.x as usize][point.y as usize]
    }

    pub fn get_height_map(&self) -> &Vec<Vec<i32>> {
        &self.ground_height_map
    }   

    // Get height without counting water
    pub fn get_ocean_floor_height_at(&self, point : Point2D) -> i32 {
        self.ocean_floor_height_map[point.x as usize][point.y as usize]
    }

    pub fn get_motion_blocking_height_at(&self, point : Point2D) -> i32 {
        self.motion_blocking_height_map[point.x as usize][point.y as usize]
    }

    pub fn get_surface_biome_at(&self, point : Point2D) -> Biome {
        let height = self.get_ocean_floor_height_at(point);
        let point_3d = Point3D::new(point.x, height, point.y);
        self.get_biome(point_3d).expect("Failed to get biome at point")
    }

    pub fn get_district_at(&self, point : Point2D) -> Option<DistrictID> {
        self.district_map[point.x as usize][point.y as usize]
    }

    pub fn get_super_district_at(&self, point : Point2D) -> Option<SuperDistrictID> {
        self.super_district_map[point.x as usize][point.y as usize]
    }   

    pub fn add_height(&self, point : Point2D) -> Point3D {
        Point3D::new(point.x, self.get_height_at(point), point.y)
    }

    pub fn is_in_bounds_2d(&self, point : Point2D) -> bool {
        self.build_area.drop_y().contains(point + self.build_area.origin.drop_y())
    }

    pub fn get_block(&self, mut point: Point3D) -> Option<Block> {
        point = point + self.build_area.origin;
        info!("Getting block at point: {:?}", point);

        let chunk_coordinates = point_to_chunk_coordinates(point);

        let chunk = self.chunks.get(&chunk_coordinates.drop_y())?;

        let section = chunk.sections.iter().find(|s| s.y == (point.y / CHUNK_SIZE))?;

        let block_states = section.block_states.as_ref()?;

        if block_states.data.is_none() {
            let block = block_states.palette.get(0)?;
            return Some(Block {
                id: block.name.as_str().into(),
                state: block.properties.clone(),
                data: None,
            });
        }

        let data = block_states.data.as_ref()?;

        let block_index = self.get_data_index(data, point)?;
        
        let palette = &block_states.palette;

        palette.get(block_index).map(|block| Block {
            id: block.name.as_str().into(),
            state: block.properties.clone(),
            data: None,
        })
    }

    pub fn get_biome(&self, mut point: Point3D) -> Option<Biome> {
        point = point + self.build_area.origin;
        let chunk_coordinates = point_to_chunk_coordinates(point);
        let chunk = self.chunks.get(&chunk_coordinates.drop_y())?;
        let section = chunk.sections.iter().find(|s| s.y == (point.y / CHUNK_SIZE))?;
        let biomes = section.biomes.as_ref()?;

        if biomes.data.is_none() {
            return biomes.biomes.get(0).cloned();
        }

        let data = biomes.data.as_ref()?;
        let biome_index = self.get_data_index(data, point)?;
        let biome_list = &biomes.biomes;

        biome_list.get(biome_index).cloned()

    }

    pub fn get_data_index(&self, data : &LongArray, point : Point3D) -> Option<usize> {
        let index = ((point.x.rem_euclid(CHUNK_SIZE)) + (point.z.rem_euclid(CHUNK_SIZE)) * CHUNK_SIZE + (point.y.rem_euclid(CHUNK_SIZE)) * CHUNK_SIZE * CHUNK_SIZE) as usize;

        let indices_per_long = ((CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE) as f32 / data.len() as f32).ceil() as usize;

        let bits = 64 / indices_per_long;
        let long_index = index / indices_per_long;
        let bit_index = index % indices_per_long;

        let long = data.get(long_index)?;
        let block_index = (long >> (bit_index * bits)) & ((1 << bits) - 1);

        Some(block_index as usize)
    }

    pub fn is_water(&self, point : Point2D) -> bool {
        self.ground_block_map[point.x as usize][point.y as usize].id == BlockID::Water
    }

    pub fn is_water_3d(&self, point : Point3D) -> bool {
        self.get_block(point).expect("failed to get block").id == BlockID::Water
    }

    pub fn is_claimed(&self, point : Point2D) -> bool {
        self.build_claim_map[point.x as usize][point.y as usize] != BuildClaim::None
    }

    pub fn claim(&mut self, point: Point2D, claim: BuildClaim) {
        if self.is_in_bounds_2d(point) {
            self.build_claim_map[point.x as usize][point.y as usize] = claim;
        } else {
            log::warn!("Tried to claim point {:?} out of bounds", point);
        }
    }

    pub fn get_urban_points(&self) -> HashSet<Point2D> { // BUG, doesnt get all points for some reason a handful of points are missing
        self.iter_points_2d()
            .filter(|&point| self.get_district_type(point).expect("Failed to get district type") == DistrictType::Urban)
            .collect()
    }

    pub fn get_district_type(&self, point: Point2D) -> Option<DistrictType> {
        self.get_super_district_at(point).and_then(|district_id| {
            self.super_districts.get(&district_id).map(|district| district.data.district_type)
        })
    }
}