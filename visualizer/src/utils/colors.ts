/** Interpolate between two RGB colors. t in [0, 1] */
function lerpColor(
  a: [number, number, number],
  b: [number, number, number],
  t: number,
): [number, number, number] {
  return [
    Math.round(a[0] + (b[0] - a[0]) * t),
    Math.round(a[1] + (b[1] - a[1]) * t),
    Math.round(a[2] + (b[2] - a[2]) * t),
  ];
}

/** Height gradient: deep water (blue) -> shore (sand) -> grass (green) -> mountain (brown) -> snow (white) */
const HEIGHT_STOPS: { t: number; color: [number, number, number] }[] = [
  { t: 0.0, color: [30, 60, 150] },   // deep water
  { t: 0.2, color: [60, 120, 200] },   // shallow water
  { t: 0.3, color: [194, 178, 128] },  // sand/shore
  { t: 0.4, color: [34, 139, 34] },    // grass
  { t: 0.6, color: [0, 100, 0] },      // forest green
  { t: 0.8, color: [139, 90, 43] },    // mountain brown
  { t: 1.0, color: [255, 255, 255] },  // snow
];

export function heightToColor(normalizedHeight: number): [number, number, number] {
  const t = Math.max(0, Math.min(1, normalizedHeight));
  for (let i = 1; i < HEIGHT_STOPS.length; i++) {
    if (t <= HEIGHT_STOPS[i].t) {
      const segT =
        (t - HEIGHT_STOPS[i - 1].t) /
        (HEIGHT_STOPS[i].t - HEIGHT_STOPS[i - 1].t);
      return lerpColor(HEIGHT_STOPS[i - 1].color, HEIGHT_STOPS[i].color, segT);
    }
  }
  return HEIGHT_STOPS[HEIGHT_STOPS.length - 1].color;
}

/** Golden-angle hue distribution for district coloring */
export function districtColor(id: number): [number, number, number] {
  const hue = (id * 137.508) % 360;
  return hslToRgb(hue, 0.6, 0.55);
}

function hslToRgb(h: number, s: number, l: number): [number, number, number] {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = l - c / 2;
  let r = 0, g = 0, b = 0;
  if (h < 60) { r = c; g = x; }
  else if (h < 120) { r = x; g = c; }
  else if (h < 180) { g = c; b = x; }
  else if (h < 240) { g = x; b = c; }
  else if (h < 300) { r = x; b = c; }
  else { r = c; b = x; }
  return [
    Math.round((r + m) * 255),
    Math.round((g + m) * 255),
    Math.round((b + m) * 255),
  ];
}

