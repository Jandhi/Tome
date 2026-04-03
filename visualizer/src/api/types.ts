export type GenerationPhase =
  | "idle"
  | "refreshing"
  | "districts"
  | "terrain"
  | "buildings"
  | "walls"
  | "flush"
  | "chronicle"
  | "done"
  | "error";

export interface StatusResponse {
  phase: GenerationPhase;
  width: number;
  depth: number;
  origin_x: number;
  origin_z: number;
  error?: string;
}

export interface HeightmapData {
  width: number;
  depth: number;
  heights: number[];
  min_height: number;
  max_height: number;
}

export interface BlockMapData {
  width: number;
  depth: number;
  blocks: string[];
}

export interface BiomeMapData {
  width: number;
  depth: number;
  biomes: string[];
}

export interface DistrictInfo {
  id: number;
  district_type: string;
  is_border: boolean;
  size: number;
  origin_x: number;
  origin_z: number;
}

export interface DistrictMapData {
  width: number;
  depth: number;
  districts: number[];
  super_districts: number[];
  district_types: string[];
  district_info: DistrictInfo[];
}

export interface BuildingInfo {
  id: number;
  origin_x: number;
  origin_y: number;
  origin_z: number;
  footprint: [number, number][];
}

export interface BuildingsData {
  buildings: BuildingInfo[];
}

export interface ClaimMapData {
  width: number;
  depth: number;
  claims: string[];
}

export interface WorldSnapshot {
  phase: GenerationPhase;
  width: number;
  depth: number;
  origin_x: number;
  origin_z: number;
  heightmap: HeightmapData | null;
  blocks: BlockMapData | null;
  biomes: BiomeMapData | null;
  districts: DistrictMapData | null;
  buildings: BuildingsData | null;
  claims: ClaimMapData | null;
}

export interface LogEntry {
  timestamp: string;
  level: string;
  message: string;
}

export type VisualizerEvent =
  | { type: "phase_changed"; data: GenerationPhase }
  | { type: "snapshot_updated"; data: null }
  | { type: "log_message"; data: LogEntry };
