use crate::editor::World;
use crate::generator::build_claim::BuildClaim;

use super::types::*;

pub fn extract_status(world: &World, phase: &GenerationPhase) -> StatusResponse {
    StatusResponse {
        phase: phase.clone(),
        width: world.size().x as usize,
        depth: world.size().z as usize,
        origin_x: world.origin().x,
        origin_z: world.origin().z,
        error: None,
    }
}

pub fn extract_heightmap(world: &World) -> HeightmapData {
    let width = world.size().x as usize;
    let depth = world.size().z as usize;
    let height_map = world.get_height_map();

    let mut heights = vec![0i32; width * depth];
    let mut min_height = i32::MAX;
    let mut max_height = i32::MIN;

    for x in 0..width {
        for z in 0..depth {
            let h = height_map[x][z];
            heights[x * depth + z] = h;
            if h < min_height {
                min_height = h;
            }
            if h > max_height {
                max_height = h;
            }
        }
    }

    HeightmapData {
        width,
        depth,
        heights,
        min_height,
        max_height,
    }
}

pub fn extract_biomes(world: &World) -> BiomeMapData {
    let width = world.size().x as usize;
    let depth = world.size().z as usize;
    let biome_map = world.get_ground_biome_map();

    let mut biomes = Vec::with_capacity(width * depth);
    for x in 0..width {
        for z in 0..depth {
            biomes.push(biome_map[x][z].name().to_string());
        }
    }

    BiomeMapData {
        width,
        depth,
        biomes,
    }
}

pub fn extract_parcels(world: &World) -> ParcelMapData {
    let width = world.size().x as usize;
    let depth = world.size().z as usize;

    let mut parcels = vec![-1i32; width * depth];
    let mut districts = vec![-1i32; width * depth];
    let mut parcel_types = vec![String::new(); width * depth];

    for x in 0..width {
        for z in 0..depth {
            if let Some(did) = world.parcel_map[x][z] {
                parcels[x * depth + z] = did.0 as i32;
            }
            if let Some(sid) = world.district_map[x][z] {
                districts[x * depth + z] = sid.0 as i32;
                if let Some(sd) = world.districts.get(&sid) {
                    parcel_types[x * depth + z] = format!("{:?}", sd.data.parcel_type);
                }
            }
        }
    }

    let mut parcel_info: Vec<ParcelInfo> = world
        .parcels
        .values()
        .map(|d| {
            let dtype = world
                .district_map
                .get(d.data.origin.x as usize)
                .and_then(|row| row.get(d.data.origin.z as usize))
                .and_then(|sid| sid.as_ref())
                .and_then(|sid| world.districts.get(sid))
                .map(|sd| format!("{:?}", sd.data.parcel_type))
                .unwrap_or_else(|| "Unknown".to_string());

            ParcelInfo {
                id: d.id.0,
                parcel_type: dtype,
                is_border: d.data.is_border,
                size: d.data.points_2d.len(),
                origin_x: d.data.origin.x,
                origin_z: d.data.origin.z,
            }
        })
        .collect();
    parcel_info.sort_by_key(|d| d.id);

    ParcelMapData {
        width,
        depth,
        parcels,
        districts,
        parcel_types,
        parcel_info,
    }
}

pub fn extract_buildings(world: &World) -> BuildingsData {
    let buildings = world
        .buildings
        .iter()
        .map(|b| {
            let footprint: Vec<[i32; 2]> = b
                .shape
                .get_footprint(&b.grid)
                .into_iter()
                .map(|p| [p.x, p.y])
                .collect();

            BuildingInfo {
                id: b.id.0,
                origin_x: b.grid.origin.x,
                origin_y: b.grid.origin.y,
                origin_z: b.grid.origin.z,
                footprint,
            }
        })
        .collect();

    BuildingsData { buildings }
}

pub fn extract_claims(world: &World) -> ClaimMapData {
    let width = world.size().x as usize;
    let depth = world.size().z as usize;
    let claim_map = world.get_build_claim_map();

    let mut claims = Vec::with_capacity(width * depth);
    for x in 0..width {
        for z in 0..depth {
            let claim_str = match &claim_map[x][z] {
                BuildClaim::None => "none",
                BuildClaim::Nature => "nature",
                BuildClaim::Wall => "wall",
                BuildClaim::Gate => "gate",
                BuildClaim::Path(_) => "path",
                BuildClaim::PathPlanned(_) => "path",
                BuildClaim::Building(_) => "building",
                BuildClaim::Structure(_) => "structure",
            };
            claims.push(claim_str.to_string());
        }
    }

    ClaimMapData {
        width,
        depth,
        claims,
    }
}

pub fn extract_blocks(world: &World) -> BlockMapData {
    let width = world.size().x as usize;
    let depth = world.size().z as usize;
    // height_map: MOTION_BLOCKING — the highest non-air block, including water surfaces
    // ocean_floor: OCEAN_FLOOR — the highest solid block, ignoring water
    // When the water surface is above the ocean floor, the cell is submerged.
    let surface_map = world.get_height_map();
    let ocean_floor_map = world.get_ocean_floor_height_map();

    let mut blocks = Vec::with_capacity(width * depth);
    for x in 0..width {
        for z in 0..depth {
            let surface_y = surface_map[x][z];
            let floor_y = ocean_floor_map[x][z];
            let is_water = surface_y > floor_y;

            if is_water {
                blocks.push("water".to_string());
            } else {
                // Read the block at ground level; if air/plant, try one below
                let block = world.get_block(crate::geometry::Point3D::new(x as i32, surface_y, z as i32));
                let block_id = block.as_ref().map(|b| b.id.as_str()).unwrap_or("air");

                // Strip minecraft: prefix for consistent lookup
                let id = block_id.strip_prefix("minecraft:").unwrap_or(block_id);

                if id == "air" || id == "cave_air" || id.contains("grass") || id.contains("flower")
                    || id.contains("tulip") || id.contains("daisy") || id.contains("bush")
                    || id.contains("dandelion") || id.contains("poppy") || id.contains("bluet")
                    || id.contains("fern") || id.contains("orchid") || id.contains("allium")
                    || id.contains("cornflower") || id == "dead_bush" || id == "sugar_cane"
                {
                    // Non-solid surface block — read one below for the actual ground
                    let below = world.get_block(crate::geometry::Point3D::new(x as i32, surface_y - 1, z as i32));
                    blocks.push(below.map(|b| b.id.as_str().to_string()).unwrap_or_else(|| id.to_string()));
                } else {
                    blocks.push(block_id.to_string());
                }
            }
        }
    }

    BlockMapData {
        width,
        depth,
        blocks,
    }
}

pub fn extract_full_snapshot(world: &World, phase: &GenerationPhase) -> WorldSnapshot {
    let width = world.size().x as usize;
    let depth = world.size().z as usize;

    WorldSnapshot {
        phase: phase.clone(),
        width,
        depth,
        origin_x: world.origin().x,
        origin_z: world.origin().z,
        heightmap: Some(extract_heightmap(world)),
        blocks: Some(extract_blocks(world)),
        biomes: Some(extract_biomes(world)),
        parcels: Some(extract_parcels(world)),
        buildings: Some(extract_buildings(world)),
        claims: Some(extract_claims(world)),
    }
}
