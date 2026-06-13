import { useState, useCallback } from "react";
import { api } from "../api/client";
import type {
  HeightmapData,
  BlockMapData,
  BiomeMapData,
  ParcelMapData,
  BuildingsData,
  ClaimMapData,
  StatusResponse,
} from "../api/types";

export interface MapData {
  status: StatusResponse | null;
  heightmap: HeightmapData | null;
  blocks: BlockMapData | null;
  biomes: BiomeMapData | null;
  parcels: ParcelMapData | null;
  buildings: BuildingsData | null;
  claims: ClaimMapData | null;
}

export function useMapData() {
  const [data, setData] = useState<MapData>({
    status: null,
    heightmap: null,
    blocks: null,
    biomes: null,
    parcels: null,
    buildings: null,
    claims: null,
  });
  const [loading, setLoading] = useState(false);

  const fetchAll = useCallback(async () => {
    setLoading(true);
    const [status, heightmap, blocks, biomes, parcels, buildings, claims] =
      await Promise.all([
        api.getStatus(),
        api.getHeightmap(),
        api.getBlocks(),
        api.getBiomes(),
        api.getParcels(),
        api.getBuildings(),
        api.getClaims(),
      ]);
    setData({ status, heightmap, blocks, biomes, parcels, buildings, claims });
    setLoading(false);
  }, []);

  return { data, loading, fetchAll };
}
