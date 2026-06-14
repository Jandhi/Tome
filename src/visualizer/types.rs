use serde_derive::Serialize;

/// Current generation phase
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GenerationPhase {
    Idle,
    Refreshing,
    Parcels,
    Terrain,
    Buildings,
    Walls,
    Flush,
    Chronicle,
    Done,
    Error,
}

/// WebSocket event sent to frontend
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "snake_case")]
pub enum VisualizerEvent {
    PhaseChanged(GenerationPhase),
    SnapshotUpdated,
    LogMessage(LogEntry),
}

/// A single log entry
#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
}

/// Status response for /api/status
#[derive(Debug, Clone, Serialize)]
pub struct StatusResponse {
    pub phase: GenerationPhase,
    pub width: usize,
    pub depth: usize,
    pub origin_x: i32,
    pub origin_z: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Heightmap data — flat row-major array [x * depth + z]
#[derive(Debug, Clone, Serialize)]
pub struct HeightmapData {
    pub width: usize,
    pub depth: usize,
    pub heights: Vec<i32>,
    pub min_height: i32,
    pub max_height: i32,
}

/// Biome map data — flat row-major, biome names as strings
#[derive(Debug, Clone, Serialize)]
pub struct BiomeMapData {
    pub width: usize,
    pub depth: usize,
    pub biomes: Vec<String>,
}

/// Parcel map data — flat row-major, None encoded as -1
#[derive(Debug, Clone, Serialize)]
pub struct ParcelMapData {
    pub width: usize,
    pub depth: usize,
    pub parcels: Vec<i32>,
    pub districts: Vec<i32>,
    pub parcel_types: Vec<String>,
    pub parcel_info: Vec<ParcelInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParcelInfo {
    pub id: usize,
    pub parcel_type: String,
    pub is_border: bool,
    pub size: usize,
    pub origin_x: i32,
    pub origin_z: i32,
}

/// Building info for the buildings endpoint
#[derive(Debug, Clone, Serialize)]
pub struct BuildingInfo {
    pub id: usize,
    pub origin_x: i32,
    pub origin_y: i32,
    pub origin_z: i32,
    pub footprint: Vec<[i32; 2]>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BuildingsData {
    pub buildings: Vec<BuildingInfo>,
}

/// Claim map data — flat row-major
#[derive(Debug, Clone, Serialize)]
pub struct ClaimMapData {
    pub width: usize,
    pub depth: usize,
    pub claims: Vec<String>,
}

/// Block map data — flat row-major, block IDs as strings
#[derive(Debug, Clone, Serialize)]
pub struct BlockMapData {
    pub width: usize,
    pub depth: usize,
    pub blocks: Vec<String>,
}

/// Full snapshot combining all layer data
#[derive(Debug, Clone, Serialize)]
pub struct WorldSnapshot {
    pub phase: GenerationPhase,
    pub width: usize,
    pub depth: usize,
    pub origin_x: i32,
    pub origin_z: i32,
    pub heightmap: Option<HeightmapData>,
    pub blocks: Option<BlockMapData>,
    pub biomes: Option<BiomeMapData>,
    pub parcels: Option<ParcelMapData>,
    pub buildings: Option<BuildingsData>,
    pub claims: Option<ClaimMapData>,
}
