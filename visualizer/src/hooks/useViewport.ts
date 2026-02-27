import { useState, useCallback, useRef } from "react";

export interface Viewport {
  offsetX: number;
  offsetY: number;
  zoom: number;
}

export function useViewport() {
  const [viewport, setViewport] = useState<Viewport>({
    offsetX: 0,
    offsetY: 0,
    zoom: 1,
  });

  const dragging = useRef(false);
  const lastPos = useRef({ x: 0, y: 0 });

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    dragging.current = true;
    lastPos.current = { x: e.clientX, y: e.clientY };
  }, []);

  const onMouseMove = useCallback((e: React.MouseEvent) => {
    if (!dragging.current) return;
    const dx = e.clientX - lastPos.current.x;
    const dy = e.clientY - lastPos.current.y;
    lastPos.current = { x: e.clientX, y: e.clientY };
    setViewport((v) => ({
      ...v,
      offsetX: v.offsetX + dx,
      offsetY: v.offsetY + dy,
    }));
  }, []);

  const onMouseUp = useCallback(() => {
    dragging.current = false;
  }, []);

  const onWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    const scaleBy = e.deltaY > 0 ? 0.9 : 1.1;
    setViewport((v) => {
      const newZoom = Math.max(0.1, Math.min(50, v.zoom * scaleBy));
      // Zoom toward mouse position
      const rect = (e.target as HTMLElement).getBoundingClientRect();
      const mouseX = e.clientX - rect.left;
      const mouseY = e.clientY - rect.top;
      return {
        zoom: newZoom,
        offsetX: mouseX - ((mouseX - v.offsetX) / v.zoom) * newZoom,
        offsetY: mouseY - ((mouseY - v.offsetY) / v.zoom) * newZoom,
      };
    });
  }, []);

  const resetViewport = useCallback((width: number, height: number, canvasW: number, canvasH: number) => {
    const scaleX = canvasW / width;
    const scaleY = canvasH / height;
    const zoom = Math.min(scaleX, scaleY) * 0.9;
    setViewport({
      offsetX: (canvasW - width * zoom) / 2,
      offsetY: (canvasH - height * zoom) / 2,
      zoom,
    });
  }, []);

  return {
    viewport,
    onMouseDown,
    onMouseMove,
    onMouseUp,
    onWheel,
    resetViewport,
  };
}
