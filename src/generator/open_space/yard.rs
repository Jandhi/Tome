//! Furnishing for [`RegionType::Yard`](super::RegionType::Yard) — edge + small:
//! a rustic perimeter backyard. We treat the interior as a kitchen garden (a
//! tilled crop patch with a watering hole), fence the open/wild sides to
//! enclose it, and scatter farm props (hay, barrels, composters) on the rest.

use std::collections::HashSet;

use crate::editor::Editor;
use crate::geometry::{Point2D, CARDINALS_2D};
use crate::noise::RNG;

use super::props::{is_building, is_path, place_lantern_post, put, put_forced};
use super::theme::Theme;
use super::Region;

/// A mature crop for a kitchen-garden patch.
fn crop(rng: &mut RNG) -> &'static str {
    *rng.choose(&[
        "minecraft:wheat[age=7]",
        "minecraft:carrots[age=7]",
        "minecraft:potatoes[age=7]",
        "minecraft:beetroots[age=3]",
    ])
}

/// Furnish one yard region in place.
pub async fn furnish_yard(editor: &Editor, region: &Region, rng: &mut RNG, theme: &Theme) {
    let world = editor.world();
    let cells: HashSet<Point2D> = region.cells.iter().copied().collect();
    let height_at = |c: Point2D| world.get_ocean_floor_height_at(c);

    // Split cells into interior (the field) and perimeter (fences / props),
    // keeping road-facing entrances clear.
    let mut interior: Vec<Point2D> = Vec::new();
    let mut perimeter: Vec<Point2D> = Vec::new();
    for &c in &region.cells {
        let mut on_perimeter = false;
        let mut touches_path = false;
        for d in CARDINALS_2D {
            let n = c + d;
            if !cells.contains(&n) {
                on_perimeter = true;
            }
            if is_path(world.get_claim(n).as_ref()) {
                touches_path = true;
            }
        }
        if touches_path {
            continue;
        }
        if on_perimeter {
            perimeter.push(c);
        } else {
            interior.push(c);
        }
    }

    let mut used: HashSet<Point2D> = HashSet::new();

    // Kitchen-garden patch on the interior: tilled rows around one watering
    // hole. Farmland + water are forced so they show through the grass surface.
    if !interior.is_empty() {
        let crop_block = crop(rng);
        // Watering hole: the first interior cell fully boxed in by same-height
        // region cells, so the source can't spill out a side and flood the
        // patch. `None` → an all-farmland garden (the soil is moist regardless).
        let water_cell = interior.iter().copied().find(|&c| {
            let h = height_at(c);
            CARDINALS_2D
                .iter()
                .all(|d| cells.contains(&(c + *d)) && height_at(c + *d) == h)
        });
        for &c in &interior {
            let Some(h) = height_at(c) else { continue; };
            if Some(c) == water_cell {
                put_forced(editor, c.x, h - 1, c.y, "minecraft:water").await;
            } else {
                put_forced(editor, c.x, h - 1, c.y, "minecraft:farmland[moisture=7]").await;
                put(editor, c.x, h, c.y, crop_block).await;
            }
            used.insert(c);
        }
    }

    // Perimeter: fence the open/wild sides to enclose the garden; drop a farm
    // prop on the rest (against buildings).
    rng.shuffle(&mut perimeter);
    let mut prop_budget = (region.area / 8).clamp(1, 4);
    for &c in &perimeter {
        if used.contains(&c) {
            continue;
        }
        let Some(h) = height_at(c) else { continue; };
        let open_side = CARDINALS_2D.iter().any(|d| {
            let n = c + *d;
            !cells.contains(&n)
                && !is_building(world.get_claim(n).as_ref())
                && !is_path(world.get_claim(n).as_ref())
        });
        if open_side {
            put(editor, c.x, h, c.y, &format!("minecraft:{}_fence", theme.wood)).await;
            used.insert(c);
        } else if prop_budget > 0 {
            let prop = *rng.choose(&[
                "minecraft:hay_block",
                "minecraft:barrel",
                "minecraft:composter",
            ]);
            put(editor, c.x, h, c.y, prop).await;
            used.insert(c);
            prop_budget -= 1;
        }
    }

    // A lantern for light.
    for &c in &perimeter {
        if used.contains(&c) {
            continue;
        }
        let Some(h) = height_at(c) else { continue; };
        place_lantern_post(editor, c, h, theme.wood).await;
        break;
    }
}
