# Resource Chain System

## Overview

A data-driven production chain system that maps Minecraft biome resources (wood, wool, honey, etc.) to finished goods via multi-step recipes. Districts produce raw resources based on their biome; the chain system resolves which goods they can manufacture.

**Goal**: Give districts economic identity — a forest district smells like a sawmill, a plains district like a bakery.

**Approach**: YAML data files define resources and recipes. Rust loads them into a registry at startup. Districts query the registry to resolve viable chains from their available raw inputs.

---

## Data Files ✅

All resource chain data lives in `data/resource_chains/`.

| File | Contents |
|------|----------|
| `resources.yaml` | 13 raw + 10 intermediate + 13 finished goods, each with `name`, `category`, `tier` |
| `recipes.yaml` | 23 recipes across all chains, each with `inputs`, `outputs`, `building` |
| `biome_resources.yaml` | 30 Minecraft biomes mapped to lists of raw resource IDs |

---

## Source Files ✅

| File | Contents |
|------|----------|
| `src/generator/resource_chain/types.rs` | Serde structs: `ResourceDef`, `RecipeDef`, and YAML file wrappers |
| `src/generator/resource_chain/registry.rs` | `ResourceRegistry` with load-time validation, `resolve()`, `raw_cost()`, `resources_for_biome()`, and 9 unit tests |
| `src/generator/resource_chain/mod.rs` | Public re-exports: `ResourceDef`, `RecipeDef`, `ResourceRegistry`, `ResolvedChains`, `NearMiss` |

`ResourceRegistry` is loaded as part of `LoadedData` in `src/generator/data.rs`.

**Load-time validation:**
- All resource IDs referenced in recipes must exist in `resources.yaml`
- No two recipes may produce the same resource
- No cycles in the recipe graph
- Every recipe must have at least one output

**Indices built at load time:**
- `produced_by` — resource → recipe that produces it
- `consumed_by` — resource → recipes that consume it
- `raw_cost` — resource → raw inputs required per 1 unit (recursive, memoized)

---

## Production Chains Reference

| Chain | Raw Inputs | Finished Good | Buildings |
|-------|-----------|---------------|-----------|
| Wood → Planks → Furniture | wood | furniture | sawmill, carpentry |
| Wood → Charcoal → (+ Iron) → Steel → Tools | wood, iron_ore | tools | charcoal_burner, smelter, forge, smithy |
| Wheat → Flour → Bread | wheat | bread | mill, bakery |
| Wheat + Flour → Ale | wheat | ale | mill, brewery |
| Honey → Wax → Candles | honey | candles | apiary, chandlery |
| Honey + Wheat → Mead | honey, wheat | mead | brewery |
| Sugar Cane → Sugar + Cocoa → Chocolate | sugar_cane, cocoa_beans | chocolate | mill, confectionery |
| Wool → Cloth → Clothing | wool | clothing | loom, tailor |
| Sand + Charcoal → Glass → Panes | sand, wood | glass_panes | charcoal_burner, glassworks |
| Clay + Charcoal → Bricks → Pottery | clay, wood | pottery | charcoal_burner, pottery_works |
| Sugar Cane → Paper + Leather → Books | sugar_cane, leather | books | paper_mill, scriptorium |
| Flint + Feathers → Arrows | flint, feathers | arrows | fletcher |

---

## Implementation Phases

### Phases 1–3 ✅
Data files, loading, validation, resolution algorithm, and unit tests are complete.

### Phase 4: Resource chains from districts
- [ ] Given a set of districts, populate in the registry a list of possible resources. Each District will only produce 1 type of raw resource in quantity of 2, add functionality to registry so that given an option set, it will select which raw resources are best during resource chain selection later.
- [ ] Once registry.resolve() is called it needs to select which district produces which raw resources and each district is given that building it needs to build. Each district which produces a raw resource is given the building it requires to have to produce the raw resource.