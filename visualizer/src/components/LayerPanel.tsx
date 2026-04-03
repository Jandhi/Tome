import type { LayerConfig } from "./MapCanvas";

interface Props {
  layers: LayerConfig;
  onChange: (layers: LayerConfig) => void;
}

const LAYER_NAMES: { key: keyof LayerConfig; label: string }[] = [
  { key: "heightmap", label: "Heightmap" },
  { key: "biomes", label: "Biomes" },
  { key: "districts", label: "Districts" },
  { key: "buildings", label: "Buildings" },
  { key: "claims", label: "Claims" },
];

export default function LayerPanel({ layers, onChange }: Props) {
  const toggle = (key: keyof LayerConfig) => {
    onChange({
      ...layers,
      [key]: { ...layers[key], visible: !layers[key].visible },
    });
  };

  const setOpacity = (key: keyof LayerConfig, opacity: number) => {
    onChange({
      ...layers,
      [key]: { ...layers[key], opacity },
    });
  };

  return (
    <div style={panelStyle}>
      <h3 style={{ margin: "0 0 8px", fontSize: 14 }}>Layers</h3>
      {LAYER_NAMES.map(({ key, label }) => (
        <div key={key} style={{ marginBottom: 8 }}>
          <label style={{ display: "flex", alignItems: "center", gap: 6, cursor: "pointer" }}>
            <input
              type="checkbox"
              checked={layers[key].visible}
              onChange={() => toggle(key)}
            />
            <span style={{ fontSize: 13, minWidth: 70 }}>{label}</span>
          </label>
          {layers[key].visible && (
            <input
              type="range"
              min={0}
              max={1}
              step={0.05}
              value={layers[key].opacity}
              onChange={(e) => setOpacity(key, parseFloat(e.target.value))}
              style={{ width: "100%", marginTop: 2 }}
            />
          )}
        </div>
      ))}
    </div>
  );
}

const panelStyle: React.CSSProperties = {
  position: "absolute",
  top: 10,
  right: 10,
  background: "rgba(20, 20, 40, 0.9)",
  color: "#eee",
  padding: "12px 16px",
  borderRadius: 8,
  minWidth: 160,
  backdropFilter: "blur(8px)",
  border: "1px solid rgba(255,255,255,0.1)",
};
