pub use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BlockID(String);

impl From<&str> for BlockID {
    fn from(s: &str) -> Self {
        BlockID(s.to_string())
    }
}

impl BlockID {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_water(&self) -> bool {
        let id_string = &self.0;
        matches!(
            id_string.as_str(),
            "water" | "flowing_water" | "bubble_column" | "kelp" | "kelp_plant"
        )
    }

    pub fn is_tree(&self) -> bool {
        let id_string = &self.0;
        id_string.contains("log") || id_string.contains("leaves")
    }

    pub fn is_leaves(&self) -> bool {
        let id_string = &self.0;
        id_string.contains("leaves")
    }

    pub fn is_air(&self) -> bool {
        let id_string = &self.0;
        matches!(id_string.as_str(), "air" | "cave_air" | "void_air")
    }
}

impl Default for BlockID {
    fn default() -> Self {
        BlockID("air".to_string())
    }
}