export type GenerationPhase =
  | "idle"
  | "refreshing"
  | "parcels"
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

export interface ParcelInfo {
  id: number;
  parcel_type: string;
  is_border: boolean;
  size: number;
  origin_x: number;
  origin_z: number;
}

export interface ParcelMapData {
  width: number;
  depth: number;
  parcels: number[];
  districts: number[];
  parcel_types: string[];
  parcel_info: ParcelInfo[];
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
  parcels: ParcelMapData | null;
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
