import { useMemo } from "react";
import type { BiomeMapData } from "../api/types";
import { biomeToColor } from "../utils/colors";

interface Props {
  biomes: BiomeMapData;
}

export default function BiomeLegend({ biomes }: Props) {
  const entries = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const b of biomes.biomes) {
      counts[b] = (counts[b] || 0) + 1;
    }
    return Object.entries(counts)
      .sort((a, b) => b[1] - a[1])
      .map(([name, count]) => ({
        name,
        count,
        color: biomeToColor(name),
      }));
  }, [biomes]);

  return (
    <div style={panelStyle}>
      <h3 style={{ margin: "0 0 8px", fontSize: 14 }}>Biomes</h3>
      <div style={{ maxHeight: 300, overflowY: "auto" }}>
        {entries.map(({ name, count, color }) => (
          <div key={name} style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 4 }}>
            <span
              style={{
                width: 12,
                height: 12,
                borderRadius: 2,
                flexShrink: 0,
                background: `rgb(${color[0]}, ${color[1]}, ${color[2]})`,
                border: "1px solid rgba(255,255,255,0.2)",
              }}
            />
            <span style={{ fontSize: 12, flex: 1 }}>{name}</span>
            <span style={{ fontSize: 10, color: "#888" }}>{count}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

const panelStyle: React.CSSProperties = {
  background: "rgba(20, 20, 40, 0.9)",
  color: "#eee",
  padding: "12px 16px",
  borderRadius: 8,
  minWidth: 180,
  backdropFilter: "blur(8px)",
  border: "1px solid rgba(255,255,255,0.1)",
};
