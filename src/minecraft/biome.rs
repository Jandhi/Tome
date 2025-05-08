use serde::Deserialize;



#[derive(Debug, Clone, Copy)]
pub enum Biome {
    River,
    Plains,
}


impl<'de> Deserialize<'de> for Biome {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "minecraft:river" => Ok(Biome::River),
            "minecraft:plains" => Ok(Biome::Plains),
            _ => Err(serde::de::Error::custom(format!(
                "Unknown biome: {}",
                value
            ))),
        }
    }
}