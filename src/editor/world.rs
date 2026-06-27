use std::collections::{HashMap, HashSet};

use anyhow::Ok;
use fastnbt::LongArray;
use log::info;

use crate::{generator::{build_claim::BuildClaim, buildings::BuildingData, districts::{Parcel, ParcelAnalysis, ParcelID, ParcelType, District, DistrictID}, nbts::StructureID}, geometry::{Cardinal, DOWN, Point2D, Point3D, Rect2D, Rect3D}, http_mod::{GDMCHTTPProvider, HeightMapType}, minecraft::{Biome, Block, Chunk, util::point_to_chunk_coordinates}};

use super::Editor;

const CHUNK_SIZE : i32 = 16;

/// Checked access into a `map[x][z]` 2D grid by a `Point2D`.
///
/// Returns `None` for any out-of-bounds coordinate — crucially including
/// *negative* ones, which a bare `point.x as usize` cast would otherwise wrap
/// into a huge index and panic on. This is the single choke point that makes
/// the World maps impossible to panic-index. Callers that sample a cell or two
/// past the build edge (road bands, search frontiers) get `None` and decide
/// what to do, rather than crashing the generator.
fn cell_2d<T>(map: &[Vec<T>], point: Point2D) -> Option<&T> {
    if point.x < 0 || point.y < 0 {
        log_oob(point);
        return None;
    }
    match map.get(point.x as usize).and_then(|col| col.get(point.y as usize)) {
        Some(cell) => Some(cell),
        None => {
            log_oob(point);
            None
        }
    }
}

/// Trace (not warn) on out-of-bounds map access. OOB is *expected* here —
/// `is_water` / road-band / search-frontier probes routinely sample a cell or
/// two past the build edge, so a louder level would flood every normal run and
/// bury the signal. At `RUST_LOG=trace` this leaves a coordinate trail so a
/// genuine bug (a wrong point that silently gets skipped instead of panicking)
/// can still be tracked down.
fn log_oob(point: Point2D) {
    log::trace!("World map access out of bounds at {:?}; returning None", point);
}

#[derive(Debug)]
pub struct World {
    pub build_area : Rect3D,
    pub parcels : HashMap<ParcelID, Parcel>,
    pub parcel_analysis_data : HashMap<ParcelID, ParcelAnalysis>,
    pub district_analysis_data : HashMap<DistrictID, ParcelAnalysis>,
    pub districts : HashMap<DistrictID, District>,
    pub parcel_map : Vec<Vec<Option<ParcelID>>>,
    pub district_map : Vec<Vec<Option<DistrictID>>>,
    pub buildings : Vec<BuildingData>,
    pub structures : Vec<StructureID>,
    pub gate_locations : Vec<(Point3D, Cardinal)>,
    /// Walkway guard posts grouped by wall tower: each inner Vec holds the
    /// walkable feet positions (walkway cell at surface + 1) just outside one
    /// tower's base, where a guard NPC can stand. Populated by `build_wall_towers`.
    pub tower_guard_posts : Vec<Vec<Point3D>>,
    /// Per wall tower: `(base_centre, support_y)` — the tower's 5×5 base centre
    /// (build-area local) and a height within its solid base ring. Used to hang
    /// the civic banner on the tower's outward face without reading blocks back
    /// (see `generator/civic_banner.rs`). Populated by `build_wall_towers`.
    pub tower_bases : Vec<(Point2D, i32)>,
    /// Regularized "inside the wall" cell set. When `Some`, `get_urban_points`
    /// returns it instead of the raw district union (see districts/footprint.rs).
    pub urban_footprint : Option<HashSet<Point2D>>,

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

        let parcel_map = vec![vec![None; size_z_usize]; size_x_usize];
        let district_map = vec![vec![None; size_z_usize]; size_x_usize];

        let chunk_rect = Rect3D {
            origin: point_to_chunk_coordinates(build_area.origin),
            size: point_to_chunk_coordinates(build_area.max()) - point_to_chunk_coordinates(build_area.origin) + Point3D::new(1, 1, 1),
        };
        info!("Chunk rect: {:?}", chunk_rect);
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
        let ground_block_map = vec![vec![Block::new(Default::default(), None, None); size_z_usize]; size_x_usize];
        let build_claim_map = vec![vec![BuildClaim::None; size_z_usize]; size_x_usize];
        let ground_biome_map = vec![vec![Biome::unknown(); size_z_usize]; size_x_usize];

