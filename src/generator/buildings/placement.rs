use std::{cell::RefCell, collections::{HashMap, HashSet}, i32};

use strum::IntoEnumIterator;

use crate::{editor::Editor, generator::{BuildClaim, buildings::{BuildingData, Grid, build_floor, build_stairs, foundation::build_foundation, grid::DEFAULT_GRID_CELL_SIZE, roofs::build_roof, set::BuildingSetID, shape::BuildingShape, walls::build_walls}, chronicle::SettlementInfo, data::LoadedData, districts::{DistrictID, replace_ground_smooth}, materials::{MaterialId, MaterialRole, Palette, PaletteId}, nbts::Rotation, style::{DistrictStyle, Style}, terrain::force_height}, geometry::{ Cardinal, Point2D, UP, average_to_neighbours_5_away, get_edge, get_ordered_edge, get_outer_and_inner_points, voronoi_fill_with_recenter}, minecraft::{Biome, BiomeStonetype, BiomeWoodtype, Block}, noise::RNG};

use super::BuildingID;

pub fn get_city_blocks_and_off_limits(editor : &mut Editor, rng : &mut RNG) -> (Vec<HashSet<Point2D>>, HashSet<Point2D>) {
    let points = editor.world().get_urban_points();
    let off_limits : HashSet<Point2D> = points.iter()
        .filter(|point| {
            editor.world().gate_locations.iter().any(|(gate_point, _)| gate_point.drop_y().distance_manhattan(point) < 10)
        })
        .cloned()
        .collect();

    let points = points.difference(&off_limits).cloned().collect::<HashSet<_>>();
    let sections = points.len() / 1500 + 1;

    let city_blocks = voronoi_fill_with_recenter(
        &points, 
        &|point| { point.neighbours() },
         &|set| { 
            let average = set.iter().fold(Point2D::ZERO, |p1, p2| p1 + *p2) / set.len() as i32;

            if !points.contains(&average) {
                return set.iter().min_by_key(|p| p.distance_manhattan(&average)).expect("Set should not be empty").clone();
            }

            average
        }, 
         rng, sections, 3);

    (city_blocks, off_limits)
}

