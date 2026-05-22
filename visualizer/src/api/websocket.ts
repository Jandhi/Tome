import type { VisualizerEvent } from "./types";

export type EventHandler = (event: VisualizerEvent) => void;

export function createWebSocket(onEvent: EventHandler): () => void {
  const wsUrl = `ws://${window.location.hostname}:3000/ws`;
  let ws: WebSocket | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let closed = false;

  function connect() {
    if (closed) return;
    ws = new WebSocket(wsUrl);

    ws.onmessage = (msg) => {
      try {
        const event: VisualizerEvent = JSON.parse(msg.data);
        onEvent(event);
      } catch {
        // ignore malformed messages
      }
    };

    ws.onclose = () => {
      if (!closed) {
        reconnectTimer = setTimeout(connect, 2000);
      }
    };

    ws.onerror = () => {
      ws?.close();
    };
  }

  connect();

  // Return cleanup function
  return () => {
    closed = true;
    if (reconnectTimer) clearTimeout(reconnectTimer);
    ws?.close();
  };
}
