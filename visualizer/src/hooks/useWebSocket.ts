import { useEffect, useRef, useState, useCallback } from "react";
import { createWebSocket } from "../api/websocket";
import type { GenerationPhase, LogEntry } from "../api/types";
import { api } from "../api/client";

export function useWebSocket(onSnapshotUpdated: () => void) {
  const [phase, setPhase] = useState<GenerationPhase>("idle");
  const [connected, setConnected] = useState(false);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const cleanupRef = useRef<(() => void) | null>(null);

  // Fetch existing logs on mount
  useEffect(() => {
    api.getLogs().then((existing) => {
      if (existing) setLogs(existing);
    });
  }, []);

  useEffect(() => {
    const cleanup = createWebSocket((event) => {
      setConnected(true);
      if (event.type === "phase_changed") {
        setPhase(event.data);
      } else if (event.type === "snapshot_updated") {
        onSnapshotUpdated();
      } else if (event.type === "log_message") {
        setLogs((prev) => {
          const next = [...prev, event.data];
          return next.length > 200 ? next.slice(-200) : next;
        });
      }
    });
    cleanupRef.current = cleanup;

    return () => {
      cleanup();
      cleanupRef.current = null;
    };
  }, [onSnapshotUpdated]);

  const clearLogs = useCallback(() => setLogs([]), []);

  return { phase, connected, logs, clearLogs };
}
