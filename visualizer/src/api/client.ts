import type {
  StatusResponse,
  HeightmapData,
  BlockMapData,
  BiomeMapData,
  DistrictMapData,
  BuildingsData,
  ClaimMapData,
  WorldSnapshot,
  LogEntry,
} from "./types";

const BASE_URL = `${window.location.protocol}//${window.location.hostname}:3000`;

async function fetchJson<T>(path: string): Promise<T | null> {
  try {
    const res = await fetch(`${BASE_URL}${path}`);
    if (!res.ok) return null;
    return await res.json();
  } catch {
    return null;
  }
}

export const api = {
  getStatus: () => fetchJson<StatusResponse>("/api/status"),
  getSnapshot: () => fetchJson<WorldSnapshot>("/api/snapshot"),
  getHeightmap: () => fetchJson<HeightmapData>("/api/heightmap"),
  getBlocks: () => fetchJson<BlockMapData>("/api/blocks"),
  getBiomes: () => fetchJson<BiomeMapData>("/api/biomes"),
  getDistricts: () => fetchJson<DistrictMapData>("/api/districts"),
  getBuildings: () => fetchJson<BuildingsData>("/api/buildings"),
  getClaims: () => fetchJson<ClaimMapData>("/api/claims"),
  getLogs: () => fetchJson<LogEntry[]>("/api/logs"),
  postGenerate: async (): Promise<boolean> => {
    try {
      const res = await fetch(`${BASE_URL}/api/generate`, { method: "POST" });
      return res.ok;
    } catch {
      return false;
    }
  },
  postRefresh: async (): Promise<boolean> => {
    try {
      const res = await fetch(`${BASE_URL}/api/refresh`, { method: "POST" });
      return res.ok;
    } catch {
      return false;
    }
  },
};
