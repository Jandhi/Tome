use std::{collections::{HashMap, HashSet}, hash::Hash};

use crate::{geometry::Point2D, noise::RNG};

pub fn voronoi<TNode, TDist>(points : &HashSet<TNode>, distance_func : &impl Fn(TNode, TNode) -> TDist, rng : &mut RNG, sections : usize) -> Vec<HashSet<TNode>> 
    where TNode: Eq + Clone + Hash,
          TDist : PartialOrd + Copy + Ord
{
    let points_vec = points.iter().collect::<Vec<_>>();
    let start_points = rng.choose_many(&points_vec, sections).iter().map(|node| (***node).clone()).collect::<Vec<_>>();

    voronoi_with_points(points, distance_func, start_points)
}

pub fn voronoi_with_points<TNode, TDist>(points : &HashSet<TNode>, distance_func : &impl Fn(TNode, TNode) -> TDist, start_points : Vec<TNode>) -> Vec<HashSet<TNode>> 
    where TNode: Eq + Clone + Hash,
          TDist : PartialOrd + Copy + Ord
{
    let mut sections : Vec<HashSet<TNode>> = start_points.iter().map(|node| {
        HashSet::new()
    }).collect();

    for point in points {
        let best_index = (0..sections.len()).min_by_key(|&i| {
            distance_func(point.clone(), start_points[i].clone())
        }).unwrap();

        sections[best_index].insert(point.clone());
    }

    sections
}

pub fn voronoi_fill<TNode>(points : &HashSet<TNode>, neighbour_func : &impl Fn(TNode) -> Vec<TNode>, rng : &mut RNG, sections : usize) -> Vec<HashSet<TNode>> 
    where TNode : Eq + Clone + Hash {
    let points_vec = points.iter().collect::<Vec<_>>();
    let start_points = rng.choose_many(&points_vec, sections).iter().map(|node| (***node).clone()).collect::<Vec<_>>();
    voronoi_fill_with_points(points, neighbour_func, start_points)
}

pub fn voronoi_fill_with_points<TNode>(points : &HashSet<TNode>, neighbour_func : &impl Fn(TNode) -> Vec<TNode>, start_points : Vec<TNode>) -> Vec<HashSet<TNode>> 
    where TNode : Eq + Clone + Hash
{
    let mut sections : Vec<HashSet<TNode>> = start_points.iter().map(|node| {
        let mut section = HashSet::new();
        section.insert(node.clone());
        section
    }).collect();

    let mut queue : Vec<(TNode, usize)> = start_points.iter().enumerate().map(|(i, node)| (node.clone(), i)).collect();
    let mut visited : HashSet<TNode> = start_points.iter().map(|node| node.clone()).collect(); 

    while !queue.is_empty() {
        let (point, section_index) = queue.remove(0);
        let neighbours = neighbour_func(point);

        for neighbour in neighbours {
            if visited.contains(&neighbour) {
                continue;
            }

            if !points.contains(&neighbour) {
                continue;
            }

            sections[section_index].insert(neighbour.clone());
            visited.insert(neighbour.clone());
            queue.push((neighbour, section_index));
        }
    }

    sections
}

pub fn voronoi_fill_with_recenter<TNode>(
    points : &HashSet<TNode>, 
    neighbour_func : &impl Fn(TNode) -> Vec<TNode>, 
    recenter_func : &impl Fn(&HashSet<TNode>) -> TNode,
    rng : &mut RNG,
    sections : usize,
    recenters : usize
) -> Vec<HashSet<TNode>> 
    where TNode : Eq + Clone + Hash
{
    let points_vec = points.iter().collect::<Vec<_>>();
    let start_points = rng.choose_many(&points_vec, sections).iter().map(|node| (***node).clone()).collect::<Vec<_>>();
    let mut sections = voronoi_fill_with_points(points, &neighbour_func, start_points);

    for _ in 0..recenters {
        let start_points = sections.iter().map(|section| {
            recenter_func(section)
        }).collect::<Vec<_>>();

        sections = voronoi_fill_with_points(points, &neighbour_func, start_points);
    }

    sections
}