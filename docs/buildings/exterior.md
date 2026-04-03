# Exterior Module

Decorates the area around the building within the plot. Runs last in the pipeline,
after all building geometry is placed.

## Input
- `Plot` (bounds `min`/`max` + `usable` mask)
- `Footprint` (polygon outline of the building)
- `WallSegments` (needed to locate doors)
- `Palette` (material palette for the building, gives us flower/wood/stone choices)

## Output
- `()` (modifies world directly via Editor)

## Available Space

The exterior module works with the "yard" -- the usable plot area that is not occupied
by the building footprint. We compute this once up front and reuse it for every subsystem.

```
yard_mask[x][z] = plot.usable[x][z] && !footprint.contains(x, z)
```

Each subsystem that places blocks marks those cells as consumed so later subsystems
skip them. Processing order matters: paths first (they must connect), then fences
(they border the property), then gardens fill remaining space, then lighting goes
on top of whatever is already placed.

```
  Plot
  +-----------------+
  | y y y y y y y y |    y = yard (usable, not footprint)
  | # # # # # y y y |    # = building footprint
  | # # # # # x x y |    x = unusable (water, tree, etc.)
  | # # # # # x x y |    . = outside plot
  | # # # # # y y y |
  | y y y y y y y y |
  | x x y y y y y y |
  | x x y y y y y y |
  +-----------------+
```

## Entrance Detection

The walls module produces `WallSegments`, each of which knows its openings.
We need door positions to know where paths should lead.

**Getting door positions from WallSegments:**

In the current codebase, `BuildingShape` stores `doors: Option<Vec<WallPlacement>>` where
each `WallPlacement` has a `cell: Point3D` (grid coordinate) and `direction: Cardinal`
(which face of the cell the door is on). The `Grid::get_door_world_position(cell, direction)`
method converts this to a world-space `Point3D`.

For the new pipeline's `WallSegments` output, we need the equivalent information exposed:
- A list of door positions in world coordinates (the block just outside the door)
- The cardinal direction each door faces (outward from the building)

The exterior module picks the **primary entrance** as the door whose outward-facing
direction points most toward the nearest plot edge, since that is likely the road side.
If multiple doors tie, pick the one closest to a plot edge. Other doors are secondary
entrances and may also get short connecting paths but not the main walkway.

```rust
fn find_primary_entrance(doors: &[(Point3D, Cardinal)], plot: &Plot) -> (Point3D, Cardinal) {
    // For each door, measure distance from the door position (stepped one block
    // in the door's facing direction) to the nearest plot edge.
    // The door with the shortest distance is the primary entrance.
    doors.iter()
        .min_by_key(|(pos, dir)| {
            let outside = *pos + (*dir).into();
            distance_to_nearest_plot_edge(outside.drop_y(), plot)
        })
        .unwrap()
}
```

## 1. Path Generation

Paths connect the primary entrance to the nearest plot edge (which borders the road/
public space). Secondary doors may get shorter paths connecting to the main path or
directly to a nearby edge.

**Algorithm:**

1. Identify the primary entrance door and its world position (one block outside the door).
2. Identify the target: the nearest point on the plot boundary in the door's facing
   direction. Walk in the door's cardinal direction until hitting the plot edge. If the
   plot edge is not directly ahead (e.g. the door faces a corner), find the closest
   edge point instead.
3. Trace a path between these two points. For most houses this is a short straight line
   (3-10 blocks). If obstacles exist in the yard mask, do a simple L-shaped or
   two-segment path around them rather than full A* (the distances are tiny).
4. The path is 1-2 blocks wide depending on building size.
5. Place path blocks: use `PrimaryStone` slab/block from the palette. Alternate with
   `SecondaryStone` every few blocks for texture variation (weighted random, same
   approach as the existing road paving in `placement.rs`).
6. Mark path cells as consumed in the yard mask.

**Path block selection:**
- Small buildings (footprint < 20 blocks): 1-wide path, slabs only
- Medium/large buildings: 2-wide path, mix of slabs and full blocks

**Door steps:** If the terrain at the door is lower than `base_y`, place a short staircase
descending from the door to natural grade before starting the path. Use `PrimaryStone`
stair blocks facing outward, one per Y level of difference. For differences of 1-3 blocks
this produces a clean entry. For larger drops (4+), extend the stairs diagonally along the
building wall to keep the slope gentle.

**Edge cases:**
- If the door is already adjacent to the plot edge, skip path generation (door opens
  directly onto the road).
- If no usable path exists (yard completely blocked), skip gracefully.

## 2. Fence / Wall Placement

Fences run along the plot boundary to define the property edge. They skip the path
entrance and any plot edges that border non-usable cells (water, cliffs).

**Algorithm:**

1. Walk the perimeter of the plot bounds (`min` to `max` rectangle edges).
2. For each perimeter cell, check:
   - Is the cell usable? (`plot.usable[x][z] == true`)
   - Is it not part of the path? (not consumed by path generation)
   - Is the adjacent cell outside the plot or non-usable? (confirms this is a real edge)
3. Place a fence post at qualifying cells. Use `WoodPillar` material from palette for
   fence posts, `PrimaryWood` for fence rails/connections.
4. At the path crossing point(s), place fence gates instead of fence blocks. The gate
   width matches the path width (1-2 blocks).
