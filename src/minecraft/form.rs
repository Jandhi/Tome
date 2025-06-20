use serde_derive::{Serialize, Deserialize};

use crate::minecraft::BlockID;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlockForm {
    #[serde(rename = "block")]
    Block,
    #[serde(rename = "stairs")]
    Stairs,
    #[serde(rename = "slab")]
    Slab,
    #[serde(rename = "wall")]
    Wall,
    #[serde(rename = "fence")]
    Fence,
    #[serde(rename = "fence_gate")]
    FenceGate,
    #[serde(rename = "pillar")]
    Pillar,
    #[serde(rename = "trapdoor")]
    Door,
    #[serde(rename = "door")]
    Trapdoor,
    #[serde(rename = "button")]
    Button,
    #[serde(rename = "pressure_plate")]
    PressurePlate,
    #[serde(rename = "chiseled")]
    Chiseled,
    #[serde(rename = "wood")]
    Wood,
    #[serde(rename = "log")]
    Log,

    // SIGNS
    #[serde(rename = "sign")]
    Sign,
    #[serde(rename = "wall_sign")]
    WallSign,
    #[serde(rename = "hanging_sign")]
    HangingSign,
    #[serde(rename = "hanging_wall_sign")]
    HangingWallSign,

    // DECORATION
    #[serde(rename = "flower")]
    Flower,

    // SPARSE
    #[serde(rename = "sparse")]
    Sparse,

}

impl BlockForm {
    pub fn infer_from_block(block : BlockID) -> BlockForm {
        let id_string = serde_json::to_string(&block).expect("Failed to serialize BlockID to string");

        if id_string.contains("stairs") {
            BlockForm::Stairs
        } else if id_string.contains("slab") {
            BlockForm::Slab
        } else if id_string.contains("wall") {
            BlockForm::Wall
        } else if id_string.contains("fence") {
            BlockForm::Fence
        } else if id_string.contains("fence_gate") {
            BlockForm::FenceGate
        } else if id_string.contains("pillar") || id_string.contains("log") {
            BlockForm::Pillar
        } else if id_string.contains("trapdoor") {
            BlockForm::Trapdoor
        } else if id_string.contains("door") {
            BlockForm::Door
        } else if id_string.contains("button") {
            BlockForm::Button
        } else if id_string.contains("pressure_plate") {
            BlockForm::PressurePlate
        } else if id_string.contains("chiseled") {
            BlockForm::Chiseled
        } else if id_string.contains("sign") && !id_string.contains("wall_sign") && !id_string.contains("hanging_sign") && !id_string.contains("hanging_wall_sign") {
            BlockForm::Sign
        } else if id_string.contains("wall_sign") && !id_string.contains("hanging_wall_sign") {
            BlockForm::WallSign
        } else if id_string.contains("hanging_sign") {
            BlockForm::HangingSign
        } else if id_string.contains("hanging_wall_sign") {
            BlockForm::HangingWallSign
        } else if id_string.contains("air") || id_string.contains("water") || id_string.contains("lava") || id_string.contains("snow") {
            BlockForm::Sparse
        } else {
            BlockForm::Block // Default to block form for unrecognized types
        }
    }

    pub fn density(self) -> f32 {
        match self {
            BlockForm::Block => 1.0,
            BlockForm::Stairs => 0.5,
            BlockForm::Slab => 0.4,
            BlockForm::Wall => 0.5,
            BlockForm::Fence => 0.4,
            BlockForm::FenceGate => 0.4,
            BlockForm::Pillar => 1.0,
            BlockForm::Trapdoor => 0.2,
            BlockForm::Door => 0.2,
            BlockForm::Button => 0.1,
            BlockForm::PressurePlate => 0.1,
            BlockForm::Chiseled => 1.0,
            BlockForm::Sign | BlockForm::WallSign | BlockForm::HangingSign | BlockForm::HangingWallSign => 0.1,
            BlockForm::Sparse => 0.0,
            BlockForm::Wood => 1.0,
            BlockForm::Log => 1.0,
            BlockForm::Flower => 0.0,
        }
    }
}