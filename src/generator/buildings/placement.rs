use std::{collections::{HashMap, HashSet}, i32};

use crate::{editor::Editor, generator::{buildings::{build_floor, build_stairs, constants::{BUILDING_GROUND_DIG_COST, BUILDING_GROUND_RAISE_COST, BUILDING_MAX_AVERAGE_GROUND_COST}, roofs::build_roof, set::{BuildingSet, BuildingSetID}, shape::BuildingShape, walls::build_walls, BuildingData, Grid}, data::LoadedData, districts::{replace_ground_smooth, DistrictType, HasDistrictData}, materials::PaletteId, nbts::{Rotation, Transform}, paths::PathType, style::Style, BuildClaim}, geometry::{get_outer_and_inner_points, Point2D}, minecraft::{Block, BlockID}, noise::RNG};

use super::BuildingID;

pub fn get_city_blocks_and_off_limits(editor : &mut Editor, rng : &mut RNG) -> (Vec<HashSet<Point2D>>, HashSet<Point2D>) {
    let mut points = editor.world().get_urban_points();
    let off_limits : HashSet<Point2D> = points.iter()
        .filter(|point| {
            editor.world().gate_locations.iter().any(|(gate_point, _)| gate_point.drop_y().distance_manhattan(point) < 10)
        })
        .cloned()
        .collect();

    for point in off_limits.iter() {
        points.remove(point);
    }

    let num_city_blocks = points.len() / 500;

    let mut  points_vec = points.iter().cloned().collect::<Vec<_>>();
    let mut spawn_points = HashSet::new();

    for _ in 0..num_city_blocks {
        if points.is_empty() {
            break;
        }

        let point = rng.choose::<Point2D>(&points_vec).clone();
        spawn_points.insert(point);
        points.remove(&point);
        points_vec.retain(|p| *p != point);
    }

    let mut city_blocks = spawn_points.iter()
        .map(|point| {
            let mut block = HashSet::new();
            block.insert(*point);
            block
        })
        .collect::<Vec<_>>();
    let mut visited = spawn_points.iter().cloned().collect::<HashSet<_>>();
    let mut queue = spawn_points.iter().enumerate().map(|(i, p)| (i, *p)).collect::<Vec<_>>();

    while queue.len() > 0 {
        let (index, point) = queue.remove(0);

        for neighbour in point.neighbours() {
            if !editor.world().is_in_bounds_2d(neighbour) || visited.contains(&neighbour) || !points.contains(&neighbour) {
                continue;
            }

            city_blocks[index].insert(neighbour);
            visited.insert(neighbour);
            queue.push((index, neighbour));
        }
    }

    (city_blocks, off_limits)
}

pub async fn place_buildings_in_area(editor : &mut Editor, rng : &mut RNG, data : &LoadedData, style : Style, palette : &PaletteId) {
    let mut outers : HashSet<Point2D> = HashSet::new();
    let mut inners : Vec<HashSet<Point2D>> = vec![];

    let (city_blocks, off_limits) = get_city_blocks_and_off_limits(editor, rng);

    for point in off_limits {
        outers.insert(point);
    }

    for block in city_blocks {
        let (outer, inner) = get_outer_and_inner_points(&block, 3);
        outers.extend(outer);
        inners.push(inner);
    }
    

    use BlockID::*;
    let block_vec : Vec<Block> = vec![
        Stone, Cobblestone, StoneBricks, Andesite, Gravel,
        StoneStairs, CobblestoneStairs, StoneBrickStairs, AndesiteStairs,
        StoneSlab, CobblestoneSlab, StoneBrickSlab, AndesiteSlab,
    ].into_iter().map(|id| Block { id, data: None, state: None }).collect();

    let mut blocks_dict: HashMap<u32, HashMap<u32, f32>> = HashMap::new();

    let block_dict = [
        (0, 3.0),  // Stone
        (1, 2.0),  // Cobblestone
        (2, 8.0),  // Stone Bricks
        (3, 3.0),  // Andesite
        (4, 1.0),  // Gravel
    ].into_iter().collect();
    blocks_dict.insert(0, block_dict);

    let stair_dict = [
        (5, 3.0),  // Stone stairs
        (6, 2.0),  // Cobblestone stairs
        (7, 8.0),  // Stone Bricks stairs
        (8, 4.0),  // Andesite stairs
    ].into_iter().collect();
    blocks_dict.insert(1, stair_dict);

    let slab_dict = [
        (9, 3.0),   // Stone slab
        (10, 2.0),  // Cobblestone slab
        (11, 8.0),  // Stone Bricks slab
        (12, 4.0),  // Andesite slab
    ].into_iter().collect();
    blocks_dict.insert(2, slab_dict);

    replace_ground_smooth(
        &outers,
        &blocks_dict,
        &block_vec,
        rng,
        editor,
        Some(0),
        None, // No permit blocks
        Some(false), // Ignore water
    ).await;
}

