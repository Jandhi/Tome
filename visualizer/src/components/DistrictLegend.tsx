const TYPE_COLORS: Record<string, [number, number, number]> = {
  Urban: [220, 180, 60],
  Rural: [60, 170, 80],
  OffLimits: [160, 60, 60],
  Unknown: [128, 128, 128],
};

const BOUNDARY_ENTRIES = [
  { label: "Super-district boundary", color: [30, 30, 30] as [number, number, number] },
];

export default function DistrictLegend() {
  return (
    <div style={panelStyle}>
      <h3 style={{ margin: "0 0 8px", fontSize: 14 }}>Districts</h3>
      {Object.entries(TYPE_COLORS).map(([name, color]) => (
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
          <span style={{ fontSize: 12 }}>{name}</span>
        </div>
      ))}
      <div style={{ borderTop: "1px solid rgba(255,255,255,0.1)", margin: "6px 0" }} />
      {BOUNDARY_ENTRIES.map(({ label, color }) => (
        <div key={label} style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 4 }}>
          <span
            style={{
              width: 12,
              height: 4,
              flexShrink: 0,
              background: `rgb(${color[0]}, ${color[1]}, ${color[2]})`,
              borderRadius: 1,
            }}
          />
          <span style={{ fontSize: 12 }}>{label}</span>
        </div>
      ))}
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
