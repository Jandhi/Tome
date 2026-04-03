import type { HeightmapData, BlockMapData } from "../api/types";
import { blockToColor } from "../utils/colors";

export function renderHeightmap(
  data: HeightmapData,
  blocks: BlockMapData | null,
): ImageData {
  const { width, depth, heights, min_height, max_height } = data;
  const imageData = new ImageData(depth, width);
  const range = max_height - min_height || 1;

  for (let x = 0; x < width; x++) {
    for (let z = 0; z < depth; z++) {
      const idx = x * depth + z;
      const normalized = (heights[idx] - min_height) / range;

      // Block color as base, height as brightness modifier
      const blockId = blocks?.blocks[idx] ?? "";
      const [br, bg, bb] = blockToColor(blockId);

      // Shade: map normalized height to a brightness multiplier (0.5 – 1.2)
      const shade = 0.5 + normalized * 0.7;

      const pixelIdx = idx * 4;
      imageData.data[pixelIdx] = Math.min(255, Math.round(br * shade));
      imageData.data[pixelIdx + 1] = Math.min(255, Math.round(bg * shade));
      imageData.data[pixelIdx + 2] = Math.min(255, Math.round(bb * shade));
      imageData.data[pixelIdx + 3] = 255;
    }
  }

  return imageData;
}
