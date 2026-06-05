# Production Area — Implementation Summary

Raw resource buildings (farm, woodcutter_hut, iron_mine) are placed as NBT structures. After each placement, a **production area painter** fills the remaining unclaimed space in the building's rural super-district with resource-appropriate terrain.

---

## System Overview

The painter for each raw resource is declared as a `production_painter` field on its gather recipe in `recipes.yaml`. The painter itself is defined in `data/resource_chains/production_painters.yaml`. The painter name flows from the recipe → `DistrictResourceAssignment` → `paint_production_area()` at the call site.

```
recipes.yaml
  gather_wheat  ──production_painter──▶  "wheat_fields"
  gather_wood   ──production_painter──▶  "logging_area"
  gather_iron   ──production_painter──▶  "mine_terrain"

production_painters.yaml
  wheat_fields:  { type: palettes, palettes: [farmland, wheat], irrigation: true, flatten_strength: 0.6 }
  logging_area:  { type: logging,  percent: 0.7, stump: oak_log }
  mine_terrain:  { type: palettes, palettes: [mine_ground] }

data/paint_palettes/palettes.yaml
  farmland, wheat, young_wheat, carrot, potato, beetroot,
  rural_road, rural_road_wet, mine_ground
```

---

## `PaintPalette`

A named, weighted list of block strings used to paint terrain. Defined in `src/generator/districts/paint_palette.rs`, exported from `src/generator/districts/mod.rs`.

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `palette` | `HashMap<String, f32>` | Block string → relative weight. Parsed by `string_to_block`. |
| `smooth` | `bool` | Reserved for road/path use; unused in production area painting. |
| `tags` | `Vec<String>` (optional) | Behaviour modifiers — see below. |

### Tag semantics

| Tag | Effect |
|-----|--------|
| `"crops"` | Placed at `height_offset = Some(1)` — one block above the surface, on top of a ground palette. |
| `"farmland"` | Informational only. |

No tag = ground-level replacement (`height_offset = None`).

### Rust types

```rust
pub struct PaintPaletteId(pub String);

pub struct PaintPalette {
    pub palette: HashMap<String, f32>,
    #[serde(default)]
    pub smooth: bool,
    pub tags: Option<Vec<String>>,
}
```

`to_weighted_blocks()` converts the palette to the `(HashMap<usize, f32>, Vec<Block>)` form expected by `replace_ground`. Loaded via `load_yaml` (single consolidated YAML file), not the `Loadable` directory-scan trait. Stored in `LoadedData::paint_palettes: HashMap<PaintPaletteId, PaintPalette>`.

---

## `ProductionPainter`

Defined in `src/generator/resource_chain/production_painter.rs`. Two variants are implemented:

**`type: palettes`** — calls `replace_ground` for each palette in order. Ground palettes land before crop palettes (ordering in YAML matters). Supports optional irrigation channels and terrain smoothing.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `palettes` | `Vec<String>` | required | PaintPalette names, applied in order. |
| `irrigation` | `bool` | `false` | When true, cells on a random axis/offset stripe become water channels instead of receiving the ground palette. |
| `flatten_strength` | `f32` | `0.0` | `0.0` = no smoothing; `1.0` = 5 passes via `smooth_terrain`. |

**`type: logging`** — fells a random percentage of existing trees in the district and leaves stumps. Does not plant anything or smooth terrain.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `percent` | `f32` | required | Fraction of tree-topped cells to fell, `0.0–1.0`. |
| `stump` | `String` | `"oak_log"` | Block placed at the base of each felled tree. |

`ProductionPainter` is owned by `ResourceRegistry` (loaded and validated internally), not stored separately in `LoadedData`. `ResourceRegistry::load()` calls `validate_painters()` after loading, which checks that every gather recipe's painter name exists in `production_painters.yaml`.

---

## `paint_production_area`

Located in `src/generator/resource_chain/production_area.rs`, exported via `src/generator/resource_chain/mod.rs`.

### Signature

```rust
pub async fn paint_production_area(
    super_district: &SuperDistrict,
    painter_name: &str,
    data: &LoadedData,
    editor: &mut Editor,
    rng: &mut RNG,
)
```

### Algorithm

