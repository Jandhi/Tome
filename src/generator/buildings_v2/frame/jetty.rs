//! Jetty transform: grow a frame's upper floors outward over the ground floor.
//! Gated on plot bounds and non-overlap of the grown extents; falls back to the
//! input frame unchanged when ineligible.

use crate::geometry::{Point2D, Rect2D};

use super::model::Frame;

/// Which of a rect's four sides may grow outward when jettying. A side is
/// growable only when it faces open air; a side that abuts another rect (a
/// shared seam) stays flush so the overhang doesn't overlap the neighbour.
struct GrowMask {
    west: bool,  // -x
    east: bool,  // +x
    north: bool, // -y
    south: bool, // +y
}

/// For each rect, compute which sides face open air (growable) vs. abut another
/// rect (blocked). Adjacency mirrors `find_boundaries`: two rects share a side
/// when their edges touch (offset by 1) and their spans on the perpendicular
/// axis overlap. Even a partial overlap blocks the whole side — that keeps each
/// grown extent a single rectangle (the conservative v1 choice).
fn growable_sides(rects: &[Rect2D]) -> Vec<GrowMask> {
    let mut masks: Vec<GrowMask> = rects.iter()
        .map(|_| GrowMask { west: true, east: true, north: true, south: true })
        .collect();

    for i in 0..rects.len() {
        for j in 0..rects.len() {
            if i == j { continue; }
            let a = &rects[i];
            let b = &rects[j];
            let z_overlap = a.min().y.max(b.min().y) <= a.max().y.min(b.max().y);
            let x_overlap = a.min().x.max(b.min().x) <= a.max().x.min(b.max().x);

            if a.max().x + 1 == b.min().x && z_overlap { masks[i].east = false; }
            if b.max().x + 1 == a.min().x && z_overlap { masks[i].west = false; }
            if a.max().y + 1 == b.min().y && x_overlap { masks[i].south = false; }
            if b.max().y + 1 == a.min().y && x_overlap { masks[i].north = false; }
        }
    }
    masks
}

/// Eligibility gate + transform: returns a new Frame whose upper floors overhang
/// the ground floor. Each rect grows its open-air sides by 1 block on every
/// floor above the ground; sides shared with an adjacent rect stay flush so the
/// seams stay aligned and no extents overlap. Single-rect buildings are the
/// trivial case (all four sides grow). Falls back to the input frame unchanged
/// when there's only one floor or the grown extents wouldn't fit `plot_bounds`.
pub fn apply_jetty(frame: Frame, plot_bounds: &Rect2D) -> Frame {
    if frame.max_floors() < 2 { return frame; }

    let rects = frame.footprint().rects();
    let masks = growable_sides(rects);

    // Grown upper-floor extent per rect: expand each open-air side by 1.
    let grown: Vec<Rect2D> = rects.iter().zip(&masks).map(|(r, m)| {
        Rect2D::from_points(
            Point2D::new(
                r.min().x - if m.west { 1 } else { 0 },
                r.min().y - if m.north { 1 } else { 0 },
            ),
            Point2D::new(
                r.max().x + if m.east { 1 } else { 0 },
                r.max().y + if m.south { 1 } else { 0 },
            ),
        )
    }).collect();

    // Only rects that actually have an upper floor (count >= 2) contribute a
    // grown extent worth checking.
    let upper: Vec<(usize, Rect2D)> = grown.iter().enumerate()
        .filter(|(i, _)| frame.floor_counts()[*i] >= 2)
        .map(|(i, g)| (i, *g))
        .collect();

    // A single overflowing extent disables jettying for the whole building.
    if upper.iter().any(|(_, g)| !plot_bounds.contains_rect(g)) {
        return frame;
    }

    // Growth must not make two upper-floor extents share cells — e.g. two wings
    // on the same side growing into a 1-cell gap. Adjacent rects stay flush at
    // their seam so they only touch (never overlap); a true overlap means a
    // gap closed, which `find_boundaries`/`compute_room_interior` can't model.
    // Bail to flush rather than produce overlapping rooms.
    let overlaps = upper.iter().enumerate().any(|(a, (_, ga))| {
        upper.iter().skip(a + 1).any(|(_, gb)| {
            ga.min().x <= gb.max().x && gb.min().x <= ga.max().x
                && ga.min().y <= gb.max().y && gb.min().y <= ga.max().y
        })
    });
    if overlaps {
        return frame;
    }

    // Ground floor keeps the footprint rect; floors 1..count use the grown
    // rect. Each rect keeps its own floor count (1-floor wings never grow).
    let max_floors = frame.max_floors();
    let extents: Vec<Vec<Option<Rect2D>>> = (0..frame.rect_count()).map(|i| {
        let count = frame.floor_counts()[i];
        let ground = rects[i];
        let upper = grown[i];
        (0..max_floors).map(|f| {
            if f >= count { None }
            else if f == 0 { Some(ground) }
            else { Some(upper) }
        }).collect()
    }).collect();

    Frame::with_per_floor_extents(
        frame.footprint().clone(),
        frame.base_y(),
        extents,
        frame.wall_height(),
    )
}