        let mut world = World {
            build_area,
            parcels: HashMap::new(),
            districts: HashMap::new(),
            parcel_map,
            district_map,
            buildings: Vec::new(),
            structures: Vec::new(),
            gate_locations: Vec::new(),
            tower_guard_posts: Vec::new(),
            tower_bases: Vec::new(),
            urban_footprint: None,
            ground_height_map,
            ocean_floor_height_map,
            motion_blocking_height_map,
            build_claim_map,
            chunks,
            ground_biome_map,
            ground_block_map,
            parcel_analysis_data: HashMap::new(),
            district_analysis_data: HashMap::new(),
        };

        let y_offset = build_area.origin.y;
        for x in 0..size_x_usize {
            for z in 0..size_z_usize {
                world.ground_height_map[x][z] = ground_map[x][z] - y_offset;
                world.ocean_floor_height_map[x][z] = ocean_map[x][z] - y_offset;
                world.motion_blocking_height_map[x][z] = motion_blocking_map[x][z] - y_offset;
                // The heightmap value is the first *air* cell above the surface (see
                // `add_non_tree_height` / `analyze_parcel`, which read the surface block at
                // `height - 1`). Sample one block down so `ground_block_map` — and thus
                // `is_water` / `get_ground_block` — actually holds the surface block rather
                // than the air above it. Without the `-1`, `is_water` reported false over
                // open water, letting buildings be placed on water and backfilled.
                // Fall back to a sane default if the chunk/section isn't loaded
                // for this cell rather than aborting the whole world load.
                world.ground_block_map[x][z] = world
                    .get_block(Point3D::new(x as i32, world.ground_height_map[x][z] - 1, z as i32))
                    .unwrap_or_else(|| Block::new(Default::default(), None, None));
                world.ground_biome_map[x][z] = world
                    .get_biome(Point3D::new(x as i32, world.ocean_floor_height_map[x][z], z as i32))
                    .unwrap_or_else(Biome::unknown);
            }
        }

