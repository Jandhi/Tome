use std::{collections::HashMap, future::Future, pin::Pin};

use anyhow::Ok;
use log::info;

use crate::{generator::districts::{District, DistrictID, SuperDistrictID}, geometry::{Point2D, Point3D, Rect2D, Rect3D}, http_mod::{GDMCHTTPProvider, HeightMapType}, minecraft::{util::point_to_chunk_coordinates, Biome, Block, BlockID, Chunk}};

use super::Editor;

const CHUNK_SIZE : i32 = 16;

#[derive(Debug)]
pub struct World {
    pub build_area : Rect3D,
    pub districts : HashMap<DistrictID, District>,
    pub district_map : Vec<Vec<Option<DistrictID>>>,
    pub super_district_map : Vec<Vec<Option<SuperDistrictID>>>,

    ground_height_map : Vec<Vec<i32>>,
    surface_height_map : Vec<Vec<i32>>,
    motion_blocking_height_map : Vec<Vec<i32>>,
    surface_biome_map : Vec<Vec<Biome>>,

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
            origin: build_area.origin / 16,
            size: build_area.last() / 16 - build_area.origin / 16 + Point3D::new(1, 1, 1),
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

        let mut ground_height_map = vec![vec![0; size_z_usize]; size_x_usize];
        let mut surface_height_map = vec![vec![0; size_z_usize]; size_x_usize];
        let mut motion_blocking_height_map = vec![vec![0; size_z_usize]; size_x_usize];

        let y_offset = build_area.origin.y;
        for x in 0..size_x_usize {
            for z in 0..size_z_usize {
                ground_height_map[x][z] = ground_map[x][z] - y_offset;
                surface_height_map[x][z] = ocean_map[x][z] - y_offset;
                motion_blocking_height_map[x][z] = motion_blocking_map[x][z] - y_offset;
            }
        }

        let mut world = World {
            build_area,
            districts: HashMap::new(),
            district_map,
            super_district_map,
            ground_height_map,
            surface_height_map,
            motion_blocking_height_map,
            surface_biome_map: vec![vec![Biome::Unknown; size_z_usize]; size_x_usize],
            chunks,
        };

