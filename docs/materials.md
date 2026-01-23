# Materials & Palette System

The materials system provides data-driven block selection with support for visual variation, weathering effects, and consistent theming across structures.

## Overview

Materials abstract away individual Minecraft blocks, allowing generators to work with logical materials (e.g., "wood planks") that map to specific blocks based on context, biome, and style.

## Data Structures

### MaterialId

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MaterialId(String);
```

String identifier for a material (e.g., `"spruce_planks"`, `"cobblestone"`).

### Material

```rust
pub struct Material {
    pub id: MaterialId,
    pub connections: Option<MaterialConnections>,
    pub blocks: HashMap<BlockForm, MaterialBlocks>,
}
```

### MaterialBlocks

```rust
pub struct MaterialBlocks {
    pub blocks: Vec<WeightedBlock>,
}

pub struct WeightedBlock {
    pub block: Block,
    pub weight: f32,
}
```

Weighted random selection allows variation (e.g., mixing cobblestone with mossy cobblestone).

### BlockForm

```rust
pub enum BlockForm {
    Full,       // Solid cube (planks, stone)
    Slab,       // Half block
    Stair,      // Step block
    Wall,       // Wall block (cobblestone wall)
    Fence,      // Fence/fence gate
    Door,       // Door block
    Trapdoor,   // Trapdoor block
    Button,     // Button block
    Pressure,   // Pressure plate
}
```

Materials define which forms are available and map to specific blocks.

## Material Connections

Materials can be linked to create visual variation:

```rust
pub struct MaterialConnections {
    pub lighter: Option<MaterialId>,
    pub darker: Option<MaterialId>,
    pub more_worn: Option<MaterialId>,
    pub less_worn: Option<MaterialId>,
    pub wetter: Option<MaterialId>,
    pub drier: Option<MaterialId>,
    pub more_decorated: Option<MaterialId>,
    pub less_decorated: Option<MaterialId>,
}
```

### Connection Types

| Feature | Description | Example |
|---------|-------------|---------|
| Shade | Lighter/darker variants | Oak → Dark Oak |
| Wear | Weathering level | Stone → Cobblestone → Mossy Cobblestone |
| Moisture | Wet/dry variants | Cobblestone → Mossy Cobblestone |
| Decoration | Detail level | Plain → Carved |

### Traversing Connections

```rust
pub enum MaterialFeature {
    Shade,
    Wear,
    Moisture,
    Decoration,
}

// Get a related material
let darker = material.get_connected(MaterialFeature::Shade, -1);
let more_worn = material.get_connected(MaterialFeature::Wear, 1);
```

## Palettes

Palettes assign materials to semantic roles:

```rust
pub struct Palette {
    pub id: String,
    pub materials: HashMap<MaterialRole, MaterialId>,
}
```

### Material Roles

```rust
pub enum MaterialRole {
    Primary,        // Main structural material
    Secondary,      // Accent/contrast material
    Tertiary,       // Third material option
    Floor,          // Floor surfaces
    Roof,           // Roof covering
    Trim,           // Decorative trim
    Support,        // Structural supports
    Window,         // Window frames
    Door,           // Door material
    Fence,          // Fencing
    Path,           // Pathway blocks
    Flower,         // Decorative plants
}
```

### Example Palette

```json
{
    "id": "medieval_oak",
    "materials": {
        "Primary": "oak_planks",
        "Secondary": "cobblestone",
        "Floor": "oak_planks",
        "Roof": "dark_oak_planks",
        "Trim": "stripped_oak_log",
        "Support": "oak_log",
        "Path": "gravel"
    }
}
```

## Data Loading

Materials and palettes are loaded from JSON files in `data/materials/`:

```
data/
├── materials/
│   ├── wood/
│   │   ├── oak.json
│   │   ├── spruce.json
│   │   └── birch.json
│   ├── stone/
│   │   ├── cobblestone.json
│   │   └── stone_brick.json
│   └── palettes/
│       ├── medieval.json
│       └── desert.json
```

### Material JSON Format

```json
{
    "id": "oak_planks",
    "connections": {
        "darker": "dark_oak_planks",
        "more_worn": "oak_planks_weathered"
    },
    "blocks": {
        "Full": {
            "blocks": [
                { "block": "minecraft:oak_planks", "weight": 1.0 }
            ]
        },
        "Slab": {
            "blocks": [
                { "block": "minecraft:oak_slab", "weight": 1.0 }
            ]
        },
        "Stair": {
            "blocks": [
                { "block": "minecraft:oak_stairs", "weight": 1.0 }
            ]
        }
    }
}
```

### Weighted Block Selection

For visual variety, materials can have multiple blocks:

```json
{
    "id": "cobblestone_mix",
    "blocks": {
        "Full": {
            "blocks": [
                { "block": "minecraft:cobblestone", "weight": 0.7 },
                { "block": "minecraft:mossy_cobblestone", "weight": 0.3 }
            ]
        }
    }
}
```

## Usage in Generation

### Getting a Block from Material

```rust
// Get the material
let material = materials.get(&MaterialId::new("oak_planks"));

// Get a block for a specific form
let block = material.get_block(BlockForm::Full, &mut rng);

// Place the block
editor.place_block(position, block).await;
```

### Using Palettes

```rust
// Load palette
let palette = palettes.get("medieval_oak");

// Get material for a role
let primary_id = palette.get(MaterialRole::Primary);
let primary = materials.get(primary_id);

// Get block
let wall_block = primary.get_block(BlockForm::Full, &mut rng);
```

### Applying Features

```rust
// Get a darker variant for shadows
let base = materials.get(&MaterialId::new("oak_planks"));
let darker_id = base.connections.darker.as_ref();
let darker = materials.get(darker_id.unwrap());
```

## Biome Integration

Palettes can be selected based on biome:

```rust
fn palette_for_biome(biome: Biome) -> &Palette {
    match biome {
        Biome::Desert => palettes.get("desert"),
        Biome::Taiga => palettes.get("spruce"),
        Biome::Savanna => palettes.get("acacia"),
        _ => palettes.get("default"),
    }
}
```

## Block States

Materials handle block states (orientation, etc.):

```rust
pub struct Block {
    pub id: BlockID,
    pub state: Option<HashMap<String, String>>,
    pub data: Option<String>,
}

// Stairs with facing direction
let stair_block = Block {
    id: BlockID::new("oak_stairs"),
    state: Some(hashmap!{
        "facing" => "north",
        "half" => "bottom",
    }),
    data: None,
};
```

## Form Fallbacks

If a material doesn't have a specific form, it can fall back:

```rust
impl Material {
    fn get_block(&self, form: BlockForm, rng: &mut Rng) -> Option<Block> {
        // Try exact form
        if let Some(blocks) = self.blocks.get(&form) {
            return Some(blocks.choose(rng));
        }

        // Fallback to Full for missing forms
        if form != BlockForm::Full {
            return self.get_block(BlockForm::Full, rng);
        }

        None
    }
}
```

## Best Practices

1. **Define all forms** - If a material has slab/stair variants, define them
2. **Use connections** - Link related materials for dynamic variation
3. **Weight appropriately** - Use weights for natural-looking variation
4. **Consistent palettes** - Ensure palettes have all needed roles
5. **Biome matching** - Create biome-specific palettes for immersion
