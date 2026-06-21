//! Hipped roofs with upturned eave corners — the Japanese cultural roof.
//!
//! Unlike a gable, all four sides slope toward a central ridge (or a single
//! apex for square rects). The signature of this style is the *sorihafu*
//! corner curl: the four diagonal corner cells of the overhang sit higher
//! than the cardinal eave, lifting the corners of the roof above the wall
//! line. The lift is baked directly into the heightmap so the dedicated
//! slab placer (or the shared stairs placer) renders the corner higher than
//! its cardinal neighbours from any viewing angle.

use crate::geometry::{Point2D, Rect2D};

use super::heightmap::RoofHeightmap;

/// Overhang depth in blocks for hipped roofs. Single-block overhang keeps the
/// corner upturn close to the building so it reads as "lifted corners" rather
/// than "flying eaves" on small footprints.
pub const HIPPED_OVERHANG: i32 = 1;

/// Surface variant: half-step slab staircase or full-block stair staircase.
/// Slab is the low-profile kawara-tile look; Stairs is steeper and closer to
/// a stair-stepped shrine roof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HippedPitch {
    Slab,
    Stairs,
}

impl HippedPitch {
    /// Rise per horizontal block. 0.5 for slab (half-step), 1.0 for stairs.
    pub fn rise(&self) -> f32 {
        match self {
            HippedPitch::Slab => 0.5,
            HippedPitch::Stairs => 1.0,
        }
    }

    /// Extra height added to the four diagonal corner overhang cells. Both
    /// variants lift by one block — for slab that places the corner pair
    /// one block above the cardinal rim; for stairs it's a one-block stair
    /// step above the cardinal eave.
    pub fn corner_lift(&self) -> f32 {
        match self {
            HippedPitch::Slab => 1.0,
            HippedPitch::Stairs => 1.0,
        }
    }
}

/// Build a hipped-roof heightmap for a single rect.
///
/// Each cell's height is the signed distance to the nearest rect edge,
/// scaled by the pitch's rise:
/// - Interior cells: positive, peaking at the ridge line / apex.
/// - Overhang cells (outside the rect, within [`HIPPED_OVERHANG`]): negative,
///   producing the eave drop.
///
/// The four diagonal corner cells of the outermost overhang ring are then
/// lifted by the pitch's `corner_lift` — that's the curl. Corners that fall
/// inside any of `others` (other rects in the same building) are NOT lifted:
/// at a rect-rect junction the lifted cell is geometrically an inner corner,
/// not an outer one, so the curl there would show up as a spurious bump on
/// the neighbour's roof.
pub fn hipped_heightmap(rect: &Rect2D, others: &[&Rect2D], pitch: HippedPitch) -> RoofHeightmap {
    let min = rect.min();
    let max = rect.max();
    let overhang = HIPPED_OVERHANG;
    let rise = pitch.rise();

    let hm_min_x = min.x - overhang;
    let hm_max_x = max.x + overhang;
    let hm_min_z = min.y - overhang;
    let hm_max_z = max.y + overhang;

    let width = (hm_max_x - hm_min_x + 1) as usize;
    let depth = (hm_max_z - hm_min_z + 1) as usize;

    let mut hm = RoofHeightmap::new(hm_min_x, hm_min_z, width, depth);

    // Cap the apex to the nearest whole-block multiple. For slab pitch this
    // prevents the lone bottom slab perched above the surrounding top-slab
    // ring (a "slab on top"). For stairs pitch the natural max is already a
    // whole step so the cap is a no-op.
    let short_dim = (max.x - min.x).min(max.y - min.y);
    let natural_max_h = (short_dim / 2) as f32 * rise;
    let cap_h = natural_max_h.floor();

    for x in hm_min_x..=hm_max_x {
        for z in hm_min_z..=hm_max_z {
            let dx = (x - min.x).min(max.x - x);
            let dz = (z - min.y).min(max.y - z);
            let dist = dx.min(dz);
            let h = (dist as f32 * rise).min(cap_h);
            hm.set(x, z, h);
        }
    }

    if overhang >= 1 {
        let lift = pitch.corner_lift();
        for (ox, oz) in [(-overhang, -overhang), (overhang, -overhang),
                         (-overhang, overhang), (overhang, overhang)] {
            let cx = if ox < 0 { min.x + ox } else { max.x + ox };
            let cz = if oz < 0 { min.y + oz } else { max.y + oz };
            let corner = Point2D::new(cx, cz);
            if others.iter().any(|r| r.contains(corner)) {
                continue;
            }
            let base = hm.get(cx, cz);
            hm.set(cx, cz, base + lift);
        }
    }

    hm
}