5. Corner posts use a full log block instead of a fence for visual weight.

**Fence vs wall decision:**
- Default to fences (wooden fences from palette wood type)
- If the building style has stone as its primary material (`PrimaryWall` resolves to
  stone), use cobblestone walls instead for a masonry look
- Skip fences entirely for very small plots where the building nearly fills the space
  (yard area < 15% of plot area)

**Fence height:** Always 1 block (standard Minecraft fence = 1.5 block collision height).

## 3. Garden / Decoration Placement

Gardens fill the remaining yard space (after paths and fences are placed). The approach
uses zone-based placement: divide remaining yard cells into clusters, then assign each
cluster a decoration type.

**Step 1: Identify plantable zones**

Flood-fill the remaining (unconsumed, usable) yard cells into connected regions.
Each connected region becomes a "zone."

**Step 2: Assign zone types by size**

| Zone size (blocks) | Assignment |
|---|---|
| 1-3 | Single decoration (flower pot, lantern on fence, small bush) |
| 4-8 | Flower bed (fill with `Flower` material from palette) |
| 9-15 | Mixed garden (flowers + leaf blocks as bushes + 1-2 tall grass patches) |
| 16+ | Structured garden or crop patch |

For structured gardens (16+ blocks):
- 60% chance: rows of crops (wheat, carrots, potatoes -- pick one randomly). Place
  farmland blocks with water source every 4 blocks. Rows run parallel to the nearest
  building wall.
- 30% chance: flower garden with a central feature (single leaf-block tree, armor stand,
  or composting bin).
- 10% chance: leave as grass with scattered tall grass and flowers for a "wild" look.

**Step 3: Place blocks**

- Flowers: use `Flower` MaterialRole from palette. Place on grass blocks.
- Bushes: oak/spruce leaves (match palette wood type) placed as single blocks.
- Crops: farmland + crop block. Water source block every 4th row, covered with a
  trapdoor to look intentional.
- Tall grass / ferns: scatter randomly at 30% density in wild zones.

**Adjacent-to-building rule:** The 1-block strip immediately adjacent to the building
walls (but not at doors) gets special treatment: 50% chance of flower box (trapdoor on
wall + flower on top), 50% chance of bush (leaf block). This softens the building-to-
ground transition.

## 4. Outdoor Lighting

Lighting prevents mob spawning in the yard and adds atmosphere. Placement depends on
what else has been placed.

**Lamp post placement:**
1. Walk the path from entrance to plot edge. Place a lamp post every 6-8 blocks along
   the path, offset 1 block to the side. Lamp posts are: fence post + fence post +
   torch/lantern on top (3 blocks tall). Use `WoodPillar` material for the post.
2. If the path is shorter than 6 blocks, place one lamp post at the gate/entrance.

**Fence lighting:**
- At each fence corner post, 50% chance to place a torch or lantern on top.
- If the fence perimeter is longer than 20 blocks, place a lantern on every 8th fence
  post.

**Wall-mounted lighting:**
- On exterior building walls adjacent to the yard (not where windows/doors are), place
  wall torches every 6 blocks at y+2 above ground level. This is just `torch` with a
  directional wall placement facing outward.

**Block choices:**
- Lanterns (`lantern` block) for lamp posts and fence posts (looks better elevated).
- Wall torches (`wall_torch` with facing direction) on building walls.
- If biome is cold (detected from palette/style), use `soul_lantern` and `soul_wall_torch`
  for blue-tinted lighting.

## Processing Order

```
exterior::generate(plot, footprint, wall_segments, palette):
    yard = compute_yard_mask(plot, footprint)
    doors = extract_door_positions(wall_segments)
    primary_entrance = find_primary_entrance(doors, plot)

    // 1. Paths (must be first -- fences need to know where the gate goes)
    path_cells = generate_path(primary_entrance, plot, &mut yard)

    // 2. Fences (need path crossing info for gate placement)
    generate_fences(plot, &yard, &path_cells, palette)

    // 3. Gardens (fill whatever remains)
    generate_gardens(&mut yard, footprint, palette)

    // 4. Lighting (goes on top of everything)
    place_path_lighting(&path_cells, palette)
    place_fence_lighting(plot, palette)
    place_wall_lighting(footprint, wall_segments, palette)
```

## Integration with Pipeline

The main pipeline currently calls `exterior::generate(Plot, Footprint)`. This needs to
expand to also pass `WallSegments` (for door positions) and `Palette` (for material
choices). The updated pipeline call:

```
  → walls::generate(Frame) → WallSegments
  → ...
  → exterior::generate(Plot, Footprint, WallSegments, Palette) → ()
```

This is a minor change to the pipeline signature. The walls module already produces
`WallSegments` and the palette is available throughout the pipeline.

## Notes

- All placement goes through `Editor` so it respects the existing world state and
  claim system. Exterior cells should be claimed as `BuildClaim::Building(id)` to
  prevent other systems from overwriting them.
- The `Flower` MaterialRole already exists in the palette system and is populated with
  biome-appropriate flowers during building placement (see `placement.rs`).
- Keep decoration density reasonable. A half-empty yard with a path and some flowers
  looks better than every cell crammed with blocks. Target 40-60% fill of yard space.
- All randomness goes through the shared `RNG` for reproducibility.
