import { useRef, useEffect, useMemo } from "react";
import type { MapData } from "../hooks/useMapData";
import type { Viewport } from "../hooks/useViewport";
import { renderHeightmap } from "../layers/heightmap";
import { renderBiomes } from "../layers/biomes";
import { renderParcels } from "../layers/parcels";
import { renderBuildings } from "../layers/buildings";
import { renderClaims } from "../layers/claims";

export interface LayerConfig {
  heightmap: { visible: boolean; opacity: number };
  biomes: { visible: boolean; opacity: number };
  parcels: { visible: boolean; opacity: number };
  buildings: { visible: boolean; opacity: number };
  claims: { visible: boolean; opacity: number };
}

interface Props {
  data: MapData;
  viewport: Viewport;
  layers: LayerConfig;
  onMouseDown: (e: React.MouseEvent) => void;
  onMouseMove: (e: React.MouseEvent) => void;
  onMouseUp: () => void;
  onWheel: (e: React.WheelEvent) => void;
  onHover: (x: number, z: number) => void;
}

export default function MapCanvas({
  data,
  viewport,
  layers,
  onMouseDown,
  onMouseMove,
  onMouseUp,
  onWheel,
  onHover,
}: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  // Pre-render each layer to offscreen canvases
  const offscreenLayers = useMemo(() => {
    const result: Record<string, HTMLCanvasElement> = {};
    const width = data.status?.width ?? 0;
    const depth = data.status?.depth ?? 0;
    if (width === 0 || depth === 0) return result;

    if (data.heightmap) {
      const c = document.createElement("canvas");
      c.width = depth;
      c.height = width;
      const ctx = c.getContext("2d")!;
      ctx.putImageData(renderHeightmap(data.heightmap, data.blocks), 0, 0);
      result.heightmap = c;
    }

    if (data.biomes) {
      const c = document.createElement("canvas");
      c.width = depth;
      c.height = width;
      const ctx = c.getContext("2d")!;
      ctx.putImageData(renderBiomes(data.biomes), 0, 0);
      result.biomes = c;
    }

    if (data.parcels) {
      const c = document.createElement("canvas");
      c.width = depth;
      c.height = width;
      const ctx = c.getContext("2d")!;
      ctx.putImageData(renderParcels(data.parcels), 0, 0);
      result.parcels = c;
    }

    if (data.buildings) {
      const c = document.createElement("canvas");
      c.width = depth;
      c.height = width;
      const ctx = c.getContext("2d")!;
      ctx.putImageData(renderBuildings(data.buildings, width, depth), 0, 0);
      result.buildings = c;
    }

    if (data.claims) {
      const c = document.createElement("canvas");
      c.width = depth;
      c.height = width;
      const ctx = c.getContext("2d")!;
      ctx.putImageData(renderClaims(data.claims), 0, 0);
      result.claims = c;
    }

    return result;
  }, [data]);

  // Composite visible layers
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    canvas.width = canvas.clientWidth;
    canvas.height = canvas.clientHeight;

    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.fillStyle = "#1a1a2e";
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    ctx.save();
    ctx.translate(viewport.offsetX, viewport.offsetY);
    ctx.scale(viewport.zoom, viewport.zoom);

    // Draw layers in order
    const layerOrder: (keyof LayerConfig)[] = [
      "heightmap",
      "biomes",
      "parcels",
      "claims",
      "buildings",
    ];

    for (const name of layerOrder) {
      const cfg = layers[name];
      if (!cfg.visible || !offscreenLayers[name]) continue;
      ctx.globalAlpha = cfg.opacity;
      ctx.imageSmoothingEnabled = false;
      ctx.drawImage(offscreenLayers[name], 0, 0);
    }

    ctx.globalAlpha = 1;
    ctx.restore();
  }, [viewport, layers, offscreenLayers]);

  const handleMouseMove = (e: React.MouseEvent) => {
    onMouseMove(e);
    // Calculate map coordinates under cursor
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const mapX = Math.floor((mx - viewport.offsetX) / viewport.zoom);
    const mapZ = Math.floor((my - viewport.offsetY) / viewport.zoom);
    onHover(mapZ, mapX); // x=row, z=col in our layout
  };

  return (
    <canvas
      ref={canvasRef}
      style={{ width: "100%", height: "100%", cursor: "grab" }}
      onMouseDown={onMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={onMouseUp}
      onMouseLeave={onMouseUp}
      onWheel={onWheel}
    />
  );
}
