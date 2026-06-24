//! Deck addition · **Additional deck(s)** (gun deck) — raised topside level(s).
//!
//! Each level's walls rise from the deck below following that deck's outline and
//! curve **inward (tumblehome)** near the top (vertical lower down, near-vertical at
//! the stern). The tumblehome step is bevelled with stairs on **both** faces. Gun
//! ports line the sides (every ~2 blocks), per-ship either **trapdoor lids** or
//! **open holes**. A new deck floor caps each level — and is the base the next level
//! stacks on. Heights vary; larger ships may carry a second level.

use std::collections::{HashMap, HashSet};

use crate::generator::materials::{MaterialPlacer, Placer};
use crate::geometry::Point3D;
use crate::minecraft::BlockForm;
use crate::noise::RNG;

use super::super::palette::ShipPart;
use super::super::tuning::GUN_PORT_STEP;
use super::super::{ShipDir, ShipV2Ctx};
use super::{DeckContext, DeckState, SizeTier};

/// A randomly varied level height.
fn random_height(rng: &mut RNG) -> i32 {
    rng.rand_i32_range(3, 6) // 3..=5
}

/// How many stacked additional decks. Capped at 1 for now (stacking multiple levels is
/// a follow-up); larger ships will later carry a second one.
fn num_levels(_tier: SizeTier, _rng: &mut RNG) -> i32 {
    1
}

/// The bow/stern direction a length-end perimeter cell should bevel toward, if any: a
/// cell whose forward or aft neighbour leaves the ring while **both** sides stay in it is
/// a fore/aft length step — a stair facing the open end rakes the taper smooth (the hull
/// bilge bevel idea, applied along x). Returns `None` for side cells (handled elsewhere).
fn plan_step_dir(in_ring: &impl Fn(i32, i32, i32) -> bool, r: i32, x: i32, z: i32) -> Option<ShipDir> {
    if !(in_ring(r, x, z + 1) && in_ring(r, x, z - 1)) {
        return None; // a side cell, not a fore/aft end
    }
    if !in_ring(r, x + 1, z) {
        Some(ShipDir::Bow)
    } else if !in_ring(r, x - 1, z) {
        Some(ShipDir::Stern)
    } else {
        None
    }
}

/// Build the additional deck(s): one or more stacked topside levels of varied
/// height. Each level's inset top outline becomes the next level's base. The final
/// level's inset top outline + floor Y are written back into [`DeckState`] so later
/// additions (the railing) sit on the new top weather deck.
pub async fn build(ctx: &mut ShipV2Ctx<'_>, dc: &DeckContext<'_>, state: &mut DeckState) {
    let levels = num_levels(dc.tier, ctx.rng);
    // Decide once per ship whether the gun ports are trapdoor lids or open holes.
    let ports_are_trapdoors = ctx.rng.rand_i32_range(0, 2) == 0;

    let mut base: Vec<i32> = dc.hull.top_half.clone();
    let mut base_y = dc.deck.deck_y;
    let mut rim = state.rail_outline.clone();
    for _ in 0..levels {
        let height = random_height(ctx.rng);
        let (inset, outer, ports) =
            build_level(ctx, dc, &base, base_y, height, ports_are_trapdoors).await;
        base = inset;
        rim = outer;
        base_y += height;
        state.gun_ports.extend(ports); // re-stamped after the bowsprit
    }

    // Hand the raised top deck to subsequent additions: the inset outline for stacking,
    // the outer rim for the railing.
    state.top_outline = base;
    state.rail_outline = rim;
    state.top_y = base_y;
    state.gun_ports_trapdoors = ports_are_trapdoors;
}

