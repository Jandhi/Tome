use lerp::Lerp;
use log::info;
use noise::{NoiseFn, Perlin};

use crate::{generator::materials::feature::MaterialParameters, geometry::{Point3D, Rect3D}, noise::Seed};

pub struct PerlinSettings {
    perlin : noise::Perlin,
    octaves : u32,
    base_frequency : f32,
    frequency_multiplier : f32,
    weight_multiplier : f32,
}

impl PerlinSettings {
    pub fn new(
        seed: Seed,
        octaves: u32,
        base_frequency: f32,
        frequency_multiplier: f32,
        weight_multiplier: f32,
    ) -> Self {
        PerlinSettings {
            perlin: Perlin::new(seed.0 as u32),
            octaves,
            base_frequency,
            frequency_multiplier,
            weight_multiplier,
        }
    }

    pub fn get(&self, point: Point3D) -> f32 {
        let x = point.x as f32 * self.base_frequency as f32;
        let y = point.y as f32 * self.base_frequency as f32;
        let z = point.z as f32 * self.base_frequency as f32;

        let mut value : f32 = 0.0;
        let mut frequency = self.base_frequency;
        let mut weight : f32 = 1.0;

        for _ in 0..self.octaves {
            value += weight * self.perlin.get([
                (x * frequency) as f64,
                (y * frequency) as f64,
                (z * frequency) as f64,
            ]) as f32;
            frequency *= self.frequency_multiplier;
            weight *= self.weight_multiplier;
        }

        value
    }

    pub fn large(seed: Seed) -> PerlinSettings {
        PerlinSettings::new(
            seed,
            8,
            7.0 / 32.0,
            2.0,
            0.5,
        )
    }

    pub fn medium(seed : Seed) -> PerlinSettings {
        PerlinSettings::new(
            seed,
            8,
            10.0 / 32.0,
            2.0,
            0.5,
        )
    }

    pub fn small(seed : Seed) -> PerlinSettings {
        PerlinSettings::new(
            seed,
            8,
            16.0 / 32.0,
            2.0,
            0.5,
        )
    }
}

struct GradientAxis {
    min : i32,
    max : i32,
}

impl GradientAxis {
    pub fn new(min: i32, max: i32) -> Self {
        GradientAxis { min, max }
    }

    pub fn get_value(&self, value: f32) -> f32 {
        let range = self.max - self.min;
        if range == 0 {
            return 0.0; // Avoid division by zero
        }
        ((value - self.min as f32) / range as f32).clamp(0.0, 1.0)
    }
}

pub struct Gradient {
    perlin : PerlinSettings,
    gradient_strength : f32, // How much the gradient effects the material, from 0.0 to 1.0
    noise_strength : f32, // From 0.0 to 1.0, the proportion of noise in the gradient
    x : Option<GradientAxis>,
    y : Option<GradientAxis>,
    z : Option<GradientAxis>,
}

impl Gradient {
    pub fn new(perlin_settings: PerlinSettings, gradient_strength: f32, noise_strength: f32) -> Self {
        Gradient {
            perlin: perlin_settings,
            gradient_strength,
            noise_strength,
            x: None,
            y: None,
            z: None,
        }
    }

    pub fn with_x(mut self, min: i32, max: i32) -> Self {
        self.x = Some(GradientAxis::new(min, max));
        self
    }

    pub fn with_y(mut self, min: i32, max: i32) -> Self {
        self.y = Some(GradientAxis::new(min, max));
        self
    }

    pub fn with_z(mut self, min: i32, max: i32) -> Self {
        self.z = Some(GradientAxis::new(min, max));
        self
    }

    pub fn get_value(&self, point : Point3D) -> f32 {
        let mut value = 0.0;

        if let Some(axis) = &self.x {
            value += axis.get_value(point.x as f32);
        }
        if let Some(axis) = &self.y {
            value += axis.get_value(point.y as f32);
        }
        if let Some(axis) = &self.z {
            value += axis.get_value(point.z as f32);
        }

        // Normalize the value to be between 0.0 and 1.0
        value /= (match self.x { Some(_) => 1, None => 0 } + match self.y { Some(_) => 1, None => 0 } + match self.z { Some(_) => 1, None => 0 }) as f32;

        // Apply the gradient strength
        value = value.lerp(self.perlin.get(point), self.noise_strength);

        info!("Gradient value for point {:?}: {}", point, value);

        value.clamp(0.0, 1.0) * self.gradient_strength + 0.5 * (1.0 - self.gradient_strength)
    }
}