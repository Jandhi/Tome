mod material;
mod feature;
mod test;
mod placer;
mod gradient;
mod palette;
mod role;

pub use material::Material;
pub use material::MaterialId;
pub use feature::MaterialFeature;
pub use placer::Placer;
pub use gradient::Gradient;
pub use palette::{Palette, PaletteId, PaletteSwapResult};
pub use role::MaterialRole;