use std::{collections::HashMap, future::Future, pin::Pin};

use anyhow::Ok;
use log::info;

use crate::{generator::districts::{District, DistrictID, SuperDistrictID}, geometry::{Point2D, Point3D, Rect2D, Rect3D}, http_mod::{GDMCHTTPProvider, HeightMapType}, minecraft::Biome};

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
}

impl World {
    pub async fn new(provider : &GDMCHTTPProvider) -> anyhow::Result<Self> {
        let mut world = World {
            build_area: Rect3D::default(),
            districts: HashMap::new(),
            district_map: vec![vec![None; 0]; 0],
            super_district_map: vec![vec![None; 0]; 0],
            ground_height_map: vec![vec![0; 0]; 0],
            surface_height_map: vec![vec![0; 0]; 0],
            motion_blocking_height_map: vec![vec![0; 0]; 0],
            surface_biome_map: vec![vec![Biome::Unknown; 0]; 0],
        };

        world.init(provider).await?;
        Ok(world)
    }
    // TODO: World initialization should be in new
    async fn init(&mut self, provider : &GDMCHTTPProvider) -> anyhow::Result<()> {
        self.build_area = provider.get_build_area().await.expect("Failed to get build area");
        self.district_map = vec![vec![None; self.build_area.size.z as usize]; self.build_area.size.x as usize];
        
        let (origin_x, origin_z, size_x, size_z) = (
            self.build_area.origin.x,
            self.build_area.origin.z,
            self.build_area.size.x,
            self.build_area.size.z,
        );

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

        self.ground_height_map = vec![vec![0; size_z_usize]; size_x_usize];
        self.surface_height_map = vec![vec![0; size_z_usize]; size_x_usize];
        self.motion_blocking_height_map = vec![vec![0; size_z_usize]; size_x_usize];

        for x in 0..size_x_usize {
            for z in 0..size_z_usize {
            let y_offset = self.build_area.origin.y;
            self.ground_height_map[x][z] = ground_map[x][z] - y_offset;
            self.surface_height_map[x][z] = ocean_map[x][z] - y_offset;
            self.motion_blocking_height_map[x][z] = motion_blocking_map[x][z] - y_offset;
            }
        }

        self.init_surface_biome_map(provider).await?;

        Ok(())
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
}