1. Look up `painter_name` in `data.resource_registry.production_painters`. Warn and return if not found.
2. Retrieve the `StructureID` of the just-placed building via `editor.world().structures.last()`. Warn and return if none (guards against calling before a building is placed).
3. Build an **edge buffer**: Chebyshev neighbourhood of radius 3 around all super-district edge cells.
4. Collect **free cells**: `super_district.data.points_2d` minus edge buffer, minus already-claimed cells, minus water cells.
5. Return early if `free_cells` is empty.
6. Dispatch on painter variant:

**Palettes branch:**
- If `flatten_strength > 0.0`, call `smooth_terrain(&free_cells, flatten_strength, editor)`.
- If `irrigation`, pick a random axis (X or Z) and offset (0–4) from the RNG. Cells where `coord % 5 == offset` become irrigation channels; the rest become field cells. Otherwise all cells are field cells.
- Place water via `replace_ground` on irrigation cells.
- For each palette: look up in `data.paint_palettes`, call `replace_ground` on field cells with `height_offset = Some(1)` for crop-tagged palettes, `None` otherwise.
- Claim all `free_cells` with `BuildClaim::ProductionArea(structure_id)`.

**Logging branch:**
- Find cells whose motion-blocking top block `is_tree()`.
- Randomly select `percent` of them via `rng.choose_many`.
- For each selected cell: call `log_trees`, then place the stump block at `get_non_tree_height(cell)`.
- Claim all `free_cells` with `BuildClaim::ProductionArea(structure_id)`.

---

## `smooth_terrain`

Added to `src/generator/terrain/terraforming.rs`. Uses `average_to_neighbours_5_away_multi` (the same algorithm as road smoothing) to Gaussian-blur terrain heights before painting.

```rust
pub async fn smooth_terrain(points: &HashSet<Point2D>, strength: f32, editor: &mut Editor)
```

`strength` in `[0.0, 1.0]` maps to 0–5 passes. Heights are read with `get_non_tree_height(p)` (no `-1`) so the value passed to `force_height` matches the same convention used by building placement — this keeps the height map consistent after smoothing.

| `flatten_strength` | Iterations | Effect |
|--------------------|-----------|--------|
| `0.0` | 0 | No smoothing. |
| `0.2` | 1 | Gentle. |
| `0.6` | 3 | Noticeable levelling, retains broad shape. |
| `1.0` | 5 | Near-flat over large areas. |

---

## `BuildClaim::ProductionArea`

Added to `src/generator/build_claim.rs`:

```rust
pub enum BuildClaim {
    Nature, Wall, Gate,
    Path(PathType),
    Building(BuildingID),
    Structure(StructureID),
    ProductionArea(StructureID),   // ← new
    None,
}
```

All cells painted by `paint_production_area` are claimed with `BuildClaim::ProductionArea(id)` where `id` is the `StructureID` of the building that produced the area. Ties every production area cell back to its source building.

---

## Call Site

`paint_production_area` is called after each successful `place_rural_building` in the placement tests. For the normal resource-chain path the painter comes from `assignment.production_painter`. For the override-building test (`rural_placement_override_building`) the painter is resolved directly from the override building's own gather recipe so it always matches the placed building type, regardless of what resource was assigned to the district:

```rust
let override_painter: Option<String> = data.resource_registry.recipes()
    .values()
    .find(|r| r.inputs.is_empty() && r.building == OVERRIDE_BUILDING)
    .and_then(|r| r.production_painter.clone());
```

---

## Files

| Action | Path |
|--------|------|
| Created | `src/generator/districts/paint_palette.rs` |
| Created | `src/generator/resource_chain/production_painter.rs` |
| Created | `src/generator/resource_chain/production_area.rs` |
| Created | `data/paint_palettes/palettes.yaml` |
| Created | `data/resource_chains/production_painters.yaml` |
| Modified | `src/generator/build_claim.rs` |
| Modified | `src/generator/terrain/terraforming.rs` |
| Modified | `src/generator/districts/mod.rs` |
| Modified | `src/generator/resource_chain/types.rs` |
| Modified | `src/generator/resource_chain/registry.rs` |
| Modified | `src/generator/resource_chain/mod.rs` |
| Modified | `src/generator/data.rs` |
| Modified | `src/generator/placement/test.rs` |
| Modified | `data/resource_chains/recipes.yaml` |
