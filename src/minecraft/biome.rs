use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Biome {
    Unknown, // Placeholder for unknown biomes
    #[serde(rename = "minecraft:river")]
    River,
    #[serde(rename = "minecraft:plains")]
    Plains,
}
