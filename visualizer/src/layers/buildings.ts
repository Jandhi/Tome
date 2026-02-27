import type { BuildingsData } from "../api/types";

export function renderBuildings(
  data: BuildingsData,
  width: number,
  depth: number,
): ImageData {
  const imageData = new ImageData(depth, width);

  for (const building of data.buildings) {
    // Use a unique color per building based on ID
    const hue = (building.id * 137.508) % 360;
    const r = Math.round(180 + 75 * Math.cos((hue * Math.PI) / 180));
    const g = Math.round(180 + 75 * Math.cos(((hue - 120) * Math.PI) / 180));
    const b = Math.round(180 + 75 * Math.cos(((hue + 120) * Math.PI) / 180));

    for (const [fx, fz] of building.footprint) {
      if (fx >= 0 && fx < width && fz >= 0 && fz < depth) {
        const pixelIdx = (fx * depth + fz) * 4;
        imageData.data[pixelIdx] = r;
        imageData.data[pixelIdx + 1] = g;
        imageData.data[pixelIdx + 2] = b;
        imageData.data[pixelIdx + 3] = 200;
      }
    }
  }

  return imageData;
}