pub async fn place_buildings(editor : &mut Editor, rng : &mut RNG, data : &LoadedData, style : Style, core_palettes : Vec<&PaletteId>, settlement_info : &SettlementInfo) {
    let mut outers : HashSet<Point2D> = HashSet::new();
    let mut inners : Vec<HashSet<Point2D>> = vec![];
    
    let (city_blocks, off_limits) = get_city_blocks_and_off_limits(editor, rng);
    
    for point in off_limits {
        outers.insert(point);
    }
    
    let mut block_districts = vec![];
    for block in city_blocks.iter() {
        let (outer, inner) = get_outer_and_inner_points(&block, 3);
        outers.extend(outer);
        
        block_districts.push(inner.iter().fold(HashMap::new(), |mut acc : HashMap::<DistrictID, usize>, point| {
            let district = editor.world().get_district_at(*point);
            
            if let Some(parcel) = district {
                acc.entry(parcel).and_modify(|e| *e += 1).or_insert(1);
            }
            
            acc
        }).iter().max_by_key(|(_, count)| *count).map(|(parcel, _)| *parcel).unwrap());

        inners.push(inner);
    }

    let data = RefCell::new(data);
    let flowers : MaterialId = (*rng.choose(&vec![
        "cold_flowers",
        "warm_flowers",
        "tulips",
    ])).into();
    let cores = core_palettes.iter().map(|id| {
        let mut palette = data.borrow().palettes.get(id).expect("Core palette not found").clone();
        palette.materials.insert(MaterialRole::Flower, flowers.clone());
        palette
    }).collect::<Vec<_>>();
    let roofs = rng.choose_many(&vec![
        "acacia_wood_roof".into(),
        "brick_roof".into(),
        "oak_wood_roof".into(),
        "red_wood_roof".into(),
        "blackstone_roof".into(),
        "blue_wood_roof".into(),
    ], 3).iter().map(|id| data.borrow().palettes.get(id).expect("Roof palette not found")).collect::<Vec<_>>();
    
    

    let mut district_styles : HashMap<DistrictID, DistrictStyle> = HashMap::new();
    for district_id in block_districts.iter() {
        if district_styles.contains_key(&*district_id) {
            continue; // Already have a style for this district
        }

        let district_data = editor.world().district_analysis_data.get(district_id)
            .expect("Super parcel analysis data not found");

        

        let woods = district_data.biome_count().keys().into_iter()
            .map(|biome| {
                log::info!("Processing biome: {:?}", biome);
                BiomeWoodtype::from_biome(biome.clone())
                    .map(|wood_type| 
                        data.borrow().palettes.get(&wood_type.get_wood_palette_id())
                            .expect("Wood palette not found")
                    )}
                )
            .filter_map(|wood| wood)
            .collect::<Vec<_>>();

        let stones = district_data.biome_count().into_iter()
            .max_by_key(|item| item.1)
            .map(|(biome, _)| 
                BiomeStonetype::from_biome(biome.clone())
                    .into_iter()
                    .map(|stone_type| 
                        stone_type.get_stone_palette_ids().iter().map(|id| 
                            data.borrow().palettes.get(id)
                                .expect("Stone palette not found")
                        ).collect::<Vec<_>>()
                    )
                    .flatten()
                    .collect::<Vec<_>>()
            )
            .unwrap();

        district_styles.insert(*district_id, DistrictStyle::generate_style(rng, cores.iter().collect(), roofs.clone(), woods.clone(), stones.clone()));
    }
    
    let urban_area_edge = get_edge(&editor.world().get_urban_points());
    smooth_and_pave_road(editor, rng, &outers.difference(&urban_area_edge).cloned().collect(), PavingType::from_biome(settlement_info.top_three_biomes[0].clone())).await;

    let sets = data.borrow().building_sets.iter().filter(|(_, set)| {
        set.style == style
    }).map(|(id, _)| id.clone()).collect::<Vec<_>>();

    for (block_index, block) in inners.iter().enumerate() {
        for point in get_ordered_edge(&block) {
            for direction in Cardinal::iter() {
                if !outers.contains(&(point + direction.into())) {
                    continue;
                }

                let door_position = point + match direction {
                    Cardinal::South => Point2D { x: DEFAULT_GRID_CELL_SIZE.x / 2, y: 1 - DEFAULT_GRID_CELL_SIZE.z },
                    Cardinal::West => Point2D { x: 0, y: DEFAULT_GRID_CELL_SIZE.z / 2 },
                    Cardinal::East => Point2D { x: 1 - DEFAULT_GRID_CELL_SIZE.x, y: 1 - DEFAULT_GRID_CELL_SIZE.z - DEFAULT_GRID_CELL_SIZE.z / 2 },
                    Cardinal::North => Point2D { x: -DEFAULT_GRID_CELL_SIZE.x / 2, y: 0 },
                };
                let mut height_point = door_position;
                while block.contains(&height_point) {
                    height_point += direction.into();
                }

                // if distance > 5 { // The door is too far from the road, height will be awkward
                //     continue;
                // }

                let y = editor.world().get_height_at(height_point);

                if y.abs_diff(editor.world().get_height_at(point)) > 3 {
                    continue; // Skip if the height difference is too large, this is probably indicative of a bad spot to place
                }

                let grid = Grid::new((point + match direction {
                    Cardinal::North => Point2D { x: 1 - DEFAULT_GRID_CELL_SIZE.x, y: 0 },
                    Cardinal::East => Point2D { x: 1 - DEFAULT_GRID_CELL_SIZE.x, y: 1 - DEFAULT_GRID_CELL_SIZE.z },
                    Cardinal::South => Point2D { x: 0, y: 1 - DEFAULT_GRID_CELL_SIZE.z }, 
                    Cardinal::West => Point2D { x: 0, y: 0 },
                }).add_y(y));

                let set = rng.choose(&sets);
                let shapes = &data.borrow().building_sets.get(set).expect("Building set not found").shapes;
                let mut shapes_dict = shapes.iter()
                    .enumerate()
                    .map(|(index, shape)| {
                        (index, shape.cells().iter().map(|cell| cell.drop_y()).collect::<HashSet<_>>().iter().count() as f32)
                    })
                    .collect::<HashMap<_, _>>();

                while !shapes_dict.is_empty() {
                    let index = rng.pop_weighted(&mut shapes_dict).expect("No shapes available").0;
                    let shape = shapes.get(index).expect("Shape index out of bounds");
                    let mut shape = shape.clone(); 
                    
                    shape.rotate(match direction {
                        Cardinal::South => Rotation::None,
                        Cardinal::West => Rotation::Once,
                        Cardinal::North => Rotation::Twice,
                        Cardinal::East => Rotation::Thrice,
                    });
                    
                    let footprint = shape.get_footprint(&grid);
                    if footprint.iter().any(|point| !block.contains(point) || editor.world().is_claimed(*point) || editor.world().is_water(*point)) {
                        continue;
                    }

                    let data_ref = &data.borrow();
                    let palette = district_styles.get(&block_districts[block_index])
                        .expect("Super parcel style not found")
                        .generate_palette(rng);

                    place_building(editor, &shape, grid, set, data_ref, style, rng, &palette).await;
                    break;
                }
            }
        }
    }
}

pub enum PavingType {
    Stone,
    Sandstone,
    RedSandstone
}

impl PavingType {
    pub fn from_biome(biome : Biome) -> Self {
        match biome.name() {
            "desert" | "desert_hills" | "desert_lakes" | "beach" => PavingType::Sandstone,
            "badlands" | "eroded_badlands" | "wooded_badlands" | "savanna" | "savanna_plateau" | "shattered_savanna" | "shattered_savanna_plateau" => PavingType::RedSandstone,
            _ => PavingType::Stone,
        }
    }
}

