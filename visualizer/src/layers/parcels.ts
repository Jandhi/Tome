import type { ParcelMapData } from "../api/types";

const TYPE_COLORS: Record<string, [number, number, number]> = {
  Urban: [220, 180, 60],
  Rural: [60, 170, 80],
  OffLimits: [160, 60, 60],
  Unknown: [128, 128, 128],
};

function typeColor(dtype: string): [number, number, number] {
  return TYPE_COLORS[dtype] ?? TYPE_COLORS.Unknown;
}

export function renderParcels(data: ParcelMapData): ImageData {
  const { width, depth, parcels, districts, parcel_types } = data;
  const imageData = new ImageData(depth, width);

  // Precompute boundary info
  for (let x = 0; x < width; x++) {
    for (let z = 0; z < depth; z++) {
      const idx = x * depth + z;
      const did = parcels[idx];
      const sid = districts[idx];
      const dtype = parcel_types[idx];
      const [r, g, b] = typeColor(dtype);
      const pixelIdx = idx * 4;

      if (did < 0) {
        imageData.data[pixelIdx + 3] = 0;
        continue;
      }

      // Base fill color from parcel type
      imageData.data[pixelIdx] = r;
      imageData.data[pixelIdx + 1] = g;
      imageData.data[pixelIdx + 2] = b;
      imageData.data[pixelIdx + 3] = 120;

      // Check neighbors for boundaries
      const neighbors: [number, number][] = [
        [x - 1, z], [x + 1, z], [x, z - 1], [x, z + 1],
      ];

      let isSuperEdge = false;
      let isParcelEdge = false;

      for (const [nx, nz] of neighbors) {
        if (nx < 0 || nx >= width || nz < 0 || nz >= depth) {
          isSuperEdge = true;
          continue;
        }
        const nIdx = nx * depth + nz;
        const nSid = districts[nIdx];
        const nDid = parcels[nIdx];

        if (nSid !== sid) {
          isSuperEdge = true;
        } else if (nDid !== did) {
          isParcelEdge = true;
        }
      }

      if (isSuperEdge) {
        // Thick super-parcel boundary — also check diagonal neighbors for thicker lines
        imageData.data[pixelIdx] = 30;
        imageData.data[pixelIdx + 1] = 30;
        imageData.data[pixelIdx + 2] = 30;
        imageData.data[pixelIdx + 3] = 230;
      } else if (isParcelEdge) {
        // Thin parcel boundary — darken the type color
        imageData.data[pixelIdx] = Math.round(r * 0.4);
        imageData.data[pixelIdx + 1] = Math.round(g * 0.4);
        imageData.data[pixelIdx + 2] = Math.round(b * 0.4);
        imageData.data[pixelIdx + 3] = 180;
      }
    }
  }

  // Second pass: thicken super-parcel boundaries by marking pixels adjacent to them
  const superEdge = new Uint8Array(width * depth);
  for (let x = 0; x < width; x++) {
    for (let z = 0; z < depth; z++) {
      const idx = x * depth + z;
      const sid = districts[idx];
      const neighbors: [number, number][] = [
        [x - 1, z], [x + 1, z], [x, z - 1], [x, z + 1],
      ];
      for (const [nx, nz] of neighbors) {
        if (nx < 0 || nx >= width || nz < 0 || nz >= depth) {
          superEdge[idx] = 1;
          break;
        }
        if (districts[nx * depth + nz] !== sid) {
          superEdge[idx] = 1;
          break;
        }
      }
    }
  }

  // Dilate super-parcel edges by 1 pixel for thickness
  for (let x = 0; x < width; x++) {
    for (let z = 0; z < depth; z++) {
      const idx = x * depth + z;
      if (superEdge[idx]) continue;
      if (parcels[idx] < 0) continue;

      const neighbors: [number, number][] = [
        [x - 1, z], [x + 1, z], [x, z - 1], [x, z + 1],
      ];
      for (const [nx, nz] of neighbors) {
        if (nx < 0 || nx >= width || nz < 0 || nz >= depth) continue;
        if (superEdge[nx * depth + nz]) {
          const pixelIdx = idx * 4;
          imageData.data[pixelIdx] = 30;
          imageData.data[pixelIdx + 1] = 30;
          imageData.data[pixelIdx + 2] = 30;
          imageData.data[pixelIdx + 3] = 200;
          break;
        }
      }
    }
  }

  return imageData;
}
