use serde_derive::{Serialize, Deserialize};

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
    
    // SIGNS
    #[serde(rename = "sign")]
    Sign,
    #[serde(rename = "wall_sign")]
    WallSign,
    #[serde(rename = "hanging_sign")]
    HangingSign,
    #[serde(rename = "hanging_wall_sign")]
    HangingWallSign,

}