pub async fn smooth_and_pave_road(editor : &mut Editor, rng : &mut RNG, outers : &HashSet<Point2D>, paving_type : PavingType) {
    let mut points = outers.iter().map(|p| editor.world().add_non_tree_height(*p)).collect::<HashSet<_>>();
    points = average_to_neighbours_5_away(&points).iter().map(|p| if p.y > 63 { *p } else { p.with_y(63) }).collect();
    force_height(editor, &points, true).await;

    let block_vec : Vec<Block> = match paving_type {
        PavingType::Stone => vec![
            "stone".into(), "cobblestone".into(), "stone_bricks".into(), "andesite".into(), "gravel".into(),
            "stone_stairs".into(), "cobblestone_stairs".into(), "stone_bricks_stairs".into(), "andesite_stairs".into(),
            "stone_slab".into(), "cobblestone_slab".into(), "stone_bricks_slab".into(), "andesite_slab".into(),
        ],
        PavingType::Sandstone => vec![
            "sandstone".into(), "cut_sandstone".into(), "smooth_sandstone".into(), "birch_planks".into(), "sand".into(),
            "sandstone_stairs".into(), "sandstone_stairs".into(), "smooth_sandstone_stairs".into(), "birch_wood_stairs".into(),
            "sandstone_slab".into(), "cut_sandstone_slab".into(), "smooth_sandstone_slab".into(), "birch_wood_slab".into(),
        ],
        PavingType::RedSandstone => vec![
            "red_sandstone".into(), "cut_red_sandstone".into(), "smooth_red_sandstone".into(), "acacia_planks".into(), "red_sand".into(),
            "red_sandstone_stairs".into(), "red_sandstone_stairs".into(), "smooth_red_sandstone_stairs".into(), "acacia_wood_stairs".into(),
            "red_sandstone_slab".into(), "cut_red_sandstone_slab".into(), "smooth_red_sandstone_slab".into(), "acacia_wood_slab".into(),
        ],
    };

    let mut blocks_dict: HashMap<usize, HashMap<usize, f32>> = HashMap::new();

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

    // fill in below so we don't have weird artifacts
    // for point in outers.iter() {
    //     let height = editor.world().get_height_at(*point);
        
    //     for dy in 1..=2 {
    //         let index = rng.choose_weighted(&blocks_dict[&0]);
    //         let block = block_vec[*index].clone();
    //         editor.place_block(&block, point.add_y(height - dy)).await;
    //     }
    // }
}

pub async fn place_building(editor : &mut Editor, shape : &BuildingShape, grid : Grid, set : &BuildingSetID, data : &LoadedData, style : Style, rng : &mut RNG, palette : &Palette) {
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

    let set = data.building_sets.get(set).expect("Building set not found");

    let roof_set = rng.choose(&set.roof_sets);
    let wall_set = rng.choose(&set.wall_sets);

    for cell in building.shape.cells().iter() {
        for point in grid.get_cell_rect(*cell).iter() {
            editor.place_block_forced(&"air".into(), point).await;
        }
    }

    build_walls(editor, wall_set, &mut building, data, rng).await.expect("Failed to build walls");
    build_roof(editor, data, &mut building, roof_set, rng).await.expect("Failed to build roof");        
    build_floor(editor, data, &mut building, rng).await;
    build_stairs(editor, &mut building, data, rng).await;
    build_foundation(editor, &building, data, rng).await;

    // Claim points outside of windows and doors
    if let Some(windows) = building.shape.windows() {
        for window in windows {
            let point = grid.get_door_world_position(window.cell, window.direction) + window.direction.into();
            if editor.world().is_claimed(point.drop_y()) {
                continue; // Skip if the point is already claimed
            }
            editor.world_mut().claim(point.drop_y(), BuildClaim::Building(building.id));
        }
    }   
    if let Some(doors) = building.shape.doors() {
        for door in doors {
            let point = grid.get_door_world_position(door.cell, door.direction) + door.direction.into();
            
            let mut clear_point = point;

            for _ in 0..5 {
                editor.place_block_forced(&"air".into(), point).await;
                editor.place_block_forced(&"air".into(), point + UP).await;
                clear_point += door.direction.into();

                match editor.world().get_claim(clear_point.drop_y()) {
                    Some(claim) => match claim {
                        BuildClaim::Building(building_id) => {
                            if building_id != building.id {
                                break;
                            }
                        },
                        _ => {
                            break;
                        },
                    },
                    None => todo!(),
                }
            }

            if editor.world().is_claimed(point.drop_y()) {
                continue; // Skip if the point is already claimed
            }
            editor.world_mut().claim(point.drop_y(), BuildClaim::Building(building.id));
            
        }
    }

    editor.world_mut().buildings.push(building);
}