/// Build a single topside level on top of `base` (half-beam per station) at
/// `base_y`, `height` tall. Returns `(inset top outline, outer rim outline, gun ports)` —
/// the inset for stacking the next level, the rim for the railing, and the planned gun-port
/// cells (re-stamped after the bowsprit).
async fn build_level(
    ctx: &mut ShipV2Ctx<'_>,
    dc: &DeckContext<'_>,
    base: &[i32],
    base_y: i32,
    height: i32,
    ports_are_trapdoors: bool,
) -> (Vec<i32>, Vec<i32>, Vec<(Point3D, ShipDir)>) {
    let length = base.len() as i32;
    // Tumblehome grows a touch with height (~1 per 3 blocks).
    let total_inset = ((height as f32) / 3.0).round().max(1.0) as i32;

    // Stern stays straight; tumblehome ramps in over the forward ~70%.
    let aft_factor = |x: i32| -> f32 {
        let ramp = (length as f32 * 0.3).max(1.0);
        ((x as f32) / ramp).clamp(0.0, 1.0)
    };
    // Cubic → vertical most of the way, curving in only near the very top.
    let inset_at = |r: i32, x: i32| -> i32 {
        let t = (r as f32) / (height as f32);
        (total_inset as f32 * t * t * t * aft_factor(x)).round() as i32
    };
    // The hull tapers to a sharp point at the stern; blunt it into a small transom
    // so the back of each deck is a clean flat-ish wall (and stacked levels align).
    let transom_zone = (length / 10).max(2);
    let stern_min = |x: i32| -> i32 {
        if x >= transom_zone {
            0
        } else {
            let t = (x as f32) / (transom_zone as f32);
            (2.0 * (1.0 - t)).round() as i32 // half-width ramps 2 → 0 over the zone
        }
    };
    let raw_half = |r: i32, x: i32| -> i32 {
        if x < 0 || x as usize >= base.len() {
            return 0;
        }
        (base[x as usize] - inset_at(r, x)).max(stern_min(x)).max(0)
    };
    // The widest station (≈ ⅓ from the bow on a teardrop). The hull rises to it from
    // the stern, but injecting `stern_min` (which falls toward the bow) on top of that
    // rising taper carves a V-shaped *pinch* in the stern outline. Enforce the stern
    // side (x ≤ peak) to be **non-decreasing** so the transom blends into the hull as a
    // clean flat-ish wall instead of pinching in — otherwise the deck/walls/railing all
    // follow the notch and the stern reads as weird stepped terraces.
    let peak_x = (0..length).max_by_key(|&x| raw_half(height, x)).unwrap_or(0);
    let half_at = |r: i32, x: i32| -> i32 {
        if x < 0 || x >= length {
            return 0;
        }
        if x <= peak_x {
            (0..=x).map(|x2| raw_half(r, x2)).max().unwrap_or(0)
        } else {
            raw_half(r, x)
        }
    };
    let in_ring = |r: i32, x: i32, z: i32| -> bool {
        let h = half_at(r, x);
        h >= 1 && z.abs() <= h
    };
    let is_perimeter = |r: i32, x: i32, z: i32| -> bool {
        in_ring(r, x, z)
            && (!in_ring(r, x - 1, z)
                || !in_ring(r, x + 1, z)
                || !in_ring(r, x, z - 1)
                || !in_ring(r, x, z + 1))
    };

    // --- Plan the gun ports (cannon row, both sides). The band reaches well toward the
    // bow and stern; the `h >= 2` width guard below naturally stops the ports where the
    // hull narrows at the ends. ---
    let gun_r = 2;
    let band = (length / 8)..(length * 7 / 8);
    let mut ports: Vec<(Point3D, ShipDir)> = Vec::new();
    let mut port_set: HashSet<Point3D> = HashSet::new();
    let mut x = band.start;
    while x < band.end {
        let h = half_at(gun_r, x);
        if h >= 2 {
            for (z, dir) in [(h, ShipDir::Starboard), (-h, ShipDir::Port)] {
                let cell = Point3D::new(x, base_y + gun_r, z);
                ports.push((cell, dir));
                port_set.insert(cell);
            }
        }
        x += GUN_PORT_STEP;
    }

    // --- Placers. ---
    let topside_mat = ctx
        .palette
        .get_material(dc.ship_palette.role(ShipPart::Topside))
        .expect("Topside role missing from base palette")
        .clone();
    let deck_mat = ctx
        .palette
        .get_material(dc.ship_palette.role(ShipPart::Deck))
        .expect("Deck role missing from base palette")
        .clone();
    let mut wall_rng = ctx.rng.derive();
    let mut floor_rng = ctx.rng.derive();
    let mut walls = MaterialPlacer::new(Placer::new(&ctx.data.materials, &mut wall_rng), topside_mat);
    let mut floor = MaterialPlacer::new(Placer::new(&ctx.data.materials, &mut floor_rng), deck_mat);
    let place = dc.placement;

    // --- Walls: a perimeter ring per row, insetting (tumblehome) near the top. ---
    for r in 1..=height {
        let y = base_y + r;
        for sx in 0..length {
            let h = half_at(r, sx);
            if h < 1 {
                continue;
            }
            for z in -h..=h {
                if !is_perimeter(r, sx, z) {
                    continue;
                }
                let cell = Point3D::new(sx, y, z);
                if port_set.contains(&cell) {
                    continue;
                }
                let stepped_in = r >= 2 && inset_at(r, sx) > inset_at(r - 1, sx);
                if stepped_in {
                    // Outward direction (the open neighbour) for the bevel.
                    let dir = if z > 0 && !in_ring(r, sx, z + 1) {
                        ShipDir::Starboard
                    } else if z < 0 && !in_ring(r, sx, z - 1) {
                        ShipDir::Port
                    } else if !in_ring(r, sx + 1, z) {
                        ShipDir::Bow
                    } else if !in_ring(r, sx - 1, z) {
                        ShipDir::Stern
                    } else {
                        ShipDir::Starboard
                    };
                    let facing = place.world_cardinal(dir);
                    // Inside bevel: upside-down stair at the inset edge.
                    let inside = HashMap::from([
                        ("facing".to_string(), facing.to_string()),
                        ("half".to_string(), "top".to_string()),
                    ]);
                    walls
                        .place_block(ctx.editor, place.to_world(cell), BlockForm::Stairs, Some(&inside), None)
                        .await;
                    // Outside bevel: a bottom stair on the ledge one block outboard — only
                    // for the **side** (Port/Starboard) steps, where it lands on the real
                    // outboard ledge (z ± 1). For Bow/Stern length-end steps it would jut
                    // one block past the end into the air (a stray inward-facing stair), so
                    // skip those.
                    if matches!(dir, ShipDir::Starboard | ShipDir::Port) {
                        let oz = if matches!(dir, ShipDir::Starboard) { 1 } else { -1 };
                        let outside_cell = Point3D::new(sx, y, z + oz);
                        // Outside bevel faces inward (opposite the inside stair).
                        let outside = HashMap::from([
                            ("facing".to_string(), place.world_cardinal(dir.opposite()).to_string()),
                            ("half".to_string(), "bottom".to_string()),
                        ]);
                        walls
                            .place_block(ctx.editor, place.to_world(outside_cell), BlockForm::Stairs, Some(&outside), None)
                            .await;
                    }
                } else if let Some(dir) = plan_step_dir(&in_ring, r, sx, z) {
                    // Smooth the fore/aft length taper (the same idea as the hull bilge
                    // bevel, but along x): a length-end cell becomes an upside-down stair
                    // facing the open bow/stern neighbour so the end rakes instead of
                    // stepping blockily.
                    let st = HashMap::from([
                        ("facing".to_string(), place.world_cardinal(dir).to_string()),
                        ("half".to_string(), "top".to_string()),
                    ]);
                    walls
                        .place_block(ctx.editor, place.to_world(cell), BlockForm::Stairs, Some(&st), None)
                        .await;
                } else {
                    walls.place_block(ctx.editor, place.to_world(cell), BlockForm::Block, None, None).await;
                }
            }
        }
    }

    // --- Gun ports (per-ship: all trapdoor lids, or all open holes). ---
    if ports_are_trapdoors {
        for (cell, dir) in &ports {
            let facing = place.world_cardinal(*dir);
            let state = HashMap::from([
                ("facing".to_string(), facing.to_string()),
                ("half".to_string(), "bottom".to_string()),
                ("open".to_string(), "true".to_string()),
            ]);
            walls
                .place_block(ctx.editor, place.to_world(*cell), BlockForm::Trapdoor, Some(&state), None)
                .await;
        }
    }

    // --- New deck floor: top slabs over the top-row interior. ---
    let top_y = base_y + height;
    let top_slab = HashMap::from([("type".to_string(), "top".to_string())]);
    for sx in 0..length {
        let h = half_at(height, sx);
        if h < 1 {
            continue;
        }
        for z in -h..=h {
            if in_ring(height, sx, z) && !is_perimeter(height, sx, z) {
                floor
                    .place_block(ctx.editor, place.to_world(Point3D::new(sx, top_y, z)), BlockForm::Slab, Some(&top_slab), None)
                    .await;
            }
        }
    }

    // The inset top outline (→ base for the next stacked level) and the **outer rim**:
    // the outermost solid half-beam at `top_y`. Where the top row bevels in (tumblehome),
    // an outside bevel sits one block past the inset edge, so the rim is `half + 1`.
    let inset: Vec<i32> = (0..length).map(|sx| half_at(height, sx)).collect();
    let outer: Vec<i32> = (0..length)
        .map(|sx| {
            let h = half_at(height, sx);
            if h < 1 {
                return 0;
            }
            let stepped = height >= 2 && inset_at(height, sx) > inset_at(height - 1, sx);
            if stepped {
                h + 1
            } else {
                h
            }
        })
        .collect();
    (inset, outer, ports)
}
