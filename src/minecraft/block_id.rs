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
        let name = self.name();
        name.contains("log")
            || name.contains("leaves")
            || name.contains("_stem")
            || name.contains("_wood")
            // Mangrove (and nether) roots + propagules: skipped in the ground
            // heightmap (`get_non_tree_height`) so walls/buildings sit on the mud
            // below them, not perched on the root tangle.
            || name.contains("roots")
            || name.ends_with("_propagule")
    }

    pub fn is_leaves(&self) -> bool {
        self.name().contains("leaves")
    }

    /// A log / stripped log / wood / stem / hyphae — the axis-rotatable wood
    /// pillars. Goes through `name()` so it matches `minecraft:`-namespaced ids.
    pub fn is_log(&self) -> bool {
        let name = self.name();
        name.ends_with("_log") || name.ends_with("_wood") || name.ends_with("_stem") || name.ends_with("_hyphae")
    }

    /// True if this block accepts an `axis` blockstate (logs/pillars + a handful
    /// of stone columns). Anything else must not be placed with an `axis`, or the
    /// server rejects the placement and the block silently vanishes.
    pub fn is_axis_block(&self) -> bool {
        if self.is_log() {
            return true;
        }
        let name = self.name();
        name.ends_with("_pillar")
            || matches!(
                name,
                "basalt"
                    | "polished_basalt"
                    | "deepslate"
                    | "bone_block"
                    | "bamboo_block"
                    | "stripped_bamboo_block"
                    | "muddy_mangrove_roots"
                    | "hay_block"
                    | "chain"
                    | "ochre_froglight"
                    | "verdant_froglight"
                    | "pearlescent_froglight"
            )
    }

    pub fn is_air(&self) -> bool {
        matches!(self.name(), "air" | "cave_air" | "void_air")
    }

    /// True for any colored bed. Placing a bed foot with block updates on makes
    /// the server auto-spawn the head, duplicating the half the NBT already
    /// contains — so beds must be pasted update-free.
    pub fn is_bed(&self) -> bool {
        self.name().ends_with("_bed")
    }

    /// A structure void marks "leave whatever is already here" in an NBT — it must
    /// be skipped at placement, never written, or it punches invisible holes in the
    /// terrain a structure is meant to sit on (e.g. the foundation layers of a mine).
    pub fn is_structure_void(&self) -> bool {
        self.name() == "structure_void"
    }

    /// Natural terrain a road may carve through: untouched ground plus the
    /// dirt/sand fill that terraforming (`force_height`, foundation blends)
    /// lays down. Deliberately excludes every structure block.
    pub fn is_natural_ground(&self) -> bool {
        matches!(
            self.name(),
            "dirt" | "grass_block" | "coarse_dirt" | "rooted_dirt" | "podzol" | "mycelium"
                | "sand" | "red_sand" | "sandstone" | "gravel" | "clay" | "mud"
                | "stone" | "dirt_path" | "snow" | "moss_block"
        )
    }
}

impl Default for BlockID {
    fn default() -> Self {
        BlockID("air".to_string())
    }
}