        world.init_surface_biome_map(provider).await?;
        Ok(world)
    }

    pub fn get_editor(&self) -> Editor {
        Editor::new(self.build_area)
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

    // Initializes the surface biome map a chunk at a time
    async fn init_surface_biome_map(&mut self, provider : &GDMCHTTPProvider) -> anyhow::Result<()> {
        info!("Initializing surface biome map");
        self.surface_biome_map = vec![vec![Biome::Unknown; self.build_area.size.z as usize]; self.build_area.size.x as usize];

        for x in 0..((self.build_area.size.x + CHUNK_SIZE - 1) / CHUNK_SIZE) {
            for z in 0..((self.build_area.size.z + CHUNK_SIZE - 1) / CHUNK_SIZE) {
                let chunk_origin = Point2D::new(x * CHUNK_SIZE, z * CHUNK_SIZE);
                let chunk_size_x = ((self.build_area.size.x - x * CHUNK_SIZE).min(CHUNK_SIZE)).max(0);
                let chunk_size_z = ((self.build_area.size.z - z * CHUNK_SIZE).min(CHUNK_SIZE)).max(0);
                let size = Rect2D::new(chunk_origin, Point2D::new(chunk_size_x, chunk_size_z));
                self.initialize_surface_biome_chunk(provider, size).await?;
            }
        }

        Ok(())
    }

    // Requires the pinned box to be recursive
    fn initialize_surface_biome_chunk<'a>(
        &'a mut self,
        provider: &'a GDMCHTTPProvider,
        chunk: Rect2D,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let x = chunk.origin.x;
            let z = chunk.origin.y;

            let min_height = chunk.iter()
                .map(|point| self.get_height_at(point))
                .min()
                .unwrap();
            let max_height = chunk.iter()
                .map(|point| self.get_height_at(point))
                .max()
                .unwrap();

            // To avoid fetching too many biomes at once, we split the chunk into 4 sub-chunks if the height difference is too large
            if max_height - min_height > 16 {
                let half_x = chunk.size.x / 2;
                let half_y = chunk.size.y / 2;

                let sub_chunks = [
                    Rect2D::new(chunk.origin, Point2D::new(half_x, half_y)),
                    Rect2D::new(
                        Point2D::new(chunk.origin.x + half_x, chunk.origin.y),
                        Point2D::new(chunk.size.x - half_x, half_y),
                    ),
                    Rect2D::new(
                        Point2D::new(chunk.origin.x, chunk.origin.y + half_y),
                        Point2D::new(half_x, chunk.size.y - half_y),
                    ),
                    Rect2D::new(
                        Point2D::new(chunk.origin.x + half_x, chunk.origin.y + half_y),
                        Point2D::new(chunk.size.x - half_x, chunk.size.y - half_y),
                    ),
                ];

                for sub_chunk in sub_chunks.iter() {
                    if sub_chunk.size.x > 0 && sub_chunk.size.y > 0 {
                        self.initialize_surface_biome_chunk(provider, *sub_chunk).await?;
                    }
                }
                return Ok(());
            }

            let biomes: HashMap<Point3D, Biome> = provider
                .get_biomes(
                    x,
                    min_height,
                    z,
                    chunk.size.x,
                    max_height - min_height + 1,
                    chunk.size.y,
                )
                .await?
                .iter()
                .map(|positioned_biome| {
                    let biome = positioned_biome.id;
                    let point = Point3D::new(positioned_biome.x, positioned_biome.y, positioned_biome.z);
                    (point, biome)
                })
                .collect();

            for point in chunk.iter() {
                self.surface_biome_map[point.x as usize][point.y as usize] = biomes
                    .get(&Point3D::new(point.x, min_height, point.y))
                    .expect("This should have been here")
                    .clone();
            }

            Ok(())
        })
    }

    pub fn get_height_at(&self, point : Point2D) -> i32 {
        self.ground_height_map[point.x as usize][point.y as usize]
    }

    pub fn get_height_map(&self) -> &Vec<Vec<i32>> {
        &self.ground_height_map
    }   

    // Get height without counting water
    pub fn get_surface_height_at(&self, point : Point2D) -> i32 {
        self.surface_height_map[point.x as usize][point.y as usize]
    }

    pub fn get_motion_blocking_height_at(&self, point : Point2D) -> i32 {
        self.motion_blocking_height_map[point.x as usize][point.y as usize]
    }

    pub fn get_surface_biome_at(&self, point : Point2D) -> Biome {
        self.surface_biome_map[point.x as usize][point.y as usize]
    }

    pub fn get_district_at(&self, point : Point2D) -> Option<DistrictID> {
        self.district_map[point.x as usize][point.y as usize]
    }

    pub fn add_height(&mut self, point : Point2D) -> Point3D {
        Point3D::new(point.x, self.get_height_at(point), point.y)
    }

    pub fn is_in_bounds_2d(&self, point : Point2D) -> bool {
        self.build_area.drop_y().contains(point + self.build_area.origin.drop_y())
    }

    pub fn get_block(&self, mut point: Point3D) -> Option<Block> {
        point = point + self.build_area.origin;

        let chunk_coordinates = point_to_chunk_coordinates(point);

        let chunk = self.chunks.get(&chunk_coordinates.drop_y())?;

        let section = chunk.sections.iter().find(|s| s.y == (point.y / 16))?;

        let block_states = section.block_states.as_ref()?;

        if block_states.data.is_none() {
            let block = block_states.palette.get(0)?;
            return Some(Block {
                id: block.name.as_str().into(),
                states: block.properties.clone(),
                data: None,
            });
        }

        let data = block_states.data.as_ref()?;
        let index = (point.x % 16 + point.y % 16 * 16 + point.z % 16 * 256) as usize;

        let indices_per_long = 4096 / data.len();
        let bits = 64 / indices_per_long;
        let long_index = index / indices_per_long;
        let bit_index = index % indices_per_long;

        let long = data.get(long_index)?;
        let block_index = (long >> (bit_index * bits)) & ((1 << bits) - 1);
        let palette = &block_states.palette;

        palette.get(block_index as usize).map(|block| Block {
            id: block.name.as_str().into(),
            states: block.properties.clone(),
            data: None,
        })
    }
}