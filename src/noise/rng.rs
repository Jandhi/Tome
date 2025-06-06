use std::{collections::HashMap, hash::{DefaultHasher, Hash, Hasher}};

use crate::geometry::{Point2D, Point3D};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Seed(pub i64);

impl From<i64> for Seed {
    fn from(value: i64) -> Self {
        Seed(value)
    }
}

pub struct RNG {
    seed: Seed,
    state: i64,
}

impl RNG {
    pub fn new(seed: Seed) -> Self {
        RNG { seed, state: 0 }
    }

    pub fn from_seed_and_string(seed: Seed, string: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        hasher.write(string.as_bytes());
        return Self {
            seed: Seed(seed.0 ^ hasher.finish() as i64),
            state: 0,
        }
    }

    pub fn next(&mut self) -> i64 {
        self.state += 1;
        squirrel3(self.seed, self.state)
    }

    pub fn rand_i32(&mut self, max : i32) -> i32 {
        (self.next() & 0x7FFFFFFF) as i32 % max
    }

    pub fn rand_i32_range(&mut self, min : i32, max : i32) -> i32 {
        let range = max - min;
        (self.next() & 0x7FFFFFFF) as i32 % range + min
    }

    pub fn rand_point2d(&mut self, max : Point2D) -> Point2D {
        let x = self.rand_i32(max.x);
        let y = self.rand_i32(max.y);
        Point2D::new(x, y)
    }

    pub fn rand_point2d_range(&mut self, min : Point2D, max : Point2D) -> Point2D {
        let x = self.rand_i32(max.x - min.x) + min.x;
        let y = self.rand_i32(max.y - min.y) + min.y;
        Point2D::new(x, y)
    }

    pub fn rand_point3d(&mut self, max : Point3D) -> Point3D {
        let x = self.rand_i32(max.x);
        let y = self.rand_i32(max.y);
        let z = self.rand_i32(max.z);
        Point3D::new(x, y, z)
    }

    pub fn rand_point3d_range(&mut self, min : Point3D, max : Point3D) -> Point3D {
        let x = self.rand_i32(max.x - min.x) + min.x;
        let y = self.rand_i32(max.y - min.y) + min.y;
        let z = self.rand_i32(max.z - min.z) + min.z;
        Point3D::new(x, y, z)
    }

    pub fn choose<'a, T>(&mut self, options: &'a [T]) -> &'a T {
        let index = self.rand_i32(options.len() as i32) as usize;
        &options[index]
    }

    pub fn pop<'a, T>(&mut self, options: &'a mut Vec<T>) -> Option<T> {
        if options.is_empty() {
            return None;
        }
        let index = self.rand_i32(options.len() as i32) as usize;
        Some(options.remove(index))
    }

    pub fn choose_weighted<'map, T>(&mut self, options: &'map HashMap<T, f32>) -> &'map T {
        let total_weight: f32 = options.values().sum();
        let mut rand_value = self.rand_i32(100000) as f32 / 100000.0 * total_weight;
        for (item, weight) in options.iter() {
            if rand_value < *weight {
                return item;
            }
            rand_value -= weight;
        }
        unreachable!()
    }

    pub fn pop_weighted<'map, 'items, T>(&mut self, options: &'map mut HashMap<T, f32>) -> Option<(T, f32)>
    where
        T: Eq + std::hash::Hash + Clone,
    {
        if options.is_empty() {
            return None;
        }
        let total_weight: f32 = options.values().sum();
        let mut rand_value = self.rand_i32(100000) as f32 / 100000.0 * total_weight;
        for (item, weight) in options.iter() {
            if rand_value < *weight {
                let item_key = (*item).clone();
                let weight_value = *weight;
                options.remove(&item_key);
                return Some((item_key, weight_value));
            }
            rand_value -= weight;
        }
        unreachable!()
    }

    pub fn chance(&mut self, successes : i32, total : i32) -> bool {
        if total == 0 {
            return false;
        }
        let rand_value = self.rand_i32(total);
        rand_value < successes
    }

    pub fn percent(&mut self, percent : i32) -> bool {
        if percent < 0 || percent > 100 {
            panic!("Percent must be between 0 and 100");
        }
        let rand_value = self.rand_i32(100);
        rand_value < percent
    }

    pub fn shuffle(&mut self, items : &mut Vec<i32>) {
        let len = items.len();
        for i in (1..len).rev() {
            let j = self.rand_i32(i as i32) as usize;
            items.swap(i, j);
        }
    }
}

impl From<Seed> for RNG {
    fn from(seed: Seed) -> Self {
        RNG::new(seed)
    }
}

const BIT_NOISE1 : i64 = 0x85297A4D;
const BIT_NOISE2 : i64 = 0x68E31DA4;
const BIT_NOISE3 : i64 = 0x1859C4E9;

fn squirrel3(seed : Seed, position : i64) -> i64 {
    let mut noise = position;
    noise *= BIT_NOISE1;
    noise += seed.0;
    noise ^= noise >> 8;
    noise += BIT_NOISE2;
    noise ^= noise << 8;
    noise ^= BIT_NOISE3; // Should be *=
    noise ^= noise >> 8;
    noise
}