/** Biome color map — keys are snake_case names (without minecraft: prefix) */
const BIOME_COLORS: Record<string, [number, number, number]> = {
  // Oceans
  ocean: [30, 60, 180],
  deep_ocean: [20, 40, 140],
  warm_ocean: [50, 100, 200],
  lukewarm_ocean: [40, 80, 190],
  cold_ocean: [25, 50, 160],
  frozen_ocean: [140, 170, 220],
  deep_warm_ocean: [40, 80, 170],
  deep_lukewarm_ocean: [30, 60, 160],
  deep_cold_ocean: [15, 35, 130],
  deep_frozen_ocean: [120, 150, 200],
  // Rivers & shores
  river: [50, 90, 210],
  frozen_river: [140, 180, 230],
  beach: [220, 210, 160],
  snowy_beach: [230, 225, 220],
  stone_shore: [140, 140, 140],
  stony_shore: [140, 140, 140],
  // Plains
  plains: [100, 180, 60],
  sunflower_plains: [120, 190, 50],
  snowy_plains: [220, 230, 240],
  snowy_tundra: [220, 230, 240],
  // Forests
  forest: [30, 120, 30],
  flower_forest: [80, 150, 60],
  birch_forest: [60, 150, 60],
  birch_forest_hills: [55, 145, 55],
  tall_birch_forest: [55, 145, 55],
  tall_birch_hills: [50, 140, 50],
  dark_forest: [20, 80, 20],
  dark_forest_hills: [25, 85, 25],
  old_growth_birch_forest: [55, 145, 55],
  snowy_forest: [180, 200, 210],
  // Taiga
  taiga: [40, 100, 50],
  taiga_hills: [35, 95, 45],
  taiga_mountains: [30, 90, 40],
  snowy_taiga: [50, 110, 70],
  snowy_taiga_hills: [45, 105, 65],
  snowy_taiga_mountains: [40, 100, 60],
  giant_tree_taiga: [35, 90, 45],
  giant_tree_taiga_hills: [30, 85, 40],
  giant_spruce_taiga: [30, 85, 40],
  giant_spruce_taiga_hills: [25, 80, 35],
  old_growth_pine_taiga: [35, 90, 45],
  old_growth_spruce_taiga: [30, 85, 40],
  // Jungle
  jungle: [50, 150, 20],
  jungle_hills: [45, 140, 18],
  jungle_edge: [55, 145, 25],
  modified_jungle: [48, 148, 22],
  modified_jungle_edge: [52, 142, 24],
  sparse_jungle: [60, 140, 30],
  bamboo_jungle: [70, 160, 30],
  bamboo_jungle_hills: [65, 155, 28],
  // Desert
  desert: [220, 200, 130],
  desert_hills: [210, 190, 120],
  desert_lakes: [200, 185, 125],
  // Badlands
  badlands: [200, 120, 50],
  badlands_plateau: [195, 115, 48],
  wooded_badlands: [180, 110, 40],
  wooded_badlands_plateau: [180, 110, 40],
  eroded_badlands: [190, 110, 45],
  modified_badlands_plateau: [192, 118, 48],
  modified_wooded_badlands_plateau: [178, 108, 42],
  // Savanna
  savanna: [170, 180, 60],
  savanna_plateau: [160, 170, 55],
  windswept_savanna: [155, 165, 50],
  shattered_savanna: [155, 165, 50],
  shattered_savanna_plateau: [150, 160, 48],
  // Swamp
  swamp: [50, 80, 40],
  swamp_hills: [45, 75, 38],
  mangrove_swamp: [40, 70, 35],
  // Mountains & hills
  mountains: [130, 130, 130],
  snowy_mountains: [210, 220, 230],
  wooded_mountains: [80, 110, 70],
  wooded_hills: [40, 110, 40],
  mountain_edge: [120, 125, 120],
  gravelly_mountains: [110, 110, 110],
  modified_gravelly_mountains: [105, 105, 105],
  windswept_hills: [130, 130, 130],
  windswept_forest: [80, 110, 70],
  windswept_gravelly_hills: [110, 110, 110],
  // Meadow & peaks
  meadow: [100, 190, 80],
  grove: [60, 130, 80],
  snowy_slopes: [200, 210, 220],
  frozen_peaks: [180, 200, 230],
  jagged_peaks: [170, 170, 190],
  stony_peaks: [150, 150, 160],
  // Special
  cherry_grove: [200, 140, 170],
  pale_garden: [190, 185, 175],
  dripstone_caves: [120, 100, 80],
  lush_caves: [50, 130, 50],
  deep_dark: [15, 15, 25],
  mushroom_fields: [160, 100, 160],
  mushroom_field_shore: [155, 95, 155],
  ice_spikes: [160, 200, 230],
  // Nether
  nether_wastes: [97, 38, 38],
  soul_sand_valley: [81, 62, 50],
  crimson_forest: [130, 20, 30],
  warped_forest: [20, 100, 100],
  basalt_deltas: [70, 70, 75],
  // End
  the_end: [60, 50, 80],
  small_end_islands: [55, 45, 75],
  end_midlands: [65, 55, 85],
  end_highlands: [70, 60, 90],
  end_barrens: [50, 40, 70],
};

export function biomeToColor(biome: string): [number, number, number] {
  return BIOME_COLORS[biome] ?? [128, 128, 128];
}

