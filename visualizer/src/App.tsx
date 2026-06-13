import { useState, useEffect, useCallback, useRef } from "react";
import MapCanvas, { type LayerConfig } from "./components/MapCanvas";
import LayerPanel from "./components/LayerPanel";
import InfoPanel from "./components/InfoPanel";
import StatusBar from "./components/StatusBar";
import { useViewport } from "./hooks/useViewport";
import { useMapData } from "./hooks/useMapData";
import { useWebSocket } from "./hooks/useWebSocket";
import BiomeLegend from "./components/BiomeLegend";
import ParcelLegend from "./components/ParcelLegend";
import LogPanel from "./components/LogPanel";
import { api } from "./api/client";

const DEFAULT_LAYERS: LayerConfig = {
  heightmap: { visible: true, opacity: 1.0 },
  biomes: { visible: false, opacity: 0.8 },
  parcels: { visible: true, opacity: 0.5 },
  buildings: { visible: true, opacity: 0.8 },
  claims: { visible: false, opacity: 0.6 },
};

export default function App() {
  const [layers, setLayers] = useState<LayerConfig>(DEFAULT_LAYERS);
  const [hoverPos, setHoverPos] = useState({ x: -1, z: -1 });
  const { data, fetchAll } = useMapData();
  const { viewport, onMouseDown, onMouseMove, onMouseUp, onWheel, resetViewport } =
    useViewport();

  const handleSnapshotUpdated = useCallback(() => {
    fetchAll();
  }, [fetchAll]);

  const { phase, connected, logs, clearLogs } = useWebSocket(handleSnapshotUpdated);

  // Auto-refresh from build area on page load
  const didAutoRefresh = useRef(false);
  useEffect(() => {
    if (!didAutoRefresh.current) {
      didAutoRefresh.current = true;
      api.postRefresh();
    }
  }, []);

  // Auto-fit viewport when data first loads
  useEffect(() => {
    if (data.status && data.status.width > 0) {
      resetViewport(
        data.status.depth,
        data.status.width,
        window.innerWidth,
        window.innerHeight,
      );
    }
  }, [data.status?.width, data.status?.depth]);

  const handleGenerate = useCallback(() => {
    api.postGenerate();
  }, []);

  const handleRefresh = useCallback(async () => {
    await api.postRefresh();
    fetchAll();
  }, [fetchAll]);

  const handleHover = useCallback((x: number, z: number) => {
    setHoverPos({ x, z });
  }, []);

  return (
    <div style={{ width: "100vw", height: "100vh", overflow: "hidden", position: "relative" }}>
      <MapCanvas
        data={data}
        viewport={viewport}
        layers={layers}
        onMouseDown={onMouseDown}
        onMouseMove={onMouseMove}
        onMouseUp={onMouseUp}
        onWheel={onWheel}
        onHover={handleHover}
      />
      <StatusBar
        phase={phase}
        connected={connected}
        mapSize={
          data.status
            ? { width: data.status.width, depth: data.status.depth }
            : null
        }
        onGenerate={handleGenerate}
        onRefresh={handleRefresh}
      />
      <LayerPanel layers={layers} onChange={setLayers} />
      <div style={{ position: "absolute", bottom: 10, right: 10, display: "flex", flexDirection: "column", gap: 8, alignItems: "flex-end" }}>
        {layers.biomes.visible && data.biomes && (
          <BiomeLegend biomes={data.biomes} />
        )}
        {layers.parcels.visible && <ParcelLegend />}
      </div>
      <InfoPanel data={data} hoverX={hoverPos.x} hoverZ={hoverPos.z} />
      <LogPanel logs={logs} onClear={clearLogs} />
    </div>
  );
}
