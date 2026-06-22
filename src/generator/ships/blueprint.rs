//! Terminal-friendly diagnostics for a [`HullModel`] (+ optional [`RigModel`]) —
//! the ship analogue of `buildings_v2::blueprint::render_ascii`. Two views: a
//! top-down deck plan and a side profile. Operates in the local build frame.

use std::collections::HashSet;

use crate::geometry::{Point2D, Point3D};

use super::hull::HullModel;
use super::rig::RigModel;

/// Render a top-down deck plan, a side profile, and a stern elevation as a single
/// string. Pass the rig to overlay masts/sails (the sail reads in the stern view,
/// which looks along the length).
pub fn render_ascii(model: &HullModel, rig: Option<&RigModel>) -> String {
    let mut out = String::new();
    out.push_str(&render_top(model, rig));
    out.push('\n');
    out.push_str(&render_side(model, rig));
    out.push('\n');
    out.push_str(&render_stern(model, rig));
    out
}

/// Top-down: rows are stations (stern→bow, top→bottom), columns are the beam.
/// `+` gunwale, `#` deck, `O` hatch, `I` mast.
fn render_top(model: &HullModel, rig: Option<&RigModel>) -> String {
    let half = model.dims.beam / 2;
    let deck: HashSet<Point2D> = model.deck_cells.iter().copied().collect();
    let gunwale: HashSet<Point2D> = model.gunwale.iter().copied().collect();
    let masts: HashSet<Point2D> = rig
        .map(|r| r.masts.iter().map(|m| Point2D::new(m.base.x, m.base.z)).collect())
        .unwrap_or_default();

    let mut s = String::from("Deck (top-down, bow at bottom):\n");
    for x in 0..model.dims.length {
        for z in -half..=half {
            let p = Point2D::new(x, z);
            let ch = if masts.contains(&p) {
                'I'
            } else if model.hatch == Some(p) {
                'O'
            } else if gunwale.contains(&p) {
                '+'
            } else if deck.contains(&p) {
                '#'
            } else {
                ' '
            };
            s.push(ch);
        }
        s.push('\n');
    }
    s
}

/// Side profile: rows are y, columns are the length (bow at right). `=` deck,
/// `|` rail, `#` hull, `~` waterline, plus `I` mast, `T` yard, `V` sail.
fn render_side(model: &HullModel, rig: Option<&RigModel>) -> String {
    let hull: HashSet<Point3D> = model.hull_cells.iter().map(|p| p.local).collect();
    let deck_xs: HashSet<i32> = model.deck_cells.iter().map(|p| p.x).collect();
    let rail_xs: HashSet<i32> = model.gunwale.iter().map(|p| p.x).collect();

    // Project the rig onto the centerline plane (side view looks along z).
    let mut mast_cells: HashSet<Point2D> = HashSet::new(); // (x, y)
    let mut yard_cells: HashSet<Point2D> = HashSet::new(); // (x, y) at each yard
    let mut sail_cells: HashSet<Point2D> = HashSet::new();
    let mut top = model.deck_y + 1;
    if let Some(r) = rig {
        for m in &r.masts {
            for y in (m.foot_y + 1)..=m.top_y {
                mast_cells.insert(Point2D::new(m.base.x, y));
            }
            for yard in &m.yards {
                yard_cells.insert(Point2D::new(m.base.x, yard.y));
            }
            top = top.max(m.top_y);
        }
        for c in &r.sail_cells {
            sail_cells.insert(Point2D::new(c.x, c.y));
        }
    }

    let mut s = String::from("Side profile (bow at right, waterline ~):\n");
    for y in (0..=top).rev() {
        for x in 0..model.dims.length {
            let xy = Point2D::new(x, y);
            let ch = if yard_cells.contains(&xy) {
                'T'
            } else if mast_cells.contains(&xy) {
                'I'
            } else if sail_cells.contains(&xy) {
                'V'
            } else if y == model.deck_y + 1 && rail_xs.contains(&x) {
                '|'
            } else if y == model.deck_y && deck_xs.contains(&x) {
                '='
            } else if hull.contains(&Point3D::new(x, y, 0)) {
                '#'
            } else if y == model.waterline_y {
                '~'
            } else {
                ' '
            };
            s.push(ch);
        }
        s.push('\n');
    }
    s
}

/// Stern elevation: looking along the length at the mast station. Rows are y,
/// columns are the beam. Shows the hull cross-section plus `I` mast, `T` yard,
/// `V` sail — the view where a square sail is legible.
fn render_stern(model: &HullModel, rig: Option<&RigModel>) -> String {
    let mast = rig.and_then(|r| r.masts.first());
    let station_x = mast.map(|m| m.base.x).unwrap_or(model.dims.length / 2);
    let rib = model.ribs.iter().find(|r| r.x == station_x);

    // Hull cross-section (z, y) at this station.
    let mut hull: HashSet<Point2D> = HashSet::new();
    if let Some(r) = rib {
        for (i, &hw) in r.half_widths.iter().enumerate() {
            let y = r.bottom_y + i as i32;
            for z in -hw..=hw {
                hull.insert(Point2D::new(z, y));
            }
        }
    }
    let deck_half = rib.map(|r| r.deck_half_width()).unwrap_or(model.dims.beam / 2);

    let sail: HashSet<Point2D> = rig
        .map(|r| r.sail_cells.iter().map(|c| Point2D::new(c.z, c.y)).collect())
        .unwrap_or_default();
    let max_yard_half = mast.map(|m| m.yards.iter().map(|y| y.half).max().unwrap_or(0)).unwrap_or(0);
    let top = mast.map(|m| m.top_y).unwrap_or(model.deck_y + 1);
    let cols_half = deck_half.max(max_yard_half).max(model.dims.beam / 2);

    let mut s = String::from("Stern elevation (sail across the beam):\n");
    for y in (0..=top).rev() {
        for z in -cols_half..=cols_half {
            let zy = Point2D::new(z, y);
            let yard_here = mast.map_or(false, |m| m.yards.iter().any(|yd| yd.y == y && z.abs() <= yd.half));
            let ch = if let Some(m) = mast {
                if z == 0 && y > m.base.y && y <= m.top_y {
                    'I'
                } else if yard_here {
                    'T'
                } else if sail.contains(&zy) {
                    'V'
                } else {
                    hull_char(model, &hull, deck_half, z, y)
                }
            } else {
                hull_char(model, &hull, deck_half, z, y)
            };
            s.push(ch);
        }
        s.push('\n');
    }
    s
}

fn hull_char(model: &HullModel, hull: &HashSet<Point2D>, deck_half: i32, z: i32, y: i32) -> char {
    if y == model.deck_y + 1 && z.abs() == deck_half {
        '|'
    } else if y == model.deck_y && z.abs() <= deck_half {
        '='
    } else if hull.contains(&Point2D::new(z, y)) {
        '#'
    } else if y == model.waterline_y {
        '~'
    } else {
        ' '
    }
}
