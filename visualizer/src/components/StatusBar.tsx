import type { GenerationPhase } from "../api/types";

interface Props {
  phase: GenerationPhase;
  connected: boolean;
  mapSize: { width: number; depth: number } | null;
  onGenerate: () => void;
  onRefresh: () => void;
}

const PHASE_LABELS: Record<GenerationPhase, string> = {
  idle: "Waiting for generation...",
  refreshing: "Loading build area...",
  parcels: "Generating parcels",
  terrain: "Processing terrain",
  buildings: "Placing buildings",
  walls: "Building walls",
  flush: "Flushing blocks",
  chronicle: "Writing chronicle",
  done: "Generation complete",
  error: "Generation error",
};

export default function StatusBar({ phase, connected, mapSize, onGenerate, onRefresh }: Props) {
  const isActive = !["idle", "done", "error"].includes(phase);
  const canGenerate = phase === "idle" || phase === "done" || phase === "error";

  return (
    <div style={barStyle}>
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span
          style={{
            width: 8,
            height: 8,
            borderRadius: "50%",
            background: connected ? "#4caf50" : "#f44336",
            display: "inline-block",
          }}
        />
        <span style={{ fontSize: 13 }}>
          {PHASE_LABELS[phase]}
          {isActive && <span style={spinnerStyle}>...</span>}
        </span>
      </div>
      {mapSize && (
        <span style={{ fontSize: 11, color: "#888" }}>
          {mapSize.width} x {mapSize.depth}
        </span>
      )}
      <div style={{ display: "flex", gap: 6 }}>
        {canGenerate && (
          <button style={buttonStyle} onClick={onGenerate}>
            Generate
          </button>
        )}
        <button style={buttonStyle} onClick={onRefresh}>
          Refresh
        </button>
      </div>
    </div>
  );
}

const barStyle: React.CSSProperties = {
  position: "absolute",
  top: 10,
  left: 10,
  background: "rgba(20, 20, 40, 0.9)",
  color: "#eee",
  padding: "8px 14px",
  borderRadius: 8,
  display: "flex",
  alignItems: "center",
  gap: 16,
  backdropFilter: "blur(8px)",
  border: "1px solid rgba(255,255,255,0.1)",
};

const buttonStyle: React.CSSProperties = {
  background: "rgba(255, 255, 255, 0.1)",
  color: "#eee",
  border: "1px solid rgba(255, 255, 255, 0.2)",
  borderRadius: 4,
  padding: "4px 10px",
  fontSize: 12,
  cursor: "pointer",
};

const spinnerStyle: React.CSSProperties = {
  animation: "pulse 1.5s infinite",
};
