import { useEffect, useRef, useState } from "react";
import type { LogEntry } from "../api/types";

interface Props {
  logs: LogEntry[];
  onClear: () => void;
}

const LEVEL_COLORS: Record<string, string> = {
  info: "#8cb4ff",
  warn: "#ffc107",
  error: "#f44336",
  debug: "#888",
};

export default function LogPanel({ logs, onClear }: Props) {
  const [open, setOpen] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open && bottomRef.current) {
      bottomRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [logs.length, open]);

  return (
    <div style={containerStyle}>
      <button style={toggleStyle} onClick={() => setOpen(!open)}>
        Logs ({logs.length}) {open ? "\u25BC" : "\u25B2"}
      </button>
      {open && (
        <div style={panelStyle}>
          <div style={headerStyle}>
            <span style={{ fontSize: 11, color: "#888" }}>Server Logs</span>
            <button style={clearStyle} onClick={onClear}>
              Clear
            </button>
          </div>
          <div style={scrollStyle}>
            {logs.length === 0 && (
              <div style={{ color: "#666", fontSize: 11, padding: 8 }}>
                No logs yet
              </div>
            )}
            {logs.map((entry, i) => (
              <div key={i} style={entryStyle}>
                <span style={{ color: "#666", fontSize: 10, marginRight: 6 }}>
                  {entry.timestamp}
                </span>
                <span
                  style={{
                    color: LEVEL_COLORS[entry.level] ?? "#ccc",
                    fontSize: 10,
                    fontWeight: 600,
                    marginRight: 6,
                    textTransform: "uppercase",
                    minWidth: 36,
                    display: "inline-block",
                  }}
                >
                  {entry.level}
                </span>
                <span style={{ color: "#ddd", fontSize: 11 }}>
                  {entry.message}
                </span>
              </div>
            ))}
            <div ref={bottomRef} />
          </div>
        </div>
      )}
    </div>
  );
}

const containerStyle: React.CSSProperties = {
  position: "absolute",
  bottom: 10,
  left: 10,
  right: 10,
  zIndex: 20,
  pointerEvents: "none",
};

const toggleStyle: React.CSSProperties = {
  pointerEvents: "auto",
  background: "rgba(20, 20, 40, 0.9)",
  color: "#eee",
  border: "1px solid rgba(255,255,255,0.1)",
  borderRadius: "6px 6px 0 0",
  padding: "4px 12px",
  fontSize: 11,
  cursor: "pointer",
  backdropFilter: "blur(8px)",
};

const panelStyle: React.CSSProperties = {
  pointerEvents: "auto",
  background: "rgba(10, 10, 25, 0.95)",
  border: "1px solid rgba(255,255,255,0.1)",
  borderRadius: "0 6px 6px 6px",
  backdropFilter: "blur(8px)",
  overflow: "hidden",
};

const headerStyle: React.CSSProperties = {
  display: "flex",
  justifyContent: "space-between",
  alignItems: "center",
  padding: "4px 8px",
  borderBottom: "1px solid rgba(255,255,255,0.05)",
};

const clearStyle: React.CSSProperties = {
  background: "none",
  border: "none",
  color: "#666",
  fontSize: 10,
  cursor: "pointer",
};

const scrollStyle: React.CSSProperties = {
  maxHeight: 200,
  overflowY: "auto",
  fontFamily: "monospace",
};

const entryStyle: React.CSSProperties = {
  padding: "2px 8px",
  borderBottom: "1px solid rgba(255,255,255,0.03)",
  whiteSpace: "nowrap",
};
