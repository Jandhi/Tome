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

    /// The id with any `minecraft:` namespace stripped. Block ids arrive from the
    /// server (and the synthetic world) fully qualified — e.g. `minecraft:water` —
    /// so exact-match checks below must compare against the un-namespaced name or
    /// they silently never match.
    fn name(&self) -> &str {
        self.0.strip_prefix("minecraft:").unwrap_or(&self.0)
    }

    pub fn is_water(&self) -> bool {
        matches!(
            self.name(),
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

    /// True for tree trunk blocks (`*_log`, including stripped variants). Used to
    /// locate a tree's stem, as opposed to its surrounding canopy ([`is_leaves`]).
    pub fn is_log(&self) -> bool {
        self.0.contains("log")
    }

    pub fn is_air(&self) -> bool {
        matches!(self.name(), "air" | "cave_air" | "void_air")
    }

    /// A structure void marks "leave whatever is already here" in an NBT — it must
    /// be skipped at placement, never written, or it punches invisible holes in the
    /// terrain a structure is meant to sit on (e.g. the foundation layers of a mine).
    pub fn is_structure_void(&self) -> bool {
        self.name() == "structure_void"
    }
}

impl Default for BlockID {
    fn default() -> Self {
        BlockID("air".to_string())
    }
}