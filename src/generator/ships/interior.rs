//! Stage 3 · **Interior furnishing + bulkheads** — reuses the buildings_v2 furnishing engine to
//! dress the ship's interior levels.
//!
//! - **Holds / lower holds:** one furnishable rect, packed with cargo (`hold` / `lower_hold` lists).
//! - **Gun deck ('tween):** divided **fore→aft** into **cabins** by plank **bulkheads** (with a
//!   doorway) — a stern **great cabin** (captain), then **crew quarters**, then the **galley** —
//!   each furnished from its own `data/rooms.yaml` list.
//!
//! For each room it carves the **largest interior rectangle** from the deck outline
//! (`footprint::find_largest_rect`), transforms it to a world `Rect2D` (cardinal heading →
//! axis-aligned), seeds a `ConstraintMap` blocking the **masts** + **companionway hatches**, and
//! calls `furnish_interior`.

use crate::generator::buildings_v2::footprint::find_largest_rect;
use crate::generator::buildings_v2::furnish::furnish_interior;
use crate::generator::buildings_v2::rooms::{CellState, ConstraintMap};
use crate::generator::materials::{MaterialPlacer, Placer};
use crate::geometry::{Point2D, Point3D, Rect2D};
use crate::minecraft::BlockForm;

use super::levels::{ShipLevel, ShipLevels};
use super::palette::{ShipPalette, ShipPart};
use super::{Placement, ShipCtx};

/// A room's furnishable rect must be at least this many cells to bother dressing it.
const MIN_FURNISH_AREA: i32 = 6;
/// Cap each furnishable rect to this span per side — bounds the furnishing cost on huge ships.
const FURNISH_MAX_SPAN: i32 = 14;
/// Shortest a gun-deck cabin can be (stations along the length); fewer → fewer cabins.
const CABIN_MIN_LEN: i32 = 5;
/// Gun-deck cabins, **stern → bow** (the stern gets the captain's great cabin).
const CABIN_ROOMS: &[&str] = &["captain_cabin", "crew_quarters", "galley"];

/// Furnish every interior level — holds as cargo, the gun deck as bulkheaded cabins.
pub async fn furnish(
    ctx: &mut ShipCtx<'_>,
    placement: &Placement,
    ship_palette: &ShipPalette,
    levels: &ShipLevels,
    hatch_cells: &[Point3D],
    mast_xs: &[i32],
) {
    for level in &levels.levels {
        let rect = match local_rect(level) {
            Some(r) => r,
            None => continue,
        };
        if level.name == "gun_deck" {
            furnish_cabins(ctx, placement, ship_palette, level, rect, hatch_cells, mast_xs).await;
        } else if ctx.data.furniture.rooms.contains_key(level.name) {
            furnish_local_rect(ctx, placement, level, rect, level.name, hatch_cells, mast_xs).await;
        }
    }
}

/// Largest interior rectangle of a level, in **local** coords `(lx0, lx1, lz0, lz1)` (inclusive).
fn local_rect(level: &ShipLevel) -> Option<(i32, i32, i32, i32)> {
    let max_half = level.outline.iter().copied().max().unwrap_or(0);
    if max_half < 1 {
        return None;
    }
    let width = (2 * max_half + 1) as usize;
    let mut grid = vec![vec![false; width]; level.outline.len()];
    for (x, &h) in level.outline.iter().enumerate() {
        for z in -h..=h {
            grid[x][(z + max_half) as usize] = true;
        }
    }
    let r = find_largest_rect(&grid)?;
    if r.area() < MIN_FURNISH_AREA {
        return None;
    }
    Some((r.min().x, r.max().x, r.min().y - max_half, r.max().y - max_half))
}

