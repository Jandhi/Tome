use crate::{generator::districts::DistrictID, geometry::{Point2D, Point3D, Rect3D}, http_mod::{GDMCHTTPProvider, HeightMapType}};

#[derive(Debug, Clone)]
pub struct World {
    pub build_area : Rect3D,
    pub district_map : Vec<Vec<Option<DistrictID>>>,
    ground_height_map : Vec<Vec<i32>>,
    surface_height_map : Vec<Vec<i32>>,
}

impl World {
    pub fn new() -> Self {
        World {
            build_area: Rect3D::default(),
            district_map: vec![vec![None; 0]; 0],
            ground_height_map: vec![vec![0; 0]; 0],
            surface_height_map: vec![vec![0; 0]; 0],
        }
    }

    pub async fn init(&mut self, provider : &GDMCHTTPProvider) -> anyhow::Result<()> {
        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let ground_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::MotionBlockingNoPlants).await?;
        let ocean_map = provider.get_heightmap(build_area.origin.x, build_area.origin.z, build_area.size.x, build_area.size.z, HeightMapType::OceanFloorNoPlants).await?;
        self.district_map = vec![vec![None; build_area.size.z as usize]; build_area.size.x as usize];
        
        self.ground_height_map = vec![vec![0; build_area.size.z as usize]; build_area.size.x as usize];
        self.surface_height_map = vec![vec![0; build_area.size.z as usize]; build_area.size.x as usize];

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                self.ground_height_map[x as usize][z as usize] = ground_map[x as usize][z as usize] - build_area.origin.y;
                self.surface_height_map[x as usize][z as usize] = ocean_map[x as usize][z as usize] - build_area.origin.y;
            }
        }

        Ok(())
    }

    pub fn get_height_at(&self, point : Point2D) -> i32 {
        self.ground_height_map[point.x as usize][point.y as usize]
    }   

    // Get height without counting water
    pub fn get_surface_height_at(&self, point : Point2D) -> i32 {
        self.surface_height_map[point.x as usize][point.y as usize]
    }

    pub fn add_height(&mut self, point : Point2D) -> Point3D {
        Point3D::new(point.x, self.get_height_at(point), point.y)
    }
}