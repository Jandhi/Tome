use crate::{generator::materials::Palette, noise::RNG};

pub enum DistrictStyle {
    Mono(Mono),
    MultiVariant(MultiVariant),
    WeightedMultiVariant(WeightedMultiVariant),
}

impl DistrictStyle {
    pub fn generate_palette(&self, rng : &mut RNG) -> Palette {
        match self {
            DistrictStyle::Mono(mono) => mono.palette.clone(),
            DistrictStyle::MultiVariant(multi) => {
                multi.core.clone()
                    .merged_with(rng.choose(&multi.roofs))
                    .merged_with(rng.choose(&multi.woods))
                    .merged_with(rng.choose(&multi.stones))
            }
            DistrictStyle::WeightedMultiVariant(multi) => {
                multi.core.clone()
                    .merged_with(rng.choose_weighted_vec(&multi.roofs))
                    .merged_with(rng.choose_weighted_vec(&multi.woods))
                    .merged_with(rng.choose_weighted_vec(&multi.stones))
            }
        }
    }

    pub fn generate_style(rng : &mut RNG, cores : Vec<&Palette>, roofs : Vec<&Palette>, woods : Vec<&Palette>, stones : Vec<&Palette>) -> DistrictStyle {
        match rng.rand_i32(100) {
            0..33 => { // Bimodal
                DistrictStyle::MultiVariant(MultiVariant {
                    core: (*rng.choose(&cores)).clone(),
                    roofs: rng.choose_many(&roofs, 2).into_iter().cloned().cloned().collect(),
                    woods: rng.choose_many(&roofs, 2).into_iter().cloned().cloned().collect(),
                    stones: rng.choose_many(&roofs, 2).into_iter().cloned().cloned().collect(),
                })
            }
            33..66 => { // Weighted MultiVariant
                DistrictStyle::WeightedMultiVariant(WeightedMultiVariant {
                    core: (*rng.choose(&cores)).clone(),
                    roofs: roofs.into_iter().map(|palette| (palette.clone(), rng.rand_i32(100) as f32)).collect(),
                    woods: woods.into_iter().map(|palette| (palette.clone(), rng.rand_i32(100) as f32)).collect(),
                    stones: stones.into_iter().map(|palette| (palette.clone(), rng.rand_i32(100) as f32)).collect(),
                })
            }
            _ => {
                DistrictStyle::Mono(Mono {
                    palette: (*rng.choose(&cores)).clone()
                        .merged_with(*rng.choose(&roofs))
                        .merged_with(*rng.choose(&woods))
                        .merged_with(*rng.choose(&stones)),
                })
            }
        }
    }
}

pub struct Mono {
    pub palette : Palette,
}

pub struct MultiVariant {
    pub core : Palette,
    pub roofs : Vec<Palette>,
    pub woods : Vec<Palette>,
    pub stones : Vec<Palette>,
}

pub struct WeightedMultiVariant {
    pub core : Palette,
    pub roofs : Vec<(Palette, f32)>,
    pub woods : Vec<(Palette, f32)>,
    pub stones : Vec<(Palette, f32)>,
}