pub fn get_best_height_if_placeable(editor : &mut Editor, shape : &BuildingShape, grid : &Grid) -> Option<i32> {
    let footprint = shape.get_footprint(&grid);

    for point in footprint.iter() {
        if editor.world().is_claimed(*point) {
            // If any point in the footprint is already claimed, we cannot place the building here
            return None;
        }
    }

    let (height, score) = get_best_height_and_score(editor, &footprint);

    if score < BUILDING_MAX_AVERAGE_GROUND_COST {
        Some(height)
    } else {
        None
    }
}

pub fn get_best_height_and_score(editor : &mut Editor, footprint : &HashSet<Point2D>) -> (i32, f32) {
    let min_height = footprint.iter()
        .map(|point| editor.world().get_height_at(*point))
        .min()
        .unwrap_or(0);

    let max_height = footprint.iter()
        .map(|point| editor.world().get_height_at(*point))
        .max()
        .unwrap_or(0);

    let mut best_score = f32::MAX;
    let mut best_height = min_height;

    for height in min_height..=max_height {
        let score : f32 = footprint.iter()
            .map(|point| {
                let height_at_point = editor.world().get_height_at(*point);
                let diff = height_at_point - height;

                if diff < 0 {
                    (diff.abs() as f32) * BUILDING_GROUND_DIG_COST
                } else {
                    (diff as f32) * BUILDING_GROUND_RAISE_COST // Prefer lower heights
                }
            })
            .sum();

        if score < best_score {
            best_score = score;
            best_height = height;
        }
    }

    let avg_score = (best_score as f32) / (footprint.len() as f32);

    (best_height, avg_score)
}

pub async fn place_building(editor : &mut Editor, shape : &BuildingShape, grid : Grid, set : &BuildingSetID, data : LoadedData, style : Style, rng : &RNG, palette : &PaletteId) {
    let mut building = BuildingData {
        id: BuildingID(editor.world_mut().buildings.len()),
        grid,
        shape: shape.clone(),
        palette: palette.clone(),
        style,
    };
    
    for point in building.shape.get_footprint(&building.grid) {
        editor.world_mut().claim(point, BuildClaim::Building(building.id));
    }

    let mut rng = RNG::new(100);

    let set = data.building_sets.get(set).expect("Building set not found");

    let roof_set = rng.choose(&set.roof_sets);
    let wall_set = rng.choose(&set.wall_sets);

    for cell in building.shape.cells().iter() {
        for point in grid.get_cell_rect(*cell).iter() {
            editor.place_block_forced(&BlockID::Air.into(), point).await;
        }
    }

    build_walls(editor, wall_set, &mut building, &data, &mut rng).await.expect("Failed to build walls");
    build_roof(editor, &data, &mut building, roof_set, &mut rng).await.expect("Failed to build roof");        
    build_floor(editor, &data, &mut building, &mut rng).await;
    build_stairs(editor, &mut building, &data, &mut rng).await;

    editor.world_mut().buildings.push(building);
}