/// Divide the gun deck's rect fore→aft into cabins separated by plank bulkheads (with a centreline
/// doorway), furnishing each cabin from its own room list.
async fn furnish_cabins(
    ctx: &mut ShipCtx<'_>,
    placement: &Placement,
    ship_palette: &ShipPalette,
    level: &ShipLevel,
    (lx0, lx1, lz0, lz1): (i32, i32, i32, i32),
    hatch_cells: &[Point3D],
    mast_xs: &[i32],
) {
    let span = lx1 - lx0 + 1;
    let n = (span / CABIN_MIN_LEN).clamp(1, CABIN_ROOMS.len() as i32);
    if n <= 1 {
        furnish_local_rect(ctx, placement, level, (lx0, lx1, lz0, lz1), CABIN_ROOMS[0], hatch_cells, mast_xs).await;
        return;
    }
    let seg = span / n;
    // Internal bulkhead boundary columns + the cabin x-ranges they separate.
    let boundaries: Vec<i32> = (1..n).map(|i| lx0 + i * seg).collect();
    let deck_mat = ctx.palette.get_material(ship_palette.role(ShipPart::Deck)).cloned();
    if let Some(mat) = &deck_mat {
        let door_z = if lz0 <= 0 && 0 <= lz1 { 0 } else { (lz0 + lz1) / 2 };
        let mut rng = ctx.rng.derive();
        let mut placer = MaterialPlacer::new(Placer::new(&ctx.data.materials, &mut rng), mat.clone());
        for &bx in &boundaries {
            for z in lz0..=lz1 {
                for y in (level.floor_y + 1)..level.ceiling_y {
                    // Leave a 2-tall doorway on the centreline.
                    if z == door_z && y <= level.floor_y + 2 {
                        continue;
                    }
                    placer
                        .place_block(ctx.editor, placement.to_world(Point3D::new(bx, y, z)), BlockForm::Block, None, None)
                        .await;
                }
            }
        }
    }
    // Furnish each cabin (the open run between bulkheads), stern → bow.
    for i in 0..n {
        let cx0 = if i == 0 { lx0 } else { boundaries[(i - 1) as usize] + 1 };
        let cx1 = if i == n - 1 { lx1 } else { boundaries[i as usize] - 1 };
        if cx1 < cx0 {
            continue;
        }
        let room = CABIN_ROOMS[(i as usize).min(CABIN_ROOMS.len() - 1)];
        furnish_local_rect(ctx, placement, level, (cx0, cx1, lz0, lz1), room, hatch_cells, mast_xs).await;
    }
}

/// Furnish one local rect with `room_name`: transform to world, seed the constraint map (masts +
/// hatches), and run the buildings_v2 furnishing engine.
async fn furnish_local_rect(
    ctx: &mut ShipCtx<'_>,
    placement: &Placement,
    level: &ShipLevel,
    (lx0, lx1, lz0, lz1): (i32, i32, i32, i32),
    room_name: &str,
    hatch_cells: &[Point3D],
    mast_xs: &[i32],
) {
    if !ctx.data.furniture.rooms.contains_key(room_name) {
        return;
    }
    // Furniture stands one above the laid floor; cap the span to keep furnishing fast.
    let floor_walk = level.floor_y + 1;
    let w1 = placement.to_world(Point3D::new(lx0, floor_walk, lz0));
    let w2 = placement.to_world(Point3D::new(lx1, floor_walk, lz1));
    let interior = crop_centered(
        Rect2D::from_points(Point2D::new(w1.x, w1.z), Point2D::new(w2.x, w2.z)),
        FURNISH_MAX_SPAN,
    );
    if interior.area() < MIN_FURNISH_AREA {
        return;
    }
    let floor_y = w1.y;
    let ceiling_y = placement.to_world(Point3D::new(lx0, level.ceiling_y, 0)).y;

    let mut constraints = ConstraintMap::new(&interior);
    for &mx in mast_xs {
        let w = placement.to_world(Point3D::new(mx, floor_walk, 0));
        constraints.set((w.x, w.z), CellState::Blocked);
    }
    for cell in hatch_cells.iter().filter(|c| c.y == level.floor_y) {
        let w = placement.to_world(Point3D::new(cell.x, floor_walk, cell.z));
        constraints.set((w.x, w.z), CellState::Blocked);
    }

    let room_list = &ctx.data.furniture.rooms[room_name];
    let mut rng = ctx.rng.derive();
    let _placed = furnish_interior(
        ctx.editor,
        &interior,
        &mut constraints,
        room_list,
        &ctx.data.furniture.items,
        floor_y,
        ceiling_y,
        None,
        false,
        ctx.palette,
        &ctx.data.materials,
        &ctx.data.furniture.loot,
        &mut rng,
    )
    .await;
}

/// Crop a rect to at most `max_span` per side, centred.
fn crop_centered(r: Rect2D, max_span: i32) -> Rect2D {
    let mut o = r.origin;
    let mut s = r.size;
    if s.x > max_span {
        o.x += (s.x - max_span) / 2;
        s.x = max_span;
    }
    if s.y > max_span {
        o.y += (s.y - max_span) / 2;
        s.y = max_span;
    }
    Rect2D::new(o, s)
}
