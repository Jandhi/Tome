use std::collections::{HashMap, HashSet};
use crate::editor::Editor;
use crate::generator::materials::Placer;
use crate::generator::nbts::{place_structure, Structure, StructureId};
use crate::geometry::{Point2D, Point3D, is_straight_not_diagonal_point2d, Cardinal};
use crate::noise::RNG;
use crate::minecraft::{Block, BlockID};
use crate::generator::BuildClaim;
use crate::generator::districts::WallType;
use log::{info, warn};



pub async fn build_wall_gate(
    wall_points: &Vec<Point3D>,
    editor: &mut Editor,
    rng: &mut RNG,
    material_placer: &Placer<'_>,
    is_thin: bool,
    is_palisade: bool,
    enhanced_wall_points: Option<&Vec<(Point3D, Vec<Cardinal>, WallType)>>,
    inner_wall_set: Option<&HashSet<Point3D>>,
    structures: & HashMap<StructureId, Structure>,
    gate_height: i32,
) {
    let distance_to_next_gate = 60;
    let gate_size = 7; // eventually this should depend on some type of gate we place
    let mut gate_possible = 0;
    let palisade_gate = structures.get(&"basic_palisade_gate".into()).expect("Structure not found");
    let thin_gate = structures.get(&"basic_thin_gate".into()).expect("Structure not found");
    let wide_gate = structures.get(&"basic_wide_gate".into()).expect("Structure not found");

    let inner_wall_points = inner_wall_set
        .map(|set| set.iter().map(|p| p.drop_y()).collect::<HashSet<Point2D>>())
        .unwrap_or_default();

    let air = Block {
            id: BlockID::Air,
            data: None,
            state: None,
    };
    for (i, point) in wall_points.iter().enumerate() {
        if gate_possible == 0 {
            if is_gate_possible(*point, wall_points, gate_size, i) {
                if is_palisade {
                    let middle_point = Point3D::new(wall_points[i+2].x, editor.world().get_height_at(wall_points[i+2].drop_y()), wall_points[i+2].z);
                    let direction: Cardinal;
                    let neighbours: Vec<Point2D>;
                    if point.x == wall_points[i + 6].x {
                        direction = Cardinal::North;
                    } else {
                        direction = Cardinal::East;
                    }
                    if direction == Cardinal::East {
                        neighbours = ((middle_point.x - 2)..=(middle_point.x + 2))
                            .flat_map(|x| {
                                ((middle_point.z - 1)..=(middle_point.z + 1))
                                    .map(move |z| Point2D { x, y: z })
                            })
                            .collect::<Vec<Point2D>>();
                    } else {
                        neighbours = ((middle_point.x - 1)..=(middle_point.x + 1))
                            .flat_map(|x| {
                                ((middle_point.z - 2)..=(middle_point.z + 2))
                                    .map(move |z| Point2D { x, y: z })
                            })
                            .collect::<Vec<Point2D>>();
                    }
                    let height = middle_point.y;
                    for neighbour in neighbours.iter() {
                        editor.world().claim(*neighbour, BuildClaim::Gate);
                            for height in height..height + gate_height {
                            editor.place_block_force(
                                &air,
                                neighbour.add_y(height)
                            ).await;
                        }
                    }
                    info!("Placing palisade gate at: {:?}", middle_point);
                    place_structure(editor, None, &palisade_gate, middle_point, direction, None, None, false, false).await.expect("Failed to place gate");
                    gate_possible = distance_to_next_gate;
                } else if is_thin{
                    let middle_point = Point3D::new(wall_points[i+3].x, editor.world().get_height_at(wall_points[i+3].drop_y()), wall_points[i+3].z);
                    let direction: Cardinal; // to do based on additional wall info
                    let neighbours: Vec<Point2D>;
                    if point.x == wall_points[i + 6].x {
                        direction = Cardinal::North;
                    } else {
                        direction = Cardinal::East;
                    }
                    if direction == Cardinal::North || direction == Cardinal::South {
                        neighbours = ((middle_point.x - 3)..=(middle_point.x + 3))
                            .flat_map(|x| {
                                ((middle_point.z - 1)..=(middle_point.z + 1))
                                    .map(move |z| Point2D { x, y: z })
                            })
                            .collect::<Vec<Point2D>>();
                    } else {
                        neighbours = ((middle_point.x - 1)..=(middle_point.x + 1))
                            .flat_map(|x| {
                                ((middle_point.z - 3)..=(middle_point.z + 3))
                                    .map(move |z| Point2D { x, y: z })
                            })
                            .collect::<Vec<Point2D>>();
                    }
                    let height = middle_point.y;
                    for neighbour in neighbours.iter() {
                        editor.world().claim(*neighbour, BuildClaim::Gate);
                            for height in height..height + gate_height {
                            editor.place_block_force(
                                &air,
                                neighbour.add_y(height)
                            ).await;
                        }
                    }
                    let mirror_x = if direction == Cardinal::North || direction == Cardinal::South { true } else { false };
                    // look if mirror is working
                    info!("Placing thin gate at: {:?}", middle_point);
                    place_structure(editor, None, &thin_gate, middle_point, direction, None, None, mirror_x, false).await.expect("Failed to place gate");
                    gate_possible = distance_to_next_gate;
                } else {
                    let enhanced_points = enhanced_wall_points.expect("Enhanced wall points should be provided for this wall type");
                    let direction: Cardinal = enhanced_points[i+3].1[0]; //might need to deal with error case if no directions
                    let middle_point = enhanced_points[i+3].0.drop_y() + Point2D::from(direction) * 2;

                    for a in i..i + gate_size as usize {
                        let inner_wall_point = enhanced_points[a].0.drop_y() + Point2D::from(direction) * 5;
                        if inner_wall_points.contains(&inner_wall_point) {
                            break;
                        }
                        if a == i + 6 {
                            info!("Building gate at {:?}", middle_point);
                            let neighbours: Vec<Point2D> = ((middle_point.x - 3)..=(middle_point.x + 3))
                                .flat_map(|x| {
                                    ((middle_point.y - 3)..=(middle_point.y + 3))
                                        .map(move |y| Point2D { x, y })
                                })
                                .collect::<Vec<Point2D>>();

                            let height = editor.world().get_height_at(middle_point);
                            for neighbour in neighbours.iter() {
                                editor.world().claim(*neighbour, BuildClaim::Gate);
                                    for height in height..height + gate_height {
                                    editor.place_block_force(
                                        &air,
                                        neighbour.add_y(height)
                                    ).await;
                                }
                            }
                            let mirror_x = if direction == Cardinal::North || direction == Cardinal::South { true } else { false };
                            // look if mirror is working
                            info!("Placing wide gate at: {:?}", middle_point);
                            place_structure(editor, None, &wide_gate, middle_point.add_y(height), direction.turn_right(), None, None, mirror_x, false).await.expect("Failed to place gate");
                            gate_possible = distance_to_next_gate;
                        }
                    }
                }
            }
        } else {
        gate_possible -= 1;
        }
    }
}

pub fn is_gate_possible(
    point: Point3D,
    wall_list: &Vec<Point3D>,
    gate_size: i32,
    index: usize,
) -> bool {
    // Check if the point is a valid gate position
    if index + gate_size as usize > wall_list.len() {
        return false; // Not enough points to form a gate, doesnt loop
    }
    // Check if the point is straight and not diagonal
    if is_straight_not_diagonal_point2d(
        Point2D { x: point.x, y: point.z },
        Point2D { x: wall_list[index + gate_size as usize - 1].x, y: wall_list[index + gate_size as usize - 1].z },
        gate_size - 1,
    ) && (point.y - wall_list[index + gate_size as usize - 1].y).abs() <= 1 {
        return true;
    }

    false
}