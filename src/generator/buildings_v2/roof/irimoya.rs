//! Irimoya (入母屋) hip-and-gable roofs — a second Japanese roof style.
//!
//! An irimoya is a hipped roof whose *top* is opened up into a gable: the lower
//! roof wraps all four sides like a hip (the `sorihafu` corner curl included),
//! but along the building's long axis the central span rises to a horizontal
//! ridge closed at each end by a vertical triangular gable wall (the decorative
//! pediment). The two short-end "caps" stay hipped below those gable walls, so
//! the eaves still run unbroken around the whole footprint.
//!
//! The whole shape is baked into a single [`RoofHeightmap`]:
//! - The two long-side slopes (`d_across * rise`) run continuously end to end.
//! - In the central span (`d_along >= inset`) the surface follows the gable
//!   profile up to a long ridge.
//! - In the two end caps (`d_along < inset`) the surface follows the hip
//!   profile (`min(d_along, d_across) * rise`), tapering the ridge down to the
//!   eave so the short ends close as hips.
//!
//! [`super::irimoya_roof`] renders this heightmap with the shared hipped-slab
//! placer and then fills the two gable-end triangles with wall material.

use crate::geometry::{Point2D, Rect2D};

use super::heightmap::RoofHeightmap;
use super::hipped::HIPPED_OVERHANG;

/// Rise per horizontal block. Matches `HippedPitch::Stairs` (a full block per
/// step) so the central gable rises steeply and clearly perches above the
/// lower hipped skirt — the defining "gable on top of a hip" silhouette.
pub const IRIMOYA_RISE: f32 = 1.0;

/// Vertical lift applied to the four diagonal corner overhang cells — the
/// upturned-eave curl, identical to the hipped roof's `corner_lift`.
const CORNER_LIFT: f32 = 1.0;

/// How far the gable roof projects past its end (pediment) wall, along the ridge
/// axis. The gable surface extends this many blocks beyond the pediment toward
/// the short ends — the verge overhang — while the pediment wall stays at the
/// inset plane. A hip cap is always kept at least 1 deep, so tiny footprints
/// (inset <= 1) simply get no gable overhang.
pub const GABLE_OVERHANG: i32 = 1;

/// Depth (from each short end, along the ridge) of the gable verge-overhang
/// plane: one block past the pediment at `inset`, but never closer to the end
/// than 1 (so a hip cap always remains). When this equals `inset` there is no
/// overhang to bracket.
pub fn verge_depth(inset: i32) -> i32 {
    (inset - GABLE_OVERHANG).max(1)
}

/// Axis the gable ridge runs along (the building's long axis).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LongAxis {
    X, // ridge along world X, slopes fall off in Z
    Z, // ridge along world Z, slopes fall off in X
}

/// Pick the ridge (long) axis: the longer footprint dimension. Square footprints
/// default to X — irimoya is unambiguous either way on a square.
pub fn pick_long_axis(rect: &Rect2D) -> LongAxis {
    if rect.width() > rect.length() {
        LongAxis::Z
    } else {
        LongAxis::X
    }
}

/// Inset of each gable end from the rect's short edges, measured along the ridge
/// axis. The central `[inset, along_extent - inset]` span is gabled; the two
/// `inset`-deep end caps stay hipped. Aims for the central ~half to be gabled
/// while always leaving at least a 1-deep hip cap and a 1-cell ridge span.
pub fn gable_inset(rect: &Rect2D, axis: LongAxis) -> i32 {
    let along_extent = match axis {
        LongAxis::X => rect.max().x - rect.min().x,
        LongAxis::Z => rect.max().y - rect.min().y,
    };
    // The gable spans `along_extent - 2*inset`; keep it >= 1, so inset <= (extent-1)/2.
    let max_inset = ((along_extent - 1) / 2).max(1);
    (along_extent / 4).clamp(1, max_inset)
}

/// Build the irimoya heightmap for a single rect (see module docs for the
/// piecewise hip/gable surface). `others` are the building's other top-floor
/// rects, used to suppress the corner curl at rect-rect junctions exactly as
/// [`super::hipped::hipped_heightmap`] does.
pub fn irimoya_heightmap(
    rect: &Rect2D,
    others: &[&Rect2D],
    rise: f32,
    axis: LongAxis,
    inset: i32,
) -> RoofHeightmap {
    let min = rect.min();
    let max = rect.max();
    let overhang = HIPPED_OVERHANG;

    let hm_min_x = min.x - overhang;
    let hm_max_x = max.x + overhang;
    let hm_min_z = min.y - overhang;
    let hm_max_z = max.y + overhang;

    let width = (hm_max_x - hm_min_x + 1) as usize;
    let depth = (hm_max_z - hm_min_z + 1) as usize;

    let mut hm = RoofHeightmap::new(hm_min_x, hm_min_z, width, depth);

    // Cap the ridge to a whole-block multiple, mirroring the hipped roof: this
    // keeps a clean top-slab ridge line rather than a lone bottom slab perched
    // above the surrounding tiles.
    let across_extent = match axis {
        LongAxis::X => max.y - min.y,
        LongAxis::Z => max.x - min.x,
    };
    let cap_h = ((across_extent / 2) as f32 * rise).floor();

    // The gable surface reaches one block past the pediment (at `inset`) for the
    // verge overhang, but always leave a >= 1-deep hip cap at the very ends.
    let gable_threshold = verge_depth(inset);

    for x in hm_min_x..=hm_max_x {
        for z in hm_min_z..=hm_max_z {
            let dx = (x - min.x).min(max.x - x);
            let dz = (z - min.y).min(max.y - z);
            let (d_along, d_across) = match axis {
                LongAxis::X => (dx, dz),
                LongAxis::Z => (dz, dx),
            };

            // Central span (+ verge overhang): gable profile (a long ridge).
            // End caps: hip profile, tapering the ridge down to the eave so the
            // short ends close as hips.
            let dist = if d_along >= gable_threshold {
                d_across
            } else {
                d_along.min(d_across)
            };
            let h = (dist as f32 * rise).min(cap_h);
            hm.set(x, z, h);
        }
    }

    // Sorihafu corner curl: lift the four diagonal corner overhang cells. Skip a
    // corner that lands inside another rect (it's an inner corner there).
    for (ox, oz) in [
        (-overhang, -overhang),
        (overhang, -overhang),
        (-overhang, overhang),
        (overhang, overhang),
    ] {
        let cx = if ox < 0 { min.x + ox } else { max.x + ox };
        let cz = if oz < 0 { min.y + oz } else { max.y + oz };
        let corner = Point2D::new(cx, cz);
        if others.iter().any(|r| r.contains(corner)) {
            continue;
        }
        let base = hm.get(cx, cz);
        hm.set(cx, cz, base + CORNER_LIFT);
    }

    hm
}
