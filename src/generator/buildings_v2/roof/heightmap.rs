/// Height at every (x, z) position in the roof's bounding box.
/// Heights are relative to roof_y (0 = at wall top, negative = below for overhang).
pub struct RoofHeightmap {
    min_x: i32,
    min_z: i32,
    width: usize,
    depth: usize,
    heights: Vec<f32>, // row-major: index = (x - min_x) * depth + (z - min_z)
}

impl RoofHeightmap {
    pub fn new(min_x: i32, min_z: i32, width: usize, depth: usize) -> Self {
        RoofHeightmap {
            min_x,
            min_z,
            width,
            depth,
            heights: vec![f32::NEG_INFINITY; width * depth],
        }
    }

    fn index(&self, x: i32, z: i32) -> Option<usize> {
        let lx = x - self.min_x;
        let lz = z - self.min_z;
        if lx >= 0 && lz >= 0 && (lx as usize) < self.width && (lz as usize) < self.depth {
            Some(lx as usize * self.depth + lz as usize)
        } else {
            None
        }
    }

    pub fn get(&self, x: i32, z: i32) -> f32 {
        self.index(x, z)
            .map(|i| self.heights[i])
            .unwrap_or(f32::NEG_INFINITY)
    }

    pub fn set(&mut self, x: i32, z: i32, value: f32) {
        if let Some(i) = self.index(x, z) {
            self.heights[i] = value;
        }
    }

    /// Merge another heightmap using max(self, other) at each overlapping position.
    pub fn merge_max(&mut self, other: &RoofHeightmap) {
        let x_start = self.min_x.max(other.min_x);
        let x_end = (self.min_x + self.width as i32).min(other.min_x + other.width as i32);
        let z_start = self.min_z.max(other.min_z);
        let z_end = (self.min_z + self.depth as i32).min(other.min_z + other.depth as i32);

        for x in x_start..x_end {
            for z in z_start..z_end {
                let other_h = other.get(x, z);
                if let Some(i) = self.index(x, z) {
                    self.heights[i] = self.heights[i].max(other_h);
                }
            }
        }
    }

    pub fn min_x(&self) -> i32 {
        self.min_x
    }
    pub fn min_z(&self) -> i32 {
        self.min_z
    }
    pub fn max_x(&self) -> i32 {
        self.min_x + self.width as i32 - 1
    }
    pub fn max_z(&self) -> i32 {
        self.min_z + self.depth as i32 - 1
    }
}
