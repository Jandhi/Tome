import type { BiomeMapData } from "../api/types";
import { biomeToColor } from "../utils/colors";

export function renderBiomes(data: BiomeMapData): ImageData {
  const { width, depth, biomes } = data;
  const imageData = new ImageData(depth, width);

  for (let x = 0; x < width; x++) {
    for (let z = 0; z < depth; z++) {
      const idx = x * depth + z;
      const [r, g, b] = biomeToColor(biomes[idx]);
      const pixelIdx = idx * 4;
      imageData.data[pixelIdx] = r;
      imageData.data[pixelIdx + 1] = g;
      imageData.data[pixelIdx + 2] = b;
      imageData.data[pixelIdx + 3] = 255;
    }
  }

  return imageData;
}
