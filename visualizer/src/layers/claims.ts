import type { ClaimMapData } from "../api/types";
import { CLAIM_COLORS } from "../utils/colors";

export function renderClaims(data: ClaimMapData): ImageData {
  const { width, depth, claims } = data;
  const imageData = new ImageData(depth, width);

  for (let x = 0; x < width; x++) {
    for (let z = 0; z < depth; z++) {
      const idx = x * depth + z;
      const claim = claims[idx];
      const color = CLAIM_COLORS[claim] ?? [0, 0, 0, 0];
      const pixelIdx = idx * 4;
      imageData.data[pixelIdx] = color[0];
      imageData.data[pixelIdx + 1] = color[1];
      imageData.data[pixelIdx + 2] = color[2];
      imageData.data[pixelIdx + 3] = color[3];
    }
  }

  return imageData;
}
