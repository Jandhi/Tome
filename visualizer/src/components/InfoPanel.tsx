import type { MapData } from "../hooks/useMapData";

interface Props {
  data: MapData;
  hoverX: number;
  hoverZ: number;
}

export default function InfoPanel({ data, hoverX, hoverZ }: Props) {
  const width = data.status?.width ?? 0;
  const depth = data.status?.depth ?? 0;

  if (hoverX < 0 || hoverZ < 0 || hoverX >= width || hoverZ >= depth) {
    return (
      <div style={panelStyle}>
        <span style={{ fontSize: 12, color: "#888" }}>
          Hover over the map for details
        </span>
      </div>
    );
  }

  const idx = hoverX * depth + hoverZ;
  const worldX = hoverX + (data.status?.origin_x ?? 0);
  const worldZ = hoverZ + (data.status?.origin_z ?? 0);

  const height = data.heightmap?.heights[idx] ?? "?";
  const biome = data.biomes?.biomes[idx] ?? "?";
  const parcel = data.parcels?.parcels[idx] ?? -1;
  const parcelType = data.parcels?.parcel_types[idx] ?? "";
  const claim = data.claims?.claims[idx] ?? "?";

  return (
    <div style={panelStyle}>
      <div style={{ fontSize: 11, color: "#888", marginBottom: 4 }}>
        Local: ({hoverX}, {hoverZ}) | World: ({worldX}, {worldZ})
      </div>
      <div style={rowStyle}>
        <span>Height:</span> <span>{height}</span>
      </div>
      <div style={rowStyle}>
        <span>Biome:</span> <span>{biome}</span>
      </div>
      <div style={rowStyle}>
        <span>Parcel:</span>{" "}
        <span>{parcel >= 0 ? `#${parcel} (${parcelType})` : "none"}</span>
      </div>
      <div style={rowStyle}>
        <span>Claim:</span> <span>{claim}</span>
      </div>
    </div>
  );
}

const panelStyle: React.CSSProperties = {
  position: "absolute",
  bottom: 10,
  left: 10,
  background: "rgba(20, 20, 40, 0.9)",
  color: "#eee",
  padding: "10px 14px",
  borderRadius: 8,
  minWidth: 200,
  backdropFilter: "blur(8px)",
  border: "1px solid rgba(255,255,255,0.1)",
  fontSize: 13,
};

const rowStyle: React.CSSProperties = {
  display: "flex",
  justifyContent: "space-between",
  gap: 12,
  marginBottom: 2,
};