        Ok(world)
    }

    pub fn get_editor(self) -> Editor {
        Editor::new(self.build_area, self)
    }

    /// Build a synthetic World for offline / dry-run use. No HTTP calls.
    /// Ground is flat at `ground_y` (absolute world Y). Biome = Plains, surface
    /// block = grass. Chunks are empty — `get_block` will return None for any
    /// point, so callers must either guard against missing blocks or use the
    /// editor's block cache.
    pub fn synthetic(build_area: Rect3D, ground_y: i32) -> Self {
        let size_x_usize = build_area.size.x as usize;
        let size_z_usize = build_area.size.z as usize;

        // Heightmaps are stored relative to build_area.origin.y (see World::new).
        let y_local = ground_y - build_area.origin.y;

        let ground_height_map = vec![vec![y_local; size_z_usize]; size_x_usize];
        let ocean_floor_height_map = vec![vec![y_local; size_z_usize]; size_x_usize];
        let motion_blocking_height_map = vec![vec![y_local; size_z_usize]; size_x_usize];
        let ground_block_map = vec![vec![Block::new("minecraft:grass_block".into(), None, None); size_z_usize]; size_x_usize];
        let ground_biome_map = vec![vec![Biome::unknown(); size_z_usize]; size_x_usize];
        let build_claim_map = vec![vec![BuildClaim::None; size_z_usize]; size_x_usize];
        let parcel_map = vec![vec![None; size_z_usize]; size_x_usize];
        let district_map = vec![vec![None; size_z_usize]; size_x_usize];

        World {
            build_area,
            parcels: HashMap::new(),
            parcel_analysis_data: HashMap::new(),
            districts: HashMap::new(),
            district_analysis_data: HashMap::new(),
            parcel_map,
            district_map,
            buildings: Vec::new(),
            structures: Vec::new(),
            gate_locations: Vec::new(),
            tower_guard_posts: Vec::new(),
            tower_bases: Vec::new(),
            urban_footprint: None,
            ground_height_map,
            ground_block_map,
            ocean_floor_height_map,
            ground_biome_map,
            motion_blocking_height_map,
            build_claim_map,
            chunks: HashMap::new(),
        }
    }

    /// Build an offline editor from a synthetic world. Skips all HTTP traffic —
    /// blocks are written to the editor's in-memory cache only. See
    /// `Editor::new_offline` for the editor-side behavior.
    pub fn get_offline_editor(self) -> Editor {
        Editor::new_offline(self.build_area, self)
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

    pub fn get_height_at(&self, point : Point2D) -> Option<i32> {
        cell_2d(&self.ground_height_map, point).copied()
    }

    pub fn get_non_tree_height(&self, point : Point2D) -> Option<i32> {
        let mut height = self.get_height_at(point)?;
        // Walk down through tree blocks to the real surface. If a block read
        // fails (unloaded chunk / out of section range), stop where we are
        // rather than panicking — the height so far is the best estimate.
        while let Some(block) = self.get_block(Point3D::new(point.x, height - 1, point.y)) {
            if !block.id.is_tree() {
                break;
            }
            height -= 1;
        }
        Some(height)
    }

    pub fn get_height_map(&self) -> &Vec<Vec<i32>> {
        &self.ground_height_map
    }

    pub fn get_ground_block_map(&self) -> &Vec<Vec<Block>> {
        &self.ground_block_map
    }

    pub fn get_ground_biome_map(&self) -> &Vec<Vec<Biome>> {
        &self.ground_biome_map
    }

    pub fn get_ocean_floor_height_map(&self) -> &Vec<Vec<i32>> {
        &self.ocean_floor_height_map
    }

    pub fn get_build_claim_map(&self) -> &Vec<Vec<BuildClaim>> {
        &self.build_claim_map
    }

    pub fn set_heights(&mut self, points : &HashSet<Point3D>) {
        let xlen = self.ground_height_map.len() as i32;
        let zlen = self.ground_height_map.first().map_or(0, |col| col.len()) as i32;
        for point in points {
            // Defensive: terrain ops can hand us a cell one past the build-area
            // edge, and these maps have no bounds check (see the is-water OOB
            // gotcha). Those cells aren't ours to flatten, so skip rather than
            // panic.
            if point.x < 0 || point.z < 0 || point.x >= xlen || point.z >= zlen {
                continue;
            }
            self.ground_height_map[point.x as usize][point.z as usize] = point.y;
            self.ocean_floor_height_map[point.x as usize][point.z as usize] = point.y;
        }
    }

    /// Record a column whose liquid was drained and back-filled with solid: the
    /// new surface is solid `block` capped at `surface_air - 1`, with the first
    /// air at `surface_air`. Updates ALL height maps *and* the cached
    /// `ground_block_map` so `is_water` / `get_ground_block` no longer report the
    /// (now removed) liquid here — `set_heights` alone leaves the block cache
    /// stale and `is_water` would keep returning true. No-op out of bounds.
    pub fn set_drained_surface(&mut self, point : Point2D, surface_air : i32, block : Block) {
        if !self.is_in_bounds_2d(point) {
            return;
        }
        let (x, z) = (point.x as usize, point.y as usize);
        self.ground_height_map[x][z] = surface_air;
        self.ocean_floor_height_map[x][z] = surface_air;
        self.motion_blocking_height_map[x][z] = surface_air;
        self.ground_block_map[x][z] = block;
    }

    /// Test-only: mark a surface cell as water so `is_water` reports it. Used by the
    /// footprint terrain-clip tests on a synthetic world.
    #[cfg(test)]
    pub fn set_water_for_test(&mut self, point : Point2D) {
        self.ground_block_map[point.x as usize][point.y as usize] =
            Block::new("minecraft:water".into(), None, None);
    }

    // Get height without counting water
    pub fn get_ocean_floor_height_at(&self, point : Point2D) -> Option<i32> {
        cell_2d(&self.ocean_floor_height_map, point).copied()
    }

    pub fn get_motion_blocking_height_at(&self, point : Point2D) -> Option<i32> {
        cell_2d(&self.motion_blocking_height_map, point).copied()
    }

    pub fn get_surface_biome_at(&self, point : Point2D) -> Option<Biome> {
        let height = self.get_ocean_floor_height_at(point)?;
        let point_3d = Point3D::new(point.x, height, point.y);
        self.get_biome(point_3d)
    }

    pub fn get_parcel_at(&self, point : Point2D) -> Option<ParcelID> {
        cell_2d(&self.parcel_map, point).copied().flatten()
    }

    pub fn get_district_at(&self, point : Point2D) -> Option<DistrictID> {
        cell_2d(&self.district_map, point).copied().flatten()
    }

    pub fn add_height(&self, point : Point2D) -> Option<Point3D> {
        Some(Point3D::new(point.x, self.get_height_at(point)?, point.y))
    }

    pub fn add_non_tree_height(&self, point : Point2D) -> Option<Point3D> {
        let mut new_point = Point3D::new(point.x, self.get_height_at(point)?, point.y);
        // Walk down through tree blocks; stop on a failed read rather than panic.
        while let Some(block) = self.get_block(new_point + DOWN) {
            if !block.id.is_tree() {
                break;
            }
            new_point += DOWN;
        }
        Some(new_point)
    }

    pub fn is_in_bounds_2d(&self, point : Point2D) -> bool {
        self.build_area.drop_y().contains(point + self.build_area.origin.drop_y())
    }

    pub fn get_block(&self, mut point: Point3D) -> Option<Block> {
        point = point + self.build_area.origin;
        //info!("Getting block at point: {:?}", point); uncomment if needed, but generates way to many lines of logs

        let chunk_coordinates = point_to_chunk_coordinates(point);
        //info!("Chunk coordinates: {:?}", chunk_coordinates);

        let chunk = self.chunks.get(&chunk_coordinates.drop_y())?;
        //info!("Found chunk: {:?}", chunk);

        let section = chunk.sections.iter().find(|s| s.y == chunk_coordinates.y)?;
        //info!("Found section: {:?}", section);

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

    pub fn get_ground_block(&self, point: Point2D) -> Option<&Block> {
        cell_2d(&self.ground_block_map, point)
    }

    pub fn is_water(&self, point : Point2D) -> bool {
        // Out of bounds is dry land, not a panic: callers that sample around a
        // road band / search frontier routinely probe a cell or two past the
        // build edge.
        cell_2d(&self.ground_block_map, point).map_or(false, |b| b.id.is_water())
    }

    pub fn is_water_3d(&self, point : Point3D) -> bool {
        // Missing block (unloaded chunk / out of range) is treated as not water.
        self.get_block(point).map_or(false, |b| b.id.is_water())
    }

    pub fn is_claimed(&self, point : Point2D) -> bool {
        // Out of bounds cells are unclaimable, so treat them as unclaimed.
        cell_2d(&self.build_claim_map, point).map_or(false, |c| *c != BuildClaim::None)
    }

    pub fn claim(&mut self, point: Point2D, claim: BuildClaim) {
        if self.is_in_bounds_2d(point) {
            self.build_claim_map[point.x as usize][point.y as usize] = claim;
        } else {
            log::warn!("Tried to claim point {:?} out of bounds", point);
        }
    }

    pub fn get_claim(&self, point : Point2D) -> Option<BuildClaim> {
        if self.is_in_bounds_2d(point) {
            Some(self.build_claim_map[point.x as usize][point.y as usize].clone())
        } else {
            None
        }
    }

    /// The "inside the wall" cell set. Once the urban footprint has been
    /// regularized (see districts/footprint.rs) this returns that footprint;
    /// otherwise it falls back to the raw union of `Urban`-classified districts.
    pub fn get_urban_points(&self) -> HashSet<Point2D> {
        if let Some(footprint) = &self.urban_footprint {
            return footprint.clone();
        }
        self.iter_points_2d()
            .filter(|&point| self.get_parcel_type(point) == Some(ParcelType::Urban))
            .collect()
    }

    /// Whether `point` is inside the urban area / city wall. Uses the regularized
    /// footprint when present, else the district classification.
    pub fn is_urban(&self, point: Point2D) -> bool {
        match &self.urban_footprint {
            Some(footprint) => footprint.contains(&point),
            None => self.get_parcel_type(point) == Some(ParcelType::Urban),
        }
    }

    pub fn get_parcel_type(&self, point: Point2D) -> Option<ParcelType> {
        self.get_district_at(point).and_then(|parcel_id| {
            self.districts.get(&parcel_id).map(|parcel| parcel.data.parcel_type)
        })
    }

    pub fn get_urban_parcels(&self) -> Vec<&Parcel> {
        let World { parcels, districts, district_map, .. } = self;

        parcels.values()
            .filter(|parcel| {
                let origin = parcel.data.origin.drop_y();
                let district_id = district_map[origin.x as usize][origin.y as usize];
                if let Some(district_id) = district_id {
                    if let Some(district) = districts.get(&district_id) {
                        return district.data.parcel_type == ParcelType::Urban;
                    }
                }

                false
            })
            .collect()
    }
}