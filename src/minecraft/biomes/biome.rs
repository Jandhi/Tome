use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Biome(String);

impl Biome {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Strip "minecraft:" prefix for cleaner matching
    pub fn name(&self) -> &str {
        self.0.strip_prefix("minecraft:").unwrap_or(&self.0)
    }

    pub fn unknown() -> Self {
        Biome("Unknown".to_string())
    }

    pub fn is_unknown(&self) -> bool {
        self.0 == "Unknown"
    }
}

impl From<&str> for Biome {
    fn from(s: &str) -> Self {
        Biome(s.to_string())
    }
}

impl Default for Biome {
    fn default() -> Self {
        Biome::unknown()
    }
}

impl std::fmt::Display for Biome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}