/** Minecraft block color map — surface blocks only */
const BLOCK_COLORS: Record<string, [number, number, number]> = {
  // Air — typically above grass, use grass color as fallback
  air: [90, 150, 60],
  cave_air: [90, 150, 60],
  void_air: [20, 20, 20],

  // Grass & dirt
  grass_block: [90, 150, 60],
  dirt: [134, 96, 67],
  coarse_dirt: [119, 85, 59],
  rooted_dirt: [115, 82, 56],
  podzol: [91, 63, 24],
  mycelium: [111, 99, 107],
  mud: [60, 52, 45],
  farmland: [110, 78, 50],
  dirt_path: [148, 121, 65],

  // Sand & gravel
  sand: [219, 207, 163],
  red_sand: [190, 102, 33],
  gravel: [131, 127, 126],
  clay: [160, 166, 179],
  soul_sand: [81, 62, 50],
  soul_soil: [75, 57, 46],

  // Stone types
  stone: [125, 125, 125],
  cobblestone: [120, 120, 120],
  deepslate: [80, 80, 82],
  granite: [149, 103, 85],
  diorite: [188, 182, 183],
  andesite: [136, 136, 136],
  tuff: [108, 109, 102],
  calcite: [223, 224, 220],
  dripstone_block: [134, 107, 92],
  smooth_basalt: [72, 72, 78],
  basalt: [80, 80, 84],

  // Water & ice
  water: [55, 100, 190],
  flowing_water: [55, 100, 190],
  ice: [145, 183, 255],
  packed_ice: [130, 170, 248],
  blue_ice: [100, 150, 240],
  frosted_ice: [155, 193, 255],

  // Snow
  snow: [240, 250, 255],
  snow_block: [240, 250, 255],
  powder_snow: [230, 240, 248],

  // Wood (logs)
  oak_log: [109, 85, 51],
  spruce_log: [58, 37, 16],
  birch_log: [216, 210, 193],
  jungle_log: [85, 67, 25],
  acacia_log: [103, 96, 86],
  dark_oak_log: [60, 46, 26],
  cherry_log: [53, 26, 33],
  mangrove_log: [84, 56, 30],

  // Leaves
  oak_leaves: [53, 120, 25],
  spruce_leaves: [40, 80, 40],
  birch_leaves: [70, 130, 50],
  jungle_leaves: [45, 130, 15],
  acacia_leaves: [60, 120, 30],
  dark_oak_leaves: [35, 90, 20],
  cherry_leaves: [220, 160, 180],
  mangrove_leaves: [50, 110, 30],
  azalea_leaves: [60, 120, 35],
  flowering_azalea_leaves: [75, 115, 45],

  // Plants & flowers
  short_grass: [85, 145, 55],
  tall_grass: [80, 140, 50],
  fern: [70, 130, 45],
  large_fern: [65, 125, 40],
  dead_bush: [120, 90, 45],
  seagrass: [30, 90, 50],
  tall_seagrass: [25, 85, 45],
  kelp: [50, 110, 55],
  kelp_plant: [45, 105, 50],
  lily_pad: [30, 100, 30],
  moss_block: [80, 120, 40],
  moss_carpet: [80, 120, 40],

  // Flowers — show as grass-ish, they sit on grass
  dandelion: [95, 155, 60],
  poppy: [95, 145, 55],
  blue_orchid: [85, 150, 65],
  allium: [90, 145, 60],
  azure_bluet: [90, 150, 60],
  red_tulip: [90, 148, 58],
  orange_tulip: [90, 148, 58],
  white_tulip: [90, 148, 58],
  pink_tulip: [90, 148, 58],
  oxeye_daisy: [92, 152, 60],
  cornflower: [88, 148, 62],
  lily_of_the_valley: [90, 150, 60],
  sunflower: [95, 155, 60],
  lilac: [90, 150, 60],
  rose_bush: [90, 145, 55],
  peony: [90, 150, 60],
  bush: [55, 120, 35],
  firefly_bush: [60, 125, 40],

  // Terracotta
  terracotta: [152, 94, 67],
  red_terracotta: [143, 61, 46],
  orange_terracotta: [161, 83, 37],
  yellow_terracotta: [186, 133, 35],
  brown_terracotta: [77, 51, 35],
  white_terracotta: [209, 178, 161],
  light_gray_terracotta: [135, 106, 97],

  // Ores & special
  coal_ore: [105, 105, 105],
  iron_ore: [136, 129, 122],
  copper_ore: [124, 125, 120],
  gold_ore: [145, 133, 106],
  lapis_ore: [100, 110, 140],

  // Misc
  obsidian: [15, 10, 24],
  crying_obsidian: [30, 10, 50],
  bedrock: [85, 85, 85],
  netherrack: [97, 38, 38],
  end_stone: [219, 222, 158],
  magma_block: [140, 60, 20],
  lava: [207, 92, 15],
  flowing_lava: [207, 92, 15],

  // Coral
  bubble_column: [55, 100, 190],
};

export function blockToColor(block: string): [number, number, number] {
  // Strip "minecraft:" prefix if present
  const id = block.startsWith("minecraft:") ? block.slice(10) : block;
  return BLOCK_COLORS[id] ?? [128, 128, 128];
}

/** Claim type colors */
export const CLAIM_COLORS: Record<string, [number, number, number, number]> = {
  none: [0, 0, 0, 0],
  nature: [34, 139, 34, 100],
  wall: [139, 69, 19, 180],
  gate: [218, 165, 32, 200],
  path: [180, 160, 120, 150],
  building: [180, 50, 50